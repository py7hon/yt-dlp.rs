use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub webpage_url: String,
    pub extractor: String,
    pub description: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub uploader_id: Option<String>,
    pub channel: Option<String>,
    pub channel_id: Option<String>,
    pub channel_url: Option<String>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub upload_date: Option<String>,
    pub timestamp: Option<i64>,
    pub thumbnail: Option<String>,
    pub thumbnails: Vec<Thumbnail>,
    pub formats: Vec<Format>,
    pub subtitles: HashMap<String, Vec<Subtitle>>,
    pub automatic_captions: HashMap<String, Vec<Subtitle>>,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub age_limit: Option<u8>,
    pub is_live: Option<bool>,
    pub live_status: Option<String>,
    pub playlist: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub playlist_uploader: Option<String>,
    pub playlist_index: Option<usize>,
    pub playlist_count: Option<usize>,
    pub n_entries: Option<usize>,
    pub availability: Option<String>,
}

#[allow(dead_code)]
impl VideoInfo {
    pub fn best_thumbnail(&self) -> Option<&str> {
        self.thumbnail
            .as_deref()
            .or_else(|| self.thumbnails.last().map(|t| t.url.as_str()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Format {
    pub format_id: String,
    pub format_note: Option<String>,
    pub url: String,
    pub manifest_url: Option<String>,
    pub ext: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub tbr: Option<f64>,
    pub abr: Option<f64>,
    pub vbr: Option<f64>,
    pub acodec: Option<String>,
    pub vcodec: Option<String>,
    pub asr: Option<u32>,
    pub audio_channels: Option<u8>,
    pub container: Option<String>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub protocol: String,
    pub language: Option<String>,
    pub quality: Option<f64>,
    pub source_preference: Option<i32>,
    pub has_drm: bool,
    pub dynamic_range: Option<String>,
    pub http_headers: HashMap<String, String>,
}

#[allow(dead_code)]
impl Format {
    pub fn is_video_only(&self) -> bool {
        self.vcodec.as_deref().map_or(false, |v| v != "none")
            && self.acodec.as_deref().map_or(true, |a| a == "none")
    }

    pub fn is_audio_only(&self) -> bool {
        self.acodec.as_deref().map_or(false, |a| a != "none")
            && self.vcodec.as_deref().map_or(true, |v| v == "none")
    }

    pub fn is_combined(&self) -> bool {
        self.vcodec.as_deref().map_or(false, |v| v != "none")
            && self.acodec.as_deref().map_or(false, |a| a != "none")
    }

    pub fn resolution(&self) -> String {
        match (self.width, self.height) {
            (Some(w), Some(h)) => format!("{}x{}", w, h),
            (None, Some(h)) => format!("{}p", h),
            _ => "unknown".to_string(),
        }
    }

    pub fn video_score(&self) -> f64 {
        let height_score = self.height.unwrap_or(0) as f64;
        let fps_score = self.fps.unwrap_or(30.0) / 30.0;
        let bitrate_score = self.vbr.or(self.tbr).unwrap_or(0.0) / 1000.0;
        height_score * 10.0 + fps_score + bitrate_score * 0.1
    }

    pub fn audio_score(&self) -> f64 {
        let abr_score = self.abr.unwrap_or(0.0);
        let asr_score = self.asr.unwrap_or(0) as f64 / 1000.0;
        abr_score + asr_score * 0.1
    }

    pub fn total_score(&self) -> f64 {
        self.video_score() + self.audio_score()
    }

    pub fn approx_filesize(&self) -> Option<u64> {
        self.filesize.or(self.filesize_approx)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thumbnail {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub id: Option<String>,
    pub preference: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtitle {
    pub url: String,
    pub ext: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub format: String,
    pub output_template: String,
    pub extract_audio: bool,
    pub audio_format: String,
    pub skip_download: bool,
    pub quiet: bool,
    pub no_progress: bool,
    pub retries: u32,
    pub concurrent_fragments: usize,
    pub merge_output_format: String,
    pub keep_video: bool,
    pub write_subs: bool,
    pub write_auto_subs: bool,
    pub sub_langs: Vec<String>,
    pub restrict_filenames: bool,
    pub rate_limit: Option<u64>,
    pub proxy: Option<String>,
    pub cookies_file: Option<String>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            format: "bestvideo+bestaudio/best".to_string(),
            output_template: "%(title)s [%(id)s].%(ext)s".to_string(),
            extract_audio: false,
            audio_format: "m4a".to_string(),
            skip_download: false,
            quiet: false,
            no_progress: false,
            retries: 10,
            concurrent_fragments: 4,
            merge_output_format: "mkv".to_string(),
            keep_video: false,
            write_subs: false,
            write_auto_subs: false,
            sub_langs: vec!["en".to_string()],
            restrict_filenames: false,
            rate_limit: None,
            proxy: None,
            cookies_file: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_only() -> Format {
        Format {
            format_id: "137".to_string(),
            ext: "mp4".to_string(),
            vcodec: Some("avc1".to_string()),
            acodec: Some("none".to_string()),
            height: Some(1080),
            width: Some(1920),
            fps: Some(30.0),
            vbr: Some(3000.0),
            tbr: Some(3000.0),
            ..Format::default()
        }
    }

    fn audio_only() -> Format {
        Format {
            format_id: "140".to_string(),
            ext: "m4a".to_string(),
            vcodec: Some("none".to_string()),
            acodec: Some("mp4a".to_string()),
            abr: Some(128.0),
            tbr: Some(128.0),
            asr: Some(44100),
            ..Format::default()
        }
    }

    fn combined() -> Format {
        Format {
            format_id: "18".to_string(),
            ext: "mp4".to_string(),
            vcodec: Some("avc1".to_string()),
            acodec: Some("mp4a".to_string()),
            height: Some(360),
            width: Some(640),
            abr: Some(96.0),
            vbr: Some(350.0),
            tbr: Some(446.0),
            ..Format::default()
        }
    }

    #[test]
    fn is_video_only() {
        assert!(video_only().is_video_only());
        assert!(!audio_only().is_video_only());
        assert!(!combined().is_video_only());
    }

    #[test]
    fn is_audio_only() {
        assert!(!video_only().is_audio_only());
        assert!(audio_only().is_audio_only());
        assert!(!combined().is_audio_only());
    }

    #[test]
    fn is_combined() {
        assert!(!video_only().is_combined());
        assert!(!audio_only().is_combined());
        assert!(combined().is_combined());
    }

    #[test]
    fn video_score_higher_for_higher_res() {
        let v_1080 = video_only(); // 1080p
        let v_360 = combined(); // 360p
        assert!(v_1080.video_score() > v_360.video_score());
    }

    #[test]
    fn audio_score_higher_for_higher_bitrate() {
        let high = Format {
            abr: Some(256.0),
            acodec: Some("opus".to_string()),
            vcodec: Some("none".to_string()),
            ..Format::default()
        };
        let low = Format {
            abr: Some(48.0),
            acodec: Some("opus".to_string()),
            vcodec: Some("none".to_string()),
            ..Format::default()
        };
        assert!(high.audio_score() > low.audio_score());
    }

    #[test]
    fn resolution_string() {
        assert_eq!(video_only().resolution(), "1920x1080");
        assert_eq!(audio_only().resolution(), "unknown");
        assert_eq!(combined().resolution(), "640x360");
    }

    #[test]
    fn approx_filesize_prefers_exact() {
        let f = Format {
            filesize: Some(1000),
            filesize_approx: Some(2000),
            ..Format::default()
        };
        assert_eq!(f.approx_filesize(), Some(1000));
    }

    #[test]
    fn approx_filesize_falls_back_to_approx() {
        let f = Format {
            filesize: None,
            filesize_approx: Some(2000),
            ..Format::default()
        };
        assert_eq!(f.approx_filesize(), Some(2000));
    }

    #[test]
    fn approx_filesize_none_when_both_missing() {
        assert_eq!(Format::default().approx_filesize(), None);
    }
}
