use anyhow::Result;
use colored::Colorize;

pub fn parse_rate_limit(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();
    if let Some(n) = s.strip_suffix('K') {
        Ok(n.trim().parse::<u64>()? * 1024)
    } else if let Some(n) = s.strip_suffix('M') {
        Ok(n.trim().parse::<u64>()? * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix('G') {
        Ok(n.trim().parse::<u64>()? * 1024 * 1024 * 1024)
    } else {
        Ok(s.parse::<u64>()?)
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

#[allow(dead_code)]
pub fn format_eta(secs: f64) -> String {
    if secs < 0.0 || secs.is_infinite() || secs.is_nan() {
        return "Unknown".to_string();
    }
    let secs = secs as u64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

#[allow(dead_code)]
pub fn format_speed(bytes_per_sec: f64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec as u64))
}

pub fn mime_to_ext(mime_type: &str) -> &str {
    let base = mime_type.split(';').next().unwrap_or("").trim();
    match base {
        "video/mp4" | "audio/mp4" => "mp4",
        "video/webm" | "audio/webm" => "webm",
        "video/x-flv" => "flv",
        "video/3gpp" => "3gp",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "application/x-mpegURL" | "application/vnd.apple.mpegURL" => "m3u8",
        "application/dash+xml" => "mpd",
        _ => {
            let sub = base.split('/').nth(1).unwrap_or("mp4");
            if sub.contains("mp4") {
                "mp4"
            } else if sub.contains("webm") {
                "webm"
            } else {
                "mp4"
            }
        }
    }
}

pub fn parse_mime_codecs(mime_type: &str) -> (Option<String>, Option<String>) {
    let codecs_str = mime_type
        .split(';')
        .skip(1)
        .find_map(|part| {
            let part = part.trim();
            part.strip_prefix("codecs=")
                .map(|c| c.trim_matches('"').to_string())
        });

    let codecs_str = match codecs_str {
        Some(s) => s,
        None => return (None, None),
    };

    let is_video = mime_type.starts_with("video/");
    let codecs: Vec<&str> = codecs_str.split(',').map(|s| s.trim()).collect();

    if is_video {
        if codecs.len() >= 2 {
            // video/mp4; codecs="avc1.xxx, mp4a.xxx"
            let vcodec = codec_shortname(codecs[0]);
            let acodec = codec_shortname(codecs[1]);
            (Some(vcodec), Some(acodec))
        } else if codecs.len() == 1 {
            let c = codecs[0];
            if is_audio_codec(c) {
                (Some("none".to_string()), Some(codec_shortname(c)))
            } else {
                (Some(codec_shortname(c)), Some("none".to_string()))
            }
        } else {
            (None, None)
        }
    } else {
        // audio/*
        if codecs.len() >= 1 {
            (Some("none".to_string()), Some(codec_shortname(codecs[0])))
        } else {
            (Some("none".to_string()), None)
        }
    }
}

fn is_audio_codec(codec: &str) -> bool {
    let c = codec.to_lowercase();
    c.starts_with("mp4a") || c.starts_with("opus") || c.starts_with("vorbis")
        || c.starts_with("flac") || c.starts_with("mp3") || c.starts_with("ac-3")
        || c.starts_with("ec-3")
}

pub fn codec_shortname(codec: &str) -> String {
    let codec = codec.trim();
    if codec.starts_with("avc1") || codec.starts_with("avc3") {
        "avc1".to_string()
    } else if codec.starts_with("mp4a") {
        "mp4a".to_string()
    } else if codec.starts_with("vp9") || codec.starts_with("vp09") {
        "vp9".to_string()
    } else if codec.starts_with("vp8") {
        "vp8".to_string()
    } else if codec.starts_with("av01") || codec.starts_with("av1") {
        "av01".to_string()
    } else if codec.starts_with("opus") {
        "opus".to_string()
    } else if codec.starts_with("vorbis") {
        "vorbis".to_string()
    } else if codec.starts_with("hvc1") || codec.starts_with("hev1") {
        "h265".to_string()
    } else {
        codec.split('.').next().unwrap_or(codec).to_lowercase()
    }
}

pub fn print_info(msg: &str) {
    eprintln!("{}", msg.blue());
}

pub fn print_warning(msg: &str) {
    eprintln!("{}", format!("WARNING: {}", msg).yellow());
}

pub fn print_error(msg: &str) {
    eprintln!("{}", format!("ERROR: {}", msg).red());
}

pub fn print_success(msg: &str) {
    println!("{}", msg.green());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rate_limit() {
        assert_eq!(parse_rate_limit("500K").unwrap(), 512000);
        assert_eq!(parse_rate_limit("1M").unwrap(), 1048576);
        assert_eq!(parse_rate_limit("100").unwrap(), 100);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1048576), "1.00 MiB");
        assert_eq!(format_bytes(500), "500 B");
    }

    #[test]
    fn test_parse_mime_codecs() {
        let (v, a) = parse_mime_codecs("video/mp4; codecs=\"avc1.64002A, mp4a.40.2\"");
        assert_eq!(v.as_deref(), Some("avc1"));
        assert_eq!(a.as_deref(), Some("mp4a"));

        let (v, a) = parse_mime_codecs("audio/mp4; codecs=\"mp4a.40.2\"");
        assert_eq!(v.as_deref(), Some("none"));
        assert_eq!(a.as_deref(), Some("mp4a"));
    }

    #[test]
    fn test_parse_rate_limit_gigabyte() {
        assert_eq!(parse_rate_limit("1G").unwrap(), 1_073_741_824);
    }

    #[test]
    fn test_parse_rate_limit_invalid() {
        assert!(parse_rate_limit("abc").is_err());
    }

    #[test]
    fn test_format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn test_format_bytes_gib() {
        assert_eq!(format_bytes(1_073_741_824), "1.00 GiB");
    }

    #[test]
    fn test_mime_to_ext_video_mp4() {
        assert_eq!(mime_to_ext("video/mp4"), "mp4");
        assert_eq!(mime_to_ext("video/webm"), "webm");
        assert_eq!(mime_to_ext("audio/mp4"), "mp4");
    }

    #[test]
    fn test_mime_to_ext_with_codecs() {
        assert_eq!(mime_to_ext("video/mp4; codecs=\"avc1\""), "mp4");
    }

    #[test]
    fn test_parse_mime_codecs_vp9_video_only() {
        let (v, a) = parse_mime_codecs("video/webm; codecs=\"vp9\"");
        assert_eq!(v.as_deref(), Some("vp9"));
        assert_eq!(a.as_deref(), Some("none"));
    }

    #[test]
    fn test_parse_mime_codecs_opus_audio() {
        let (v, a) = parse_mime_codecs("audio/webm; codecs=\"opus\"");
        assert_eq!(v.as_deref(), Some("none"));
        assert_eq!(a.as_deref(), Some("opus"));
    }

    #[test]
    fn test_parse_mime_codecs_av1() {
        let (v, a) = parse_mime_codecs("video/mp4; codecs=\"av01.0.05M.08\"");
        assert_eq!(v.as_deref(), Some("av01"));
        assert_eq!(a.as_deref(), Some("none"));
    }

    #[test]
    fn test_codec_shortname() {
        assert_eq!(codec_shortname("avc1.42001E"), "avc1");
        assert_eq!(codec_shortname("mp4a.40.2"), "mp4a");
        assert_eq!(codec_shortname("vp09.00.50.08"), "vp9");
        assert_eq!(codec_shortname("av01.0.00M.08"), "av01");
        assert_eq!(codec_shortname("opus"), "opus");
    }
}
