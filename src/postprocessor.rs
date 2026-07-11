#![cfg(not(target_arch = "wasm32"))]

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

pub async fn ffmpeg_merge(video_path: &Path, audio_path: &Path, output_path: &Path) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            &video_path.to_string_lossy(),
            "-i",
            &audio_path.to_string_lossy(),
            "-c",
            "copy",
            &output_path.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("Failed to run ffmpeg (is it installed?)")?;

    if !status.success() {
        return Err(anyhow!("ffmpeg exited with code {:?}", status.code()));
    }
    Ok(())
}

pub async fn ffmpeg_extract_audio(input_path: &Path, output_ext: &str) -> Result<PathBuf> {
    let output_path = input_path.with_extension(output_ext);

    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input_path.to_string_lossy().to_string(),
    ];

    // Copy if the container supports it, else transcode
    match output_ext {
        "m4a" | "aac" => {
            args.extend(["-vn".to_string(), "-c:a".to_string(), "copy".to_string()]);
        }
        "mp3" => {
            args.extend([
                "-vn".to_string(),
                "-c:a".to_string(),
                "libmp3lame".to_string(),
                "-q:a".to_string(),
                "0".to_string(),
            ]);
        }
        "opus" => {
            args.extend(["-vn".to_string(), "-c:a".to_string(), "libopus".to_string()]);
        }
        _ => {
            args.extend(["-vn".to_string(), "-c:a".to_string(), "copy".to_string()]);
        }
    }

    args.push(output_path.to_string_lossy().to_string());

    let status = Command::new("ffmpeg")
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("Failed to run ffmpeg")?;

    if !status.success() {
        return Err(anyhow!(
            "ffmpeg audio extraction exited with code {:?}",
            status.code()
        ));
    }

    Ok(output_path)
}

#[allow(dead_code)]
pub async fn ffmpeg_convert_hls(m3u8_url: &str, output_path: &Path) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            m3u8_url,
            "-c",
            "copy",
            "-bsf:a",
            "aac_adtstoasc",
            &output_path.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("Failed to run ffmpeg for HLS conversion")?;

    if !status.success() {
        return Err(anyhow!(
            "ffmpeg HLS conversion exited with code {:?}",
            status.code()
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn ffmpeg_remux(input_path: &Path, output_path: &Path) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            &input_path.to_string_lossy(),
            "-c",
            "copy",
            &output_path.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("Failed to run ffmpeg for remux")?;

    if !status.success() {
        return Err(anyhow!("ffmpeg remux exited with code {:?}", status.code()));
    }
    Ok(())
}

pub fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
