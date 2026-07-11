pub mod hls;
pub mod http;

use crate::types::{DownloadOptions, Format};
use anyhow::{Context, Result};
use reqwest::Client;

#[cfg(not(target_arch = "wasm32"))]
use reqwest::cookie::Jar;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

pub use hls::HlsDownloader;
pub use http::HttpDownloader;

#[cfg(not(target_arch = "wasm32"))]
fn load_netscape_cookies(path: &str, jar: &Arc<Jar>) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read cookies file: {}", path))?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Netscape format: domain  flag  path  secure  expiry  name  value
        let fields: Vec<&str> = line.splitn(7, '\t').collect();
        if fields.len() < 7 {
            continue;
        }
        let domain = fields[0].trim_start_matches('.');
        let secure = fields[3].eq_ignore_ascii_case("true");
        let name = fields[5];
        let value = fields[6];
        let scheme = if secure { "https" } else { "http" };
        let url = format!("{}://{}/", scheme, domain);
        if let Ok(parsed) = url.parse::<reqwest::Url>() {
            jar.add_cookie_str(&format!("{}={}", name, value), &parsed);
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn build_client(opts: &DownloadOptions) -> Result<Client> {
    let mut builder = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .cookie_store(true);

    if let Some(proxy_url) = &opts.proxy {
        let proxy = reqwest::Proxy::all(proxy_url.as_str()).context("Invalid proxy URL")?;
        builder = builder.proxy(proxy);
    }

    if let Some(cookies_file) = &opts.cookies_file {
        let jar = Arc::new(Jar::default());
        load_netscape_cookies(cookies_file, &jar)?;
        builder = builder.cookie_provider(jar);
    }

    builder.build().context("Failed to build HTTP client")
}

#[cfg(target_arch = "wasm32")]
pub fn build_client(_opts: &DownloadOptions) -> Result<Client> {
    Client::builder()
        .build()
        .context("Failed to build HTTP client")
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn download_format(
    client: &Client,
    fmt: &Format,
    output_path: &Path,
    label: &str,
    opts: &DownloadOptions,
) -> Result<()> {
    match fmt.protocol.as_str() {
        "hls" | "m3u8" | "m3u8_native" => {
            let dl = HlsDownloader::new(
                client.clone(),
                opts.concurrent_fragments,
                opts.retries,
                opts.no_progress,
                opts.quiet,
            );
            dl.download(&fmt.url, output_path, label).await?;
        }
        _ => {
            // Default: HTTP download
            let dl = HttpDownloader::new(
                client.clone(),
                opts.retries,
                opts.rate_limit,
                opts.no_progress,
                opts.quiet,
            );
            dl.download(&fmt.url, output_path, label, &fmt.http_headers)
                .await?;
        }
    }
    Ok(())
}
