use anyhow::{anyhow, Context, Result};
use m3u8_rs::{MasterPlaylist, Playlist};
use reqwest::Client;

#[cfg(not(target_arch = "wasm32"))]
use indicatif::{ProgressBar, ProgressStyle};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use tokio::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io::AsyncWriteExt;

#[cfg(not(target_arch = "wasm32"))]
use tokio::time::sleep;

#[cfg(target_arch = "wasm32")]
async fn sleep(duration: std::time::Duration) {
    gloo_timers::future::sleep(duration).await;
}

pub struct HlsDownloader {
    client: Client,
    concurrent: usize,
    retries: u32,
    no_progress: bool,
    quiet: bool,
}

impl HlsDownloader {
    pub fn new(
        client: Client,
        concurrent: usize,
        retries: u32,
        no_progress: bool,
        quiet: bool,
    ) -> Self {
        Self {
            client,
            concurrent,
            retries,
            no_progress,
            quiet,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HlsDownloader {
    pub async fn download(&self, url: &str, output_path: &Path, label: &str) -> Result<()> {
        let playlist_bytes = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to fetch HLS playlist")?
            .bytes()
            .await?;

        let playlist = m3u8_rs::parse_playlist_res(&playlist_bytes)
            .map_err(|e| anyhow!("Failed to parse HLS playlist: {}", e))?;

        let media_url = match playlist {
            Playlist::MasterPlaylist(master) => self.select_stream_url(url, &master)?,
            Playlist::MediaPlaylist(_) => url.to_string(),
        };

        let media_bytes = self.client.get(&media_url).send().await?.bytes().await?;

        let media_playlist = match m3u8_rs::parse_playlist_res(&media_bytes)
            .map_err(|e| anyhow!("Failed to parse media playlist: {}", e))?
        {
            Playlist::MediaPlaylist(p) => p,
            Playlist::MasterPlaylist(_) => {
                return Err(anyhow!("Expected media playlist, got master"))
            }
        };

        let base_url = base_url_of(&media_url);
        let segments: Vec<String> = media_playlist
            .segments
            .iter()
            .map(|seg| resolve_url(&base_url, &seg.uri))
            .collect();

        let total = segments.len() as u64;
        let pb = if !self.no_progress && !self.quiet {
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::with_template(
                    "{prefix:.bold.dim} [{bar:40.cyan/blue}] {pos}/{len} segments ({per_sec})",
                )
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("█▉▊▋▌▍▎▏ "),
            );
            pb.set_prefix(label.to_string());
            Some(pb)
        } else {
            None
        };

        let mut file = File::create(output_path)
            .await
            .context("Failed to create output file")?;

        // Download segments with limited concurrency
        let chunk_size = self.concurrent;
        for chunk in segments.chunks(chunk_size) {
            let mut futures = Vec::new();
            for seg_url in chunk {
                futures.push(self.fetch_segment(seg_url));
            }
            let results = futures_util::future::join_all(futures).await;
            for result in results {
                let data = result?;
                file.write_all(&data).await.context("Write error")?;
                if let Some(ref pb) = pb {
                    pb.inc(1);
                }
            }
        }

        file.flush().await?;

        if let Some(pb) = pb {
            pb.finish_with_message("Done");
        }

        Ok(())
    }
}

impl HlsDownloader {
    async fn fetch_segment(&self, url: &str) -> Result<bytes::Bytes> {
        let mut attempt = 0u32;
        loop {
            match self.client.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return resp.bytes().await.context("Failed to read segment");
                }
                Ok(resp) => {
                    let err = anyhow!("HTTP {} for segment {}", resp.status(), url);
                    if attempt >= self.retries {
                        return Err(err);
                    }
                    attempt += 1;
                    sleep(std::time::Duration::from_secs(2u64.pow(attempt.min(5)))).await;
                }
                Err(e) => {
                    if attempt >= self.retries {
                        return Err(e.into());
                    }
                    attempt += 1;
                    sleep(std::time::Duration::from_secs(2u64.pow(attempt.min(5)))).await;
                }
            }
        }
    }

    fn select_stream_url(&self, base: &str, master: &MasterPlaylist) -> Result<String> {
        // Pick the stream with the highest bandwidth
        let best = master.variants.iter().max_by_key(|v| v.bandwidth);

        let best = best.ok_or_else(|| anyhow!("No streams in HLS master playlist"))?;
        Ok(resolve_url(&base_url_of(base), &best.uri))
    }
}

#[cfg(target_arch = "wasm32")]
impl HlsDownloader {
    pub async fn download_to_vec_with_progress<F>(
        &self,
        url: &str,
        cors_proxy: Option<&str>,
        progress_callback: F,
    ) -> Result<Vec<u8>>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        let final_url = if let Some(proxy) = cors_proxy {
            format!("{}{}", proxy, url)
        } else {
            url.to_string()
        };

        let playlist_bytes = self
            .client
            .get(&final_url)
            .send()
            .await
            .context("Failed to fetch HLS playlist")?
            .bytes()
            .await?;

        let playlist = m3u8_rs::parse_playlist_res(&playlist_bytes)
            .map_err(|e| anyhow!("Failed to parse HLS playlist: {}", e))?;

        let media_url = match playlist {
            Playlist::MasterPlaylist(master) => self.select_stream_url(&final_url, &master)?,
            Playlist::MediaPlaylist(_) => final_url.clone(),
        };

        let media_bytes = self.client.get(&media_url).send().await?.bytes().await?;

        let media_playlist = match m3u8_rs::parse_playlist_res(&media_bytes)
            .map_err(|e| anyhow!("Failed to parse media playlist: {}", e))?
        {
            Playlist::MediaPlaylist(p) => p,
            Playlist::MasterPlaylist(_) => {
                return Err(anyhow!("Expected media playlist, got master"))
            }
        };

        let base_url = base_url_of(&media_url);
        let segments: Vec<String> = media_playlist
            .segments
            .iter()
            .map(|seg| resolve_url(&base_url, &seg.uri))
            .collect();

        let total = segments.len() as u64;
        let mut data = Vec::new();
        let mut completed_segments = 0u64;

        let chunk_size = self.concurrent;
        for chunk in segments.chunks(chunk_size) {
            let mut futures = Vec::new();
            for seg_url in chunk {
                futures.push(self.fetch_segment(seg_url));
            }
            let results = futures_util::future::join_all(futures).await;
            for result in results {
                let segment_bytes = result?;
                data.extend_from_slice(&segment_bytes);
                completed_segments += 1;
                progress_callback(completed_segments, total);
            }
        }

        Ok(data)
    }
}

fn base_url_of(url: &str) -> String {
    if let Some(pos) = url.rfind('/') {
        url[..=pos].to_string()
    } else {
        url.to_string()
    }
}

fn resolve_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else if path.starts_with('/') {
        // Absolute path — extract origin from base
        if let Some(origin_end) = base[8..].find('/').map(|i| i + 8) {
            format!("{}{}", &base[..origin_end], path)
        } else {
            format!("{}{}", base.trim_end_matches('/'), path)
        }
    } else {
        format!("{}{}", base, path)
    }
}
