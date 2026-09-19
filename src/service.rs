use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Category {
    Beauty,
    #[default]
    Landscape,
}
impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Beauty => "美女",
            Self::Landscape => "风景",
        }
    }
    fn query(self) -> (&'static str, &'static str) {
        match self {
            Self::Beauty => ("women", "001"),
            Self::Landscape => ("landscape", "100"),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Wallpaper {
    pub id: String,
    pub path: String,
    pub dimension_x: u32,
    pub dimension_y: u32,
}
#[derive(Deserialize)]
pub struct Page {
    pub data: Vec<Wallpaper>,
    pub meta: Meta,
}
#[derive(Deserialize)]
pub struct Meta {
    pub last_page: u32,
}
fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("WallpaperDesktop/", env!("CARGO_PKG_VERSION")))
        .build()?)
}
pub fn search(category: Category, page: u32, random: bool) -> Result<Page> {
    crate::sources::search(category, page, random)
}
fn allowed_image_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
            host == "wallhaven.cc"
                || host.ends_with(".wallhaven.cc")
                || host == "upload.wikimedia.org"
                || host == "www.bing.com"
                || host == "cn.bing.com"
        })
}
pub fn wallhaven_search(category: Category, page: u32, random: bool) -> Result<Page> {
    let (q, categories) = category.query();
    let mut result: Page = client()?
        .get("https://wallhaven.cc/api/v1/search")
        .timeout(Duration::from_secs(15))
        .query(&[
            ("q", q),
            ("categories", categories),
            ("purity", "100"),
            ("atleast", "3840x2160"),
            ("sorting", if random { "random" } else { "favorites" }),
            ("order", "desc"),
            ("page", &page.to_string()),
        ])
        .send()
        .context("无法连接 Wallhaven，请检查网络后重试")?
        .error_for_status()
        .context("壁纸服务暂时不可用或请求过于频繁")?
        .json()?;
    result
        .data
        .retain(|w| w.dimension_x >= 3840 && w.dimension_y >= 2160);
    Ok(result)
}
pub fn download(w: &Wallpaper) -> Result<PathBuf> {
    ensure!(
        !w.id.is_empty() && w.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "无效的壁纸编号"
    );
    let url = reqwest::Url::parse(&w.path)?;
    ensure!(allowed_image_url(&url), "无效的图片地址");
    let extension_path = if w.id.starts_with("bing-") {
        "image.jpg"
    } else {
        url.path()
    };
    let extension = extension_path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let ext = match extension.as_str() {
        "png" => "png",
        "webp" => "webp",
        "jpg" | "jpeg" => "jpg",
        _ => anyhow::bail!("不支持的图片格式"),
    };
    let folder = dirs::picture_dir()
        .or_else(dirs::download_dir)
        .context("找不到图片目录")?
        .join("Wallpaper");
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!(
        "{}{}.{}",
        if w.id.contains('-') { "" } else { "wallhaven-" },
        w.id,
        ext
    ));
    if let Ok(bytes) = std::fs::read(&path)
        && validate(&bytes).is_ok()
    {
        return Ok(path);
    }
    let response = client()?.get(url).send()?.error_for_status()?;
    let mut bytes = Vec::new();
    response
        .take(80 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 80 * 1024 * 1024, "图片超过 80 MB 限制");
    validate(&bytes)?;
    save_image(&path, &bytes)?;
    Ok(path)
}
fn save_image(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().context("无效的图片目录")?)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}
pub(crate) fn validate(bytes: &[u8]) -> Result<()> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions().context("下载内容不是有效图片")?;
    ensure!(width >= 3840 && height >= 2160, "图片实际分辨率不足 4K");
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().context("图片数据不完整或无法解码")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_trusted_source_image_hosts_are_allowed() {
        for url in [
            "https://w.wallhaven.cc/full/a.jpg",
            "https://upload.wikimedia.org/a.jpg",
            "https://www.bing.com/th?id=OHR.Test_UHD.jpg",
        ] {
            assert!(allowed_image_url(&reqwest::Url::parse(url).unwrap()));
        }
        for url in [
            "http://www.bing.com/a.jpg",
            "https://upload.wikimedia.org.evil.example/a.jpg",
            "https://example.com/a.jpg",
        ] {
            assert!(!allowed_image_url(&reqwest::Url::parse(url).unwrap()));
        }
    }
    #[test]
    fn concurrent_saves_do_not_share_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallpaper.png");
        std::thread::scope(|scope| {
            for byte in [1, 2] {
                let path = &path;
                scope.spawn(move || save_image(path, &vec![byte; 4096]).unwrap());
            }
        });
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 4096);
        assert!(bytes.iter().all(|b| *b == bytes[0]));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn rejects_truncated_4k_image() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(3840, 2160)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let bytes = bytes.into_inner();
        assert!(validate(&bytes[..bytes.len() / 2]).is_err());
    }
    #[test]
    fn categories_are_distinct() {
        assert_eq!(Category::Beauty.query(), ("women", "001"));
        assert_eq!(Category::Landscape.query(), ("landscape", "100"));
    }
    #[test]
    fn rejects_invalid_and_small_images() {
        assert!(validate(b"<html>error</html>").is_err());
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(32, 32)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert!(validate(bytes.get_ref()).is_err());
    }
    #[test]
    fn accepts_real_4k_dimensions() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(3840, 2160)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert!(validate(bytes.get_ref()).is_ok());
    }
}
