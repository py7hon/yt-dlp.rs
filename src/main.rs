mod downloader;
mod error;
mod extractor;
mod output;
mod postprocessor;
mod selector;
mod types;
mod utils;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use colored::Colorize;
use downloader::{build_client, download_format};
use extractor::get_extractor;
use output::build_output_path;
use postprocessor::{ffmpeg_available, ffmpeg_extract_audio, ffmpeg_merge};
use selector::{parse_selector, select_formats, SelectedFormats};
use types::{DownloadOptions, Format, VideoInfo};
use utils::{format_bytes, parse_rate_limit, print_error, print_info, print_success, print_warning};

#[derive(Parser, Debug)]
#[command(
    name = "yt-dpl",
    about = "Download videos from YouTube and other sites",
    version = env!("CARGO_PKG_VERSION"),
    after_help = "EXAMPLES:
    yt-dpl 'https://youtu.be/dQw4w9WgXcQ'
    yt-dpl -f 'bestvideo[height<=1080]+bestaudio' URL
    yt-dpl -x -f bestaudio URL
    yt-dpl -F URL"
)]
struct Args {
    /// URLs to download (or YouTube video IDs)
    #[arg(required = true)]
    urls: Vec<String>,

    /// Format selection (e.g. bestvideo+bestaudio, best, 137+140)
    #[arg(short = 'f', long, default_value = "bestvideo+bestaudio/best")]
    format: String,

    /// Output filename template
    #[arg(short = 'o', long, default_value = "%(title)s [%(id)s].%(ext)s")]
    output: String,

    /// List available formats and exit
    #[arg(short = 'F', long = "list-formats")]
    list_formats: bool,

    /// Extract audio after download
    #[arg(short = 'x', long = "extract-audio")]
    extract_audio: bool,

    /// Audio format when extracting (m4a, mp3, opus)
    #[arg(long = "audio-format", default_value = "m4a")]
    audio_format: String,

    /// Skip actual download (only extract info)
    #[arg(long = "skip-download")]
    skip_download: bool,

    /// Print video info as JSON
    #[arg(short = 'j', long = "dump-json")]
    dump_json: bool,

    /// Quiet (suppress output)
    #[arg(short = 'q', long)]
    quiet: bool,

    /// Disable progress bar
    #[arg(long = "no-progress")]
    no_progress: bool,

    /// Rate limit bytes/sec (e.g. 500K, 2M)
    #[arg(short = 'r', long = "rate-limit")]
    rate_limit: Option<String>,

    /// Number of download retries
    #[arg(short = 'R', long, default_value = "10")]
    retries: u32,

    /// Concurrent fragment downloads
    #[arg(long = "concurrent-fragments", default_value = "4")]
    concurrent_fragments: usize,

    /// Container format when merging (mkv, mp4, webm)
    #[arg(long = "merge-output-format", default_value = "mkv")]
    merge_output_format: String,

    /// Keep intermediate video/audio files after merging
    #[arg(short = 'k', long = "keep-video")]
    keep_video: bool,

    /// Write subtitle files
    #[arg(long = "write-subs")]
    write_subs: bool,

    /// Write auto-generated subtitles
    #[arg(long = "write-auto-subs")]
    write_auto_subs: bool,

    /// Subtitle languages (comma-separated, e.g. en,de or "all")
    #[arg(long = "sub-langs", default_value = "en")]
    sub_langs: String,

    /// Restrict filenames to ASCII
    #[arg(long = "restrict-filenames")]
    restrict_filenames: bool,

    /// Proxy URL (http/https/socks5)
    #[arg(long)]
    proxy: Option<String>,

    /// Cookies file (Netscape format)
    #[arg(long = "cookies")]
    cookies: Option<String>,

    /// Print field value (e.g. "%(title)s")
    #[arg(long = "print")]
    print_field: Option<String>,

    /// Simulate — show what would happen without downloading
    #[arg(short = 's', long = "simulate")]
    simulate: bool,
}

impl Args {
    fn to_options(&self) -> Result<DownloadOptions> {
        let rate_limit = self
            .rate_limit
            .as_deref()
            .map(parse_rate_limit)
            .transpose()
            .context("Invalid rate limit")?;

        Ok(DownloadOptions {
            format: self.format.clone(),
            output_template: self.output.clone(),
            extract_audio: self.extract_audio,
            audio_format: self.audio_format.clone(),
            skip_download: self.skip_download || self.simulate,
            quiet: self.quiet,
            no_progress: self.no_progress,
            retries: self.retries,
            concurrent_fragments: self.concurrent_fragments,
            merge_output_format: self.merge_output_format.clone(),
            keep_video: self.keep_video,
            write_subs: self.write_subs,
            write_auto_subs: self.write_auto_subs,
            sub_langs: self
                .sub_langs
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            restrict_filenames: self.restrict_filenames,
            rate_limit,
            proxy: self.proxy.clone(),
            cookies_file: self.cookies.clone(),
        })
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Err(e) = run(args).await {
        print_error(&format!("{:#}", e));
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<()> {
    let opts = args.to_options()?;
    let client = build_client(&opts).context("Failed to build HTTP client")?;

    let has_ffmpeg = ffmpeg_available();

    let mut any_error = false;

    for url in &args.urls {
        match process_url(url, &client, &opts, &args, has_ffmpeg).await {
            Ok(()) => {}
            Err(e) => {
                print_error(&format!("{}: {:#}", url, e));
                any_error = true;
            }
        }
    }

    if any_error {
        std::process::exit(1);
    }
    Ok(())
}

async fn process_url(
    url: &str,
    client: &reqwest::Client,
    opts: &DownloadOptions,
    args: &Args,
    has_ffmpeg: bool,
) -> Result<()> {
    let extractor = get_extractor(url)
        .ok_or_else(|| anyhow!("No extractor found for URL: {}", url))?;

    if !opts.quiet {
        print_info(&format!("[{}] Extracting info...", extractor.name()));
    }

    let info = extractor
        .extract(url, client)
        .await
        .context("Extraction failed")?;

    // --print field
    if let Some(field) = &args.print_field {
        let value = output::expand_template(field, &info, "");
        println!("{}", value);
        return Ok(());
    }

    // --dump-json
    if args.dump_json {
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
    }

    if !opts.quiet {
        eprintln!(
            "[{}] {}: {}",
            extractor.name().cyan(),
            info.id.yellow(),
            info.title.bold()
        );
        if let Some(d) = info.duration {
            let mins = (d / 60.0) as u64;
            let secs = (d % 60.0) as u64;
            eprintln!("      Duration: {:02}:{:02}", mins, secs);
        }
        if let Some(views) = info.view_count {
            eprintln!("      Views: {}", format_number(views));
        }
    }

    // --list-formats
    if args.list_formats {
        print_formats(&info);
        return Ok(());
    }

    // Write subtitles
    if opts.write_subs || opts.write_auto_subs {
        write_subtitles(&info, opts, client).await?;
    }

    // Select formats
    let selector = parse_selector(&opts.format)
        .with_context(|| format!("Invalid format selector: {}", opts.format))?;

    let selected = select_formats(&selector, &info.formats)
        .with_context(|| format!("No formats match: {}", opts.format))?;

    // Determine output extension and path
    let needs_merge = matches!(selected, SelectedFormats::Merge(_, _));

    // Resolve ffmpeg fallback before deciding ext, but after we know if we'd merge
    let (selected, output_ext) = if needs_merge && !has_ffmpeg {
        if !opts.quiet {
            print_info("ffmpeg not found, downloading best combined format (install ffmpeg for 1080p+)");
        }
        let fallback_selector = parse_selector("best")?;
        let fallback = select_formats(&fallback_selector, &info.formats)
            .context("No combined formats available")?;
        let ext = if let SelectedFormats::Single(f) = &fallback {
            f.ext.clone()
        } else {
            "mp4".to_string()
        };
        (fallback, ext)
    } else {
        let ext = match &selected {
            SelectedFormats::Single(fmt) => fmt.ext.clone(),
            SelectedFormats::Merge(_, _) => opts.merge_output_format.clone(),
        };
        (selected, ext)
    };

    if opts.skip_download {
        if !opts.quiet {
            match &selected {
                SelectedFormats::Single(fmt) => {
                    eprintln!(
                        "      Would download: {} ({}x{}, {})",
                        fmt.format_id,
                        fmt.width.unwrap_or(0),
                        fmt.height.unwrap_or(0),
                        fmt.approx_filesize()
                            .map(format_bytes)
                            .unwrap_or_else(|| "unknown size".to_string())
                    );
                }
                SelectedFormats::Merge(v, a) => {
                    eprintln!(
                        "      Would merge: {} (video) + {} (audio)",
                        v.format_id, a.format_id
                    );
                }
            }
        }
        return Ok(());
    }

    match selected {
        SelectedFormats::Single(fmt) => {
            do_single_download(fmt, &info, opts, client, &output_ext).await?;
        }
        SelectedFormats::Merge(video_fmt, audio_fmt) => {
            do_merge_download(video_fmt, audio_fmt, &info, opts, client, &output_ext).await?;
        }
    }

    Ok(())
}

async fn do_single_download(
    fmt: &Format,
    info: &VideoInfo,
    opts: &DownloadOptions,
    client: &reqwest::Client,
    ext: &str,
) -> Result<()> {
    let out_path = build_output_path(
        &opts.output_template,
        info,
        ext,
        opts.restrict_filenames,
    );

    if out_path.exists() {
        if !opts.quiet {
            print_warning(&format!(
                "File already exists, skipping: {}",
                out_path.display()
            ));
        }
        return Ok(());
    }

    if !opts.quiet {
        println!(
            "  {} {} → {}",
            "Downloading".green(),
            fmt.format_id.yellow(),
            out_path.display()
        );
        if let Some(size) = fmt.approx_filesize() {
            println!("  Size: ~{}", format_bytes(size));
        }
    }

    let tmp_path = out_path.with_extension(format!("{}.part", ext));
    download_format(client, fmt, &tmp_path, &fmt.format_id, opts).await?;
    tokio::fs::rename(&tmp_path, &out_path).await?;

    if opts.extract_audio {
        if !opts.quiet {
            println!("  {} audio...", "Extracting".green());
        }
        let audio_path = ffmpeg_extract_audio(&out_path, &opts.audio_format).await?;
        if !opts.keep_video {
            tokio::fs::remove_file(&out_path).await.ok();
        }
        if !opts.quiet {
            print_success(&format!("  Saved to: {}", audio_path.display()));
        }
    } else if !opts.quiet {
        print_success(&format!("  Saved to: {}", out_path.display()));
    }

    Ok(())
}

async fn do_merge_download(
    video_fmt: &Format,
    audio_fmt: &Format,
    info: &VideoInfo,
    opts: &DownloadOptions,
    client: &reqwest::Client,
    merge_ext: &str,
) -> Result<()> {
    let out_path = build_output_path(
        &opts.output_template,
        info,
        merge_ext,
        opts.restrict_filenames,
    );

    if out_path.exists() {
        if !opts.quiet {
            print_warning(&format!(
                "File already exists, skipping: {}",
                out_path.display()
            ));
        }
        return Ok(());
    }

    let tmp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let video_tmp =
        tmp_dir
            .path()
            .join(format!("{}.video.{}", info.id, video_fmt.ext));
    let audio_tmp =
        tmp_dir
            .path()
            .join(format!("{}.audio.{}", info.id, audio_fmt.ext));

    if !opts.quiet {
        println!(
            "  {} {}+{} → {}",
            "Downloading".green(),
            video_fmt.format_id.yellow(),
            audio_fmt.format_id.yellow(),
            out_path.display()
        );
        let total_size = video_fmt
            .approx_filesize()
            .and_then(|v| audio_fmt.approx_filesize().map(|a| v + a));
        if let Some(size) = total_size {
            println!("  Size: ~{}", format_bytes(size));
        }
    }

    if !opts.quiet {
        println!(
            "  [video] {} ({}{})",
            video_fmt.format_id,
            video_fmt
                .height
                .map_or_else(String::new, |h| format!("{}p", h)),
            video_fmt
                .fps
                .map_or_else(String::new, |f| format!("/{:.0}fps", f))
        );
    }
    download_format(client, video_fmt, &video_tmp, "[video]", opts).await?;

    if !opts.quiet {
        println!(
            "  [audio] {} ({} kbps)",
            audio_fmt.format_id,
            audio_fmt.abr.unwrap_or(0.0) as u32
        );
    }
    download_format(client, audio_fmt, &audio_tmp, "[audio]", opts).await?;

    if !opts.quiet {
        println!("  {} streams...", "Merging".green());
    }
    ffmpeg_merge(&video_tmp, &audio_tmp, &out_path)
        .await
        .context("Merge failed")?;

    if opts.extract_audio {
        if !opts.quiet {
            println!("  {} audio...", "Extracting".green());
        }
        let audio_path = ffmpeg_extract_audio(&out_path, &opts.audio_format).await?;
        if !opts.keep_video {
            tokio::fs::remove_file(&out_path).await.ok();
        }
        if !opts.quiet {
            print_success(&format!("  Saved to: {}", audio_path.display()));
        }
    } else if !opts.quiet {
        print_success(&format!("  Saved to: {}", out_path.display()));
    }

    Ok(())
}

fn print_formats(info: &VideoInfo) {
    println!(
        "\n{}\n",
        format!(
            "[info] Available formats for {} - {}",
            info.id, info.title
        )
        .bold()
    );

    println!(
        "{:>6}  {:6}  {:12}  {:5}  {:12}  {:6}  {:8}  {:8}",
        "ID".bold(),
        "EXT".bold(),
        "RESOLUTION".bold(),
        "FPS".bold(),
        "FILESIZE".bold(),
        "TBR".bold(),
        "VCODEC".bold(),
        "ACODEC".bold()
    );
    println!("{}", "─".repeat(80));

    let mut video_only: Vec<&Format> = info.formats.iter().filter(|f| f.is_video_only()).collect();
    let mut audio_only: Vec<&Format> = info.formats.iter().filter(|f| f.is_audio_only()).collect();
    let mut combined: Vec<&Format> = info.formats.iter().filter(|f| f.is_combined()).collect();

    video_only.sort_by(|a, b| {
        b.video_score()
            .partial_cmp(&a.video_score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    audio_only.sort_by(|a, b| {
        b.audio_score()
            .partial_cmp(&a.audio_score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    combined.sort_by(|a, b| {
        b.total_score()
            .partial_cmp(&a.total_score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let all: Vec<&Format> = video_only
        .iter()
        .chain(audio_only.iter())
        .chain(combined.iter())
        .copied()
        .collect();

    for fmt in all {
        let resolution = match (fmt.width, fmt.height) {
            (Some(w), Some(h)) => format!("{}x{}", w, h),
            (None, Some(h)) => format!("{}p", h),
            _ if fmt.is_audio_only() => "audio only".to_string(),
            _ => "unknown".to_string(),
        };

        let fps = fmt.fps.map_or_else(|| " ".to_string(), |f| format!("{:.0}", f));
        let size = fmt
            .approx_filesize()
            .map_or_else(|| "~".to_string(), format_bytes);
        let tbr = fmt
            .tbr
            .map_or_else(|| " ".to_string(), |t| format!("{:.0}k", t));
        let vcodec = fmt.vcodec.as_deref().unwrap_or("-");
        let acodec = fmt.acodec.as_deref().unwrap_or("-");
        let note = fmt.format_note.as_deref().unwrap_or("");
        let drm = if fmt.has_drm { " [DRM]" } else { "" };

        println!(
            "{:>6}  {:6}  {:12}  {:5}  {:12}  {:6}  {:8}  {:8}  {}{}",
            fmt.format_id.cyan(),
            fmt.ext,
            resolution,
            fps,
            size,
            tbr,
            vcodec,
            acodec,
            note,
            drm
        );
    }
    println!();
}

async fn write_subtitles(
    info: &VideoInfo,
    opts: &DownloadOptions,
    client: &reqwest::Client,
) -> Result<()> {
    let all_langs = opts.sub_langs.iter().any(|l| l == "all");

    let mut to_write: Vec<(String, &crate::types::Subtitle)> = Vec::new();

    if opts.write_subs {
        for (lang, subs) in &info.subtitles {
            if all_langs || opts.sub_langs.iter().any(|l| l == lang) {
                let sub = subs.iter().find(|s| s.ext == "vtt").or_else(|| subs.first());
                if let Some(sub) = sub {
                    to_write.push((lang.clone(), sub));
                }
            }
        }
    }

    if opts.write_auto_subs {
        for (lang, subs) in &info.automatic_captions {
            if all_langs || opts.sub_langs.iter().any(|l| l == lang) {
                let sub = subs.iter().find(|s| s.ext == "vtt").or_else(|| subs.first());
                if let Some(sub) = sub {
                    to_write.push((lang.clone(), sub));
                }
            }
        }
    }

    for (lang, sub) in to_write {
        let sub_path = build_output_path(
            &opts.output_template,
            info,
            &format!("{}.{}", lang, sub.ext),
            opts.restrict_filenames,
        );

        if !opts.quiet {
            println!("  {} subtitle: {}", "Writing".green(), sub_path.display());
        }

        let data = client.get(&sub.url).send().await?.bytes().await?;
        tokio::fs::write(&sub_path, &data).await?;
    }

    Ok(())
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
