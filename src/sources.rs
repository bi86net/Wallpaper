use crate::service::{Category, Meta, Page, Wallpaper};
use anyhow::{Context, Result};
use rand::Rng;
use reqwest::blocking::Client;
use serde_json::Value;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Source {
    BingSearch,
    Wallhaven,
    Commons,
    Bing,
}
impl Source {
    fn name(self) -> &'static str {
        match self {
            Self::BingSearch => "Bing 图片搜索",
            Self::Wallhaven => "Wallhaven",
            Self::Commons => "Wikimedia Commons",
            Self::Bing => "Bing",
        }
    }
}
static COOLDOWNS: Mutex<Vec<(Source, Instant)>> = Mutex::new(Vec::new());
fn cooling(source: Source) -> bool {
    COOLDOWNS
        .lock()
        .unwrap()
        .iter()
        .any(|(s, until)| *s == source && Instant::now() < *until)
}
fn request(source: Source, category: Category, page: u32, random: bool) -> Result<Page> {
    anyhow::ensure!(
        !cooling(source),
        "{} 暂时不可用，冷却后自动重试",
        source.name()
    );
    let result = match source {
        Source::BingSearch => crate::bing_search::search(
            category,
            if random {
                rand::thread_rng().gen_range(1..=5)
            } else {
                page
            },
        ),
        Source::Wallhaven => crate::service::wallhaven_search(category, page, random),
        Source::Commons => commons(category, page),
        Source::Bing => bing(page),
    };
    if result.is_err() {
        let mut entries = COOLDOWNS.lock().unwrap();
        entries.retain(|(s, _)| *s != source);
        entries.push((source, Instant::now() + Duration::from_secs(300)));
    }
    result.with_context(|| source.name())
}
fn merge(results: Vec<Result<Page>>) -> Result<Page> {
    let mut batches = Vec::new();
    let mut last_page = 1;
    let mut errors = Vec::new();
    let mut successes = 0;
    for result in results {
        match result {
            Ok(page) => {
                successes += 1;
                last_page = last_page.max(page.meta.last_page);
                batches.push(page.data.into_iter());
            }
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    anyhow::ensure!(
        successes > 0,
        "所有图片源暂时不可用，请稍后重试。{}",
        errors.join("；")
    );
    let mut data = Vec::new();
    loop {
        let before = data.len();
        for batch in &mut batches {
            if let Some(item) = batch.next() {
                data.push(item);
            }
        }
        if data.len() == before {
            break;
        }
    }
    let mut ids = std::collections::HashSet::new();
    data.retain(|w| ids.insert(w.id.clone()));
    Ok(Page {
        data,
        meta: Meta { last_page },
    })
}
pub fn search(category: Category, page: u32, random: bool) -> Result<Page> {
    let mut sources = vec![Source::BingSearch, Source::Wallhaven, Source::Commons];
    if category == Category::Landscape {
        sources.push(Source::Bing);
    }
    let results = std::thread::scope(|scope| {
        let workers: Vec<_> = sources
            .into_iter()
            .map(|source| scope.spawn(move || request(source, category, page, random)))
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("图片源请求异常")))
            })
            .collect()
    });
    merge(results)
}
fn client() -> Result<Client> {
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(15))
        .user_agent(concat!(
            "WallpaperDesktop/",
            env!("CARGO_PKG_VERSION"),
            " (personal desktop wallpaper app)"
        ))
        .build()?)
}
fn commons(category: Category, page: u32) -> Result<Page> {
    let query = match category {
        Category::Beauty => {
            "woman fashion photograph filetype:bitmap filew:>2559 fileh:>1439 -nude -naked -painting -drawing"
        }
        Category::Landscape => {
            "landscape nature filetype:bitmap filew:>2559 fileh:>1439 -map -drawing"
        }
    };
    let value: Value = client()?
        .get("https://commons.wikimedia.org/w/api.php")
        .query(&[
            ("action", "query"),
            ("format", "json"),
            ("generator", "search"),
            ("gsrsearch", query),
            ("gsrnamespace", "6"),
            ("gsrlimit", "24"),
            (
                "gsroffset",
                &((page.saturating_sub(1) % 20) * 24).to_string(),
            ),
            ("prop", "imageinfo"),
            ("iiprop", "url|size|mime"),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    parse_commons(&value, page)
}
fn parse_commons(value: &Value, page: u32) -> Result<Page> {
    anyhow::ensure!(
        value.get("error").is_none(),
        "Commons 返回错误：{}",
        value["error"]
    );
    anyhow::ensure!(
        value.get("query").is_some() || value.get("batchcomplete").is_some(),
        "Commons 返回格式异常"
    );
    let mut data = Vec::new();
    if let Some(pages) = value["query"]["pages"].as_object() {
        for item in pages.values() {
            let info = &item["imageinfo"][0];
            let (Some(id), Some(url), Some(width), Some(height)) = (
                item["pageid"].as_u64(),
                info["url"].as_str(),
                info["width"].as_u64(),
                info["height"].as_u64(),
            ) else {
                continue;
            };
            if width < u64::from(crate::service::MIN_WIDTH)
                || height < u64::from(crate::service::MIN_HEIGHT)
                || width > u32::MAX as u64
                || height > u32::MAX as u64
            {
                continue;
            }
            if !matches!(
                info["mime"].as_str(),
                Some("image/jpeg" | "image/png" | "image/webp")
            ) {
                continue;
            }
            data.push(Wallpaper {
                id: format!("commons-{id}"),
                path: url.into(),
                dimension_x: width as u32,
                dimension_y: height as u32,
            });
        }
    }
    Ok(Page {
        data,
        meta: Meta {
            last_page: if value.get("continue").is_some() {
                page.saturating_add(1).min(20)
            } else {
                page.max(1)
            },
        },
    })
}
fn bing(page: u32) -> Result<Page> {
    let markets = ["zh-CN", "en-US", "ja-JP", "de-DE", "en-GB"];
    let value: Value = client()?
        .get("https://www.bing.com/HPImageArchive.aspx")
        .query(&[
            ("format", "js"),
            ("idx", "0"),
            ("n", "8"),
            (
                "mkt",
                markets[(page.saturating_sub(1) as usize) % markets.len()],
            ),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    parse_bing(&value)
}
fn parse_bing(value: &Value) -> Result<Page> {
    let images = value["images"].as_array().context("Bing 返回格式异常")?;
    let mut data = Vec::new();
    for item in images {
        let (Some(hash), Some(base)) = (item["hsh"].as_str(), item["urlbase"].as_str()) else {
            continue;
        };
        if hash.is_empty()
            || !hash.chars().all(|c| c.is_ascii_alphanumeric())
            || !base.starts_with("/th?id=OHR.")
        {
            continue;
        }
        // UHD is only a requested size; the downloaded bytes must still pass real 2K validation.
        data.push(Wallpaper {
            id: format!("bing-{hash}"),
            path: format!("https://www.bing.com{base}_UHD.jpg"),
            dimension_x: 3840,
            dimension_y: 2160,
        });
    }
    Ok(Page {
        data,
        meta: Meta { last_page: 5 },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_images_are_interleaved_before_primary_batch_finishes() {
        fn item(id: &str) -> Wallpaper {
            Wallpaper {
                id: id.into(),
                path: String::new(),
                dimension_x: 3840,
                dimension_y: 2160,
            }
        }
        let result = merge(vec![
            Ok(Page {
                data: vec![item("a"), item("b"), item("c")],
                meta: Meta { last_page: 1 },
            }),
            Ok(Page {
                data: vec![item("backup")],
                meta: Meta { last_page: 1 },
            }),
        ])
        .unwrap();
        assert_eq!(result.data[1].id, "backup");
    }
    #[test]
    #[ignore = "Live network integration check; run explicitly"]
    fn live_backup_sources() {
        use std::io::Read;
        for category in [Category::Beauty, Category::Landscape] {
            let page = commons(category, 1).unwrap();
            assert!(!page.data.is_empty(), "Commons returned no 2K images");
            println!(
                "Commons {:?}: {} verified metadata results",
                category,
                page.data.len()
            );
        }
        let page = bing(1).unwrap();
        assert!(!page.data.is_empty());
        let response = client()
            .unwrap()
            .get(&page.data[0].path)
            .timeout(Duration::from_secs(60))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
        let mut bytes = Vec::new();
        response
            .take(80 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        crate::service::validate(&bytes).unwrap();
        println!("Bing UHD download passed image decoding and 2K dimensions check");
    }
    #[test]
    fn failed_primary_does_not_discard_backup() {
        let backup = parse_bing(
            &serde_json::json!({"images":[{"hsh":"abc123","urlbase":"/th?id=OHR.Example"}]}),
        )
        .unwrap();
        let merged = merge(vec![Err(anyhow::anyhow!("HTTP 521")), Ok(backup)]).unwrap();
        assert_eq!(merged.data[0].id, "bing-abc123");
    }
    #[test]
    fn commons_rejects_small_and_non_photographic_formats() {
        let result = parse_commons(&serde_json::json!({"query":{"pages":{
            "1":{"pageid":1,"imageinfo":[{"width":5000,"height":3000,"mime":"image/jpeg","url":"https://upload.wikimedia.org/a.jpg"}]},
            "2":{"pageid":2,"imageinfo":[{"width":100,"height":100,"mime":"image/jpeg","url":"https://upload.wikimedia.org/b.jpg"}]},
            "3":{"pageid":3,"imageinfo":[{"width":5000,"height":3000,"mime":"image/svg+xml","url":"https://upload.wikimedia.org/c.svg"}]}
        }}}),1).unwrap();
        assert_eq!(result.data.len(), 1);
        assert_eq!(result.data[0].id, "commons-1");
    }
    #[test]
    fn all_sources_failed_is_reported() {
        assert!(
            merge(vec![
                Err(anyhow::anyhow!("521")),
                Err(anyhow::anyhow!("timeout"))
            ])
            .is_err()
        );
    }
}
