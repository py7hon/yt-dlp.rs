pub mod http;
pub mod hls;

use crate::types::{DownloadOptions, Format};
use anyhow::{Context, Result};
use reqwest::Client;
use std::path::Path;

pub use http::HttpDownloader;
pub use hls::HlsDownloader;

pub fn build_client(opts: &DownloadOptions) -> Result<Client> {
    let mut builder = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10));

    if let Some(proxy_url) = &opts.proxy {
        let proxy = reqwest::Proxy::all(proxy_url.as_str())
            .context("Invalid proxy URL")?;
        builder = builder.proxy(proxy);
    }

    if let Some(cookies_file) = &opts.cookies_file {
        // Load Netscape-format cookies file
        // reqwest doesn't natively support Netscape cookies, so we skip for now
        eprintln!("Warning: --cookies not yet supported, ignoring {}", cookies_file);
    }

    builder.build().context("Failed to build HTTP client")
}

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
