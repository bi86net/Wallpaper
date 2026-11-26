use crate::service::{Category, Meta, Page, Wallpaper};
use anyhow::{Result, ensure};
use scraper::{Html, Selector};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::Duration;

pub fn search(category: Category, page: u32) -> Result<Page> {
    let query = match category {
        Category::Landscape => "自然风景 2k 壁纸",
        Category::Beauty => "美女 2k 壁纸",
    };
    let page = page.clamp(1, 20);
    let response = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0")
        .build()?
        .get("https://cn.bing.com/images/search")
        .query(&[
            ("q", query),
            ("adlt", "strict"),
            ("first", &((page - 1) * 35 + 1).to_string()),
            ("count", "35"),
        ])
        .send()?
        .error_for_status()?;
    // Never interpret a consent or challenge page as image results.
    parse(&response.text()?, page)
}
pub fn parse(html: &str, page: u32) -> Result<Page> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a.iusc[m]").unwrap();
    let mut data = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for element in document.select(&selector) {
        let Some(metadata) = element.value().attr("m") else {
            continue;
        };
        let Ok(metadata) = serde_json::from_str::<Value>(metadata) else {
            continue;
        };
        let Some(original) = metadata["murl"].as_str() else {
            continue;
        };
        let Ok(mut url) = reqwest::Url::parse(original) else {
            continue;
        };
        if !crate::remote_image::allowed_url(&url) {
            continue;
        }
        if [".gif", ".svg", ".avif"]
            .iter()
            .any(|ext| url.path().to_ascii_lowercase().ends_with(ext))
        {
            continue;
        }
        url.set_fragment(None);
        let path = url.to_string();
        let id = format!("bingsearch-{:x}", Sha256::digest(path.as_bytes()));
        if !seen.insert(id.clone()) {
            continue;
        }
        // Search snippets often omit dimensions. Never trust them; download() validates the file.
        data.push(Wallpaper {
            id,
            path,
            dimension_x: 0,
            dimension_y: 0,
        });
    }
    ensure!(
        !data.is_empty(),
        "Bing 搜图未返回可用原图，可能暂无结果或需要验证，已尝试备用源"
    );
    Ok(Page {
        data,
        meta: Meta {
            last_page: page.saturating_add(1).min(20),
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_original_not_thumbnail_and_deduplicates() {
        let html = r#"<a class="iusc" m="{&quot;murl&quot;:&quot;https://images.example.org/original.jpg?a=1&amp;b=2&quot;,&quot;turl&quot;:&quot;https://thumb.example.org/small.jpg&quot;}"></a>
<a class="iusc" m='{"murl":"https://images.example.org/original.jpg?a=1&b=2#section"}'></a>"#;
        let result = parse(html, 1).unwrap();
        assert_eq!(result.data.len(), 1);
        assert_eq!(
            result.data[0].path,
            "https://images.example.org/original.jpg?a=1&b=2"
        );
        assert!(result.data[0].id.starts_with("bingsearch-"));
    }
    #[test]
    fn rejects_challenge_and_private_urls() {
        assert!(parse("<html>Please verify</html>", 1).is_err());
        assert!(
            parse(
                r#"<a class="iusc" m='{"murl":"https://127.0.0.1/private.jpg"}'></a>"#,
                1
            )
            .is_err()
        );
    }
    #[test]
    #[ignore = "Live Bing search and original-image download"]
    fn live_bing_original() {
        let results = search(Category::Landscape, 1).unwrap();
        println!("Bing search: {} original image links", results.data.len());
        let mut failures = Vec::new();
        for item in results.data.iter().take(10) {
            println!("Trying {}", item.path);
            match crate::remote_image::download(&item.path).and_then(|bytes| {
                crate::service::validate(&bytes)?;
                Ok(bytes.len())
            }) {
                Ok(size) => {
                    println!("Bing original passed full decoding and 2K validation ({size} bytes)");
                    return;
                }
                Err(e) => failures.push(format!("{e:#}")),
            }
        }
        panic!("No working 2K original: {}", failures.join("; "));
    }
}
