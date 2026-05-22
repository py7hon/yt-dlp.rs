use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub struct HttpDownloader {
    client: Client,
    retries: u32,
    rate_limit: Option<u64>,
    no_progress: bool,
    quiet: bool,
}

impl HttpDownloader {
    pub fn new(
        client: Client,
        retries: u32,
        rate_limit: Option<u64>,
        no_progress: bool,
        quiet: bool,
    ) -> Self {
        Self {
            client,
            retries,
            rate_limit,
            no_progress,
            quiet,
        }
    }

    pub async fn download(
        &self,
        url: &str,
        output_path: &Path,
        label: &str,
        headers: &std::collections::HashMap<String, String>,
    ) -> Result<u64> {
        let mut attempt = 0u32;
        loop {
            match self
                .download_attempt(url, output_path, label, headers)
                .await
            {
                Ok(bytes) => return Ok(bytes),
                Err(e) if attempt < self.retries => {
                    attempt += 1;
                    if !self.quiet {
                        eprintln!(
                            "Download attempt {}/{} failed: {}. Retrying...",
                            attempt, self.retries, e
                        );
                    }
                    tokio::time::sleep(Duration::from_secs(
                        (2u64).pow(attempt.min(6)),
                    ))
                    .await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn download_attempt(
        &self,
        url: &str,
        output_path: &Path,
        label: &str,
        headers: &std::collections::HashMap<String, String>,
    ) -> Result<u64> {
        // Check if partial download exists for resume
        let existing_size = if output_path.exists() {
            tokio::fs::metadata(output_path).await?.len()
        } else {
            0
        };

        let mut req = self.client.get(url);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if existing_size > 0 {
            req = req.header("Range", format!("bytes={}-", existing_size));
        }

        let resp = req.send().await.context("HTTP request failed")?;
        let status = resp.status();

        if !status.is_success() && status.as_u16() != 206 {
            return Err(anyhow!(
                "HTTP {} for {}",
                status.as_u16(),
                url
            ));
        }

        let total_size = resp
            .content_length()
            .map(|l| l + existing_size)
            .unwrap_or(0);

        let pb = if !self.no_progress && !self.quiet {
            let pb = ProgressBar::new(if total_size > 0 { total_size } else { u64::MAX });
            pb.set_style(
                ProgressStyle::with_template(
                    "{prefix:.bold.dim} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})",
                )
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("█▉▊▋▌▍▎▏ "),
            );
            pb.set_prefix(label.to_string());
            pb.set_position(existing_size);
            Some(pb)
        } else {
            None
        };

        let open_mode = if existing_size > 0 && status.as_u16() == 206 {
            // Resume: append
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(output_path)
                .await
                .context("Failed to open file for append")?
        } else {
            File::create(output_path)
                .await
                .context("Failed to create output file")?
        };

        let mut file = open_mode;
        let mut stream = resp.bytes_stream();
        let mut bytes_written = existing_size;
        let start = Instant::now();
        let mut last_rate_check = Instant::now();
        let mut bytes_since_check = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk: Bytes = chunk.context("Stream error")?;
            let chunk_len = chunk.len() as u64;

            file.write_all(&chunk).await.context("Write error")?;
            bytes_written += chunk_len;
            bytes_since_check += chunk_len;

            if let Some(ref pb) = pb {
                pb.set_position(bytes_written);
            }

            // Rate limiting
            if let Some(limit) = self.rate_limit {
                let elapsed = last_rate_check.elapsed().as_secs_f64();
                if elapsed > 0.1 {
                    let rate = bytes_since_check as f64 / elapsed;
                    if rate > limit as f64 {
                        let sleep_ms = ((bytes_since_check as f64 / limit as f64 - elapsed)
                            * 1000.0) as u64;
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                    }
                    last_rate_check = Instant::now();
                    bytes_since_check = 0;
                }
            }
        }

        file.flush().await?;

        if let Some(pb) = pb {
            let elapsed = start.elapsed().as_secs_f64();
            let speed = bytes_written as f64 / elapsed.max(0.001);
            pb.finish_with_message(format!(
                "Done ({:.2} MiB, {:.1} MiB/s)",
                bytes_written as f64 / 1_048_576.0,
                speed / 1_048_576.0
            ));
        }

        Ok(bytes_written)
    }
}
