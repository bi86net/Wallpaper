use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    net::{IpAddr, ToSocketAddrs},
    time::Duration,
};
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 198 && (b == 18 || b == 19))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] == 0x0db8 || s[1] < 0x0200))
                && s[0] != 0x2002
        }
    }
}
pub fn allowed_url(url: &reqwest::Url) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if let Ok(ip) = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
    {
        return public_ip(ip);
    }
    host.contains('.')
        && ![
            "localhost",
            ".localhost",
            ".local",
            ".internal",
            ".home",
            ".lan",
        ]
        .iter()
        .any(|suffix| host.ends_with(suffix))
}
pub fn download(original: &str) -> Result<Vec<u8>> {
    let mut url = reqwest::Url::parse(original)?;
    for _ in 0..5 {
        ensure!(allowed_url(&url), "不支持的原图地址");
        let host = url.host_str().context("原图地址缺少域名")?;
        let addresses: Vec<_> = (host, 443)
            .to_socket_addrs()?
            .filter(|address| public_ip(address.ip()))
            .collect();
        ensure!(!addresses.is_empty(), "原图地址不属于公共网络");
        // Pin checked DNS results and validate every redirect; never fetch local network endpoints.
        let response = reqwest::blocking::Client::builder()
            .no_proxy()
            .resolve_to_addrs(host, &addresses)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!(
                "Mozilla/5.0 WallpaperDesktop/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?
            .get(url.clone())
            .send()?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .context("重定向缺少地址")?
                .to_str()?;
            url = url.join(location)?;
            continue;
        }
        let response = response.error_for_status()?;
        ensure!(
            response.content_length().unwrap_or(0) <= 80 * 1024 * 1024,
            "图片超过 80 MB 限制"
        );
        let mut bytes = Vec::new();
        response
            .take(80 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 80 * 1024 * 1024, "图片超过 80 MB 限制");
        return Ok(bytes);
    }
    anyhow::bail!("原图重定向次数过多")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_public_destinations() {
        for address in [
            "https://127.0.0.1/a",
            "https://10.0.0.1/a",
            "https://[::1]/a",
            "https://[::ffff:127.0.0.1]/a",
            "https://100.64.0.1/a",
            "https://printer.local/a",
            "file:///tmp/a",
            "http://example.org/a",
            "https://user:pass@example.org/a",
            "https://example.org:8080/a",
        ] {
            assert!(
                !allowed_url(&reqwest::Url::parse(address).unwrap()),
                "{address}"
            );
        }
        assert!(allowed_url(
            &reqwest::Url::parse("https://images.example.org/a").unwrap()
        ));
    }
}
