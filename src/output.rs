use crate::types::VideoInfo;
use std::path::PathBuf;

pub fn expand_template(template: &str, info: &VideoInfo, ext: &str) -> String {
    let mut result = template.to_string();

    let replacements: &[(&str, String)] = &[
        ("%(id)s", info.id.clone()),
        ("%(title)s", info.title.clone()),
        ("%(ext)s", ext.to_string()),
        ("%(uploader)s", info.uploader.clone().unwrap_or_default()),
        ("%(uploader_id)s", info.uploader_id.clone().unwrap_or_default()),
        ("%(channel)s", info.channel.clone().unwrap_or_default()),
        ("%(channel_id)s", info.channel_id.clone().unwrap_or_default()),
        ("%(upload_date)s", info.upload_date.clone().unwrap_or_default()),
        ("%(description)s", info.description.clone().unwrap_or_default()),
        (
            "%(duration)s",
            info.duration.map_or_else(String::new, format_duration),
        ),
        (
            "%(view_count)s",
            info.view_count.map_or_else(String::new, |v| v.to_string()),
        ),
        (
            "%(like_count)s",
            info.like_count.map_or_else(String::new, |v| v.to_string()),
        ),
        (
            "%(playlist_index)s",
            info.playlist_index
                .map_or_else(String::new, |i| format!("{:03}", i)),
        ),
        (
            "%(playlist)s",
            info.playlist.clone().unwrap_or_default(),
        ),
        (
            "%(playlist_id)s",
            info.playlist_id.clone().unwrap_or_default(),
        ),
        ("%(webpage_url)s", info.webpage_url.clone()),
        ("%(extractor)s", info.extractor.clone()),
    ];

    for (placeholder, value) in replacements {
        result = result.replace(placeholder, value);
    }

    result
}

fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

pub fn sanitize_filename(name: &str, restrict: bool) -> String {
    let mut s = name.to_string();

    // Replace path separators first
    s = s.replace(['/', '\\'], "_");

    if restrict {
        // ASCII only, replace anything non-alphanumeric/safe with _
        s = s
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "._-()[] ".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
    } else {
        // Remove characters invalid on Windows/macOS/Linux
        s = s.replace([':', '*', '?', '"', '<', '>', '|', '\0'], "_");
    }

    // Collapse multiple underscores/spaces
    while s.contains("__") {
        s = s.replace("__", "_");
    }

    // Trim dots and spaces from start/end (Windows compat)
    s = s.trim_matches(|c| c == ' ' || c == '.').to_string();

    // Cap length at 200 chars to avoid filesystem limits
    if s.len() > 200 {
        s.truncate(200);
        s = s.trim_end_matches(|c: char| !c.is_alphanumeric()).to_string();
    }

    if s.is_empty() {
        s = "video".to_string();
    }

    s
}

pub fn build_output_path(
    template: &str,
    info: &VideoInfo,
    ext: &str,
    restrict: bool,
) -> PathBuf {
    let expanded = expand_template(template, info, ext);

    // Split into dir and filename parts and sanitize only the filename portion
    let path = PathBuf::from(&expanded);
    if let Some(parent) = path.parent() {
        if parent != std::path::Path::new("") {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            return parent.join(sanitize_filename(&filename, restrict));
        }
    }

    PathBuf::from(sanitize_filename(&expanded, restrict))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VideoInfo;

    fn info(id: &str, title: &str, uploader: &str) -> VideoInfo {
        VideoInfo {
            id: id.to_string(),
            title: title.to_string(),
            webpage_url: format!("https://youtube.com/watch?v={}", id),
            extractor: "youtube".to_string(),
            uploader: Some(uploader.to_string()),
            upload_date: Some("20231115".to_string()),
            view_count: Some(1_234_567),
            duration: Some(213.0),
            ..VideoInfo::default()
        }
    }

    // ─── expand_template ─────────────────────────────────────────────────

    #[test]
    fn default_template() {
        let i = info("ABC123", "My Video", "Author");
        let result = expand_template("%(title)s [%(id)s].%(ext)s", &i, "mp4");
        assert_eq!(result, "My Video [ABC123].mp4");
    }

    #[test]
    fn all_basic_fields() {
        let i = info("XYZ", "Title", "Chan");
        assert_eq!(expand_template("%(id)s", &i, ""), "XYZ");
        assert_eq!(expand_template("%(title)s", &i, ""), "Title");
        assert_eq!(expand_template("%(uploader)s", &i, ""), "Chan");
        assert_eq!(expand_template("%(ext)s", &i, "webm"), "webm");
        assert_eq!(expand_template("%(upload_date)s", &i, ""), "20231115");
    }

    #[test]
    fn view_count_field() {
        let i = info("A", "T", "U");
        assert_eq!(expand_template("%(view_count)s", &i, ""), "1234567");
    }

    #[test]
    fn duration_field() {
        let i = info("A", "T", "U");
        // 213s = 03:33
        assert_eq!(expand_template("%(duration)s", &i, ""), "03:33");
    }

    #[test]
    fn missing_optional_field_is_empty() {
        let mut i = info("A", "T", "U");
        i.uploader = None;
        assert_eq!(expand_template("%(uploader)s", &i, ""), "");
    }

    #[test]
    fn unknown_placeholder_is_left_as_is() {
        let i = info("A", "T", "U");
        let result = expand_template("%(nonexistent)s", &i, "");
        assert_eq!(result, "%(nonexistent)s");
    }

    #[test]
    fn multiple_fields_in_one_template() {
        let i = info("VID", "Great Video", "Creator");
        let result = expand_template("%(uploader)s - %(title)s.%(ext)s", &i, "mp4");
        assert_eq!(result, "Creator - Great Video.mp4");
    }

    // ─── sanitize_filename ───────────────────────────────────────────────

    #[test]
    fn forward_slash_replaced() {
        assert_eq!(sanitize_filename("a/b", false), "a_b");
    }

    #[test]
    fn backslash_replaced() {
        assert_eq!(sanitize_filename("a\\b", false), "a_b");
    }

    #[test]
    fn windows_invalid_chars_replaced() {
        assert_eq!(sanitize_filename("file:name", false), "file_name");
        assert_eq!(sanitize_filename("a*b", false), "a_b");
        assert_eq!(sanitize_filename("a?b", false), "a_b");
        assert_eq!(sanitize_filename("a<b>c", false), "a_b_c");
        assert_eq!(sanitize_filename("a|b", false), "a_b");
    }

    #[test]
    fn restrict_mode_removes_non_ascii() {
        // In restrict mode, non-ASCII chars become underscores
        let result = sanitize_filename("café", true);
        assert!(!result.contains('é'), "non-ASCII should be removed in restrict mode");
    }

    #[test]
    fn restrict_mode_allows_safe_ascii() {
        let result = sanitize_filename("hello-world_v2 (HD)", true);
        assert_eq!(result, "hello-world_v2 (HD)");
    }

    #[test]
    fn long_filename_truncated() {
        let long = "a".repeat(300);
        let result = sanitize_filename(&long, false);
        assert!(result.len() <= 200);
    }

    #[test]
    fn leading_trailing_dots_trimmed() {
        assert_eq!(sanitize_filename("..file..", false), "file");
    }

    #[test]
    fn empty_becomes_video() {
        assert_eq!(sanitize_filename("", false), "video");
    }

    #[test]
    fn double_underscores_collapsed() {
        assert_eq!(sanitize_filename("a//b", false), "a_b");
    }

    // ─── build_output_path ───────────────────────────────────────────────

    #[test]
    fn builds_path_from_template() {
        let i = info("XYZ", "My Video", "U");
        let path = build_output_path("%(title)s [%(id)s].%(ext)s", &i, "mp4", false);
        assert_eq!(path.to_str().unwrap(), "My Video [XYZ].mp4");
    }

    #[test]
    fn builds_path_with_directory() {
        let i = info("X", "T", "U");
        let path = build_output_path("downloads/%(title)s.%(ext)s", &i, "mkv", false);
        assert_eq!(path.to_str().unwrap(), "downloads/T.mkv");
    }

    #[test]
    fn sanitization_applied() {
        let i = info("X", "Video: Part 1", "U");
        let path = build_output_path("%(title)s.%(ext)s", &i, "mp4", false);
        assert!(!path.to_str().unwrap().contains(':'));
    }
}
