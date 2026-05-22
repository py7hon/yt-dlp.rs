use super::Extractor;
use crate::types::{Format, Subtitle, Thumbnail, VideoInfo};
use crate::utils::{mime_to_ext, parse_mime_codecs};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

const INNERTUBE_URL: &str = "https://www.youtube.com/youtubei/v1/player";
const INNERTUBE_UA: &str =
    "com.google.android.apps.youtube.vr.oculus/1.65.10 (Linux; U; Android 12L; eureka-user Build/SQ3A.220605.009.A1) gzip";

pub struct YoutubeExtractor;

impl YoutubeExtractor {
    pub fn new() -> Self {
        Self
    }

    pub fn extract_video_id(url: &str) -> Option<String> {
        let url = url.trim();

        // Bare 11-char video ID
        if url.len() == 11 && url.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Some(url.to_string());
        }

        let patterns = [
            r"[?&]v=([0-9A-Za-z_-]{11})",
            r"youtu\.be/([0-9A-Za-z_-]{11})",
            r"youtube\.com/(?:shorts|live|embed|v)/([0-9A-Za-z_-]{11})",
            r"youtube\.com/e/([0-9A-Za-z_-]{11})",
            r"youtube\.com/watch/([0-9A-Za-z_-]{11})",
        ];
        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(caps) = re.captures(url) {
                    return caps.get(1).map(|m| m.as_str().to_string());
                }
            }
        }
        None
    }

    async fn fetch_player_response(
        &self,
        video_id: &str,
        client: &Client,
    ) -> Result<PlayerResponse> {
        let body = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "ANDROID_VR",
                    "clientVersion": "1.65.10",
                    "deviceMake": "Oculus",
                    "deviceModel": "Quest 3",
                    "androidSdkVersion": 32,
                    "userAgent": INNERTUBE_UA,
                    "osName": "Android",
                    "osVersion": "12L"
                }
            },
            "videoId": video_id,
            "playbackContext": {
                "contentPlaybackContext": {
                    "html5Preference": "HTML5_PREF_WANTS"
                }
            },
            "params": "CgIQBg=="
        });

        let resp = client
            .post(INNERTUBE_URL)
            .header("User-Agent", INNERTUBE_UA)
            .header("Content-Type", "application/json")
            .header("X-YouTube-Client-Name", "28")
            .header("X-YouTube-Client-Version", "1.65.10")
            .header("Origin", "https://www.youtube.com")
            .json(&body)
            .send()
            .await
            .context("Failed to call YouTube InnerTube API")?;

        if !resp.status().is_success() {
            return Err(anyhow!(
                "InnerTube API returned HTTP {}",
                resp.status()
            ));
        }

        resp.json::<PlayerResponse>()
            .await
            .context("Failed to parse InnerTube API response")
    }

    async fn fetch_webpage_info(
        &self,
        video_id: &str,
        client: &Client,
    ) -> Option<WebpageInfo> {
        let url = format!("https://www.youtube.com/watch?v={}", video_id);
        let resp = client
            .get(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .ok()?;

        let text = resp.text().await.ok()?;

        // Extract ytInitialData
        let likes = extract_like_count(&text);
        let categories = extract_categories(&text);
        let channel_url = extract_channel_url(&text);

        Some(WebpageInfo {
            like_count: likes,
            categories,
            channel_url,
        })
    }

    fn parse_formats(&self, streaming_data: &StreamingData, _duration_secs: Option<f64>) -> Vec<Format> {
        let mut formats = Vec::new();

        let all_formats = streaming_data
            .formats
            .iter()
            .chain(streaming_data.adaptive_formats.iter())
            .flatten();

        for sf in all_formats {
            let url = match sf.url.as_deref() {
                Some(u) => u.to_string(),
                None => {
                    // signatureCipher — parse out the URL at minimum
                    if let Some(cipher) = &sf.signature_cipher {
                        if let Some(url) = parse_cipher_url(cipher) {
                            url
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
            };

            let mime_type = &sf.mime_type;
            let ext = mime_to_ext(mime_type).to_string();
            let (vcodec, acodec) = parse_mime_codecs(mime_type);

            // Determine if it's video-only, audio-only, or combined
            let is_video_container = mime_type.starts_with("video/");
            let is_audio_container = mime_type.starts_with("audio/");

            let (vcodec, acodec) = match (vcodec, acodec) {
                (Some(v), Some(a)) => (Some(v), Some(a)),
                (Some(v), None) if is_video_container => (Some(v), Some("none".to_string())),
                (None, Some(a)) if is_audio_container => (Some("none".to_string()), Some(a)),
                (Some(v), None) if is_audio_container => (Some("none".to_string()), Some(v)),
                other => other,
            };

            // For combined formats, both codecs should be set
            let final_ext = if is_audio_container
                && acodec.as_deref() != Some("none")
                && vcodec.as_deref().map_or(true, |v| v == "none")
            {
                // Pure audio
                if ext == "mp4" { "m4a".to_string() } else { ext }
            } else {
                ext
            };

            let abr = if is_audio_container || acodec.as_deref().map_or(false, |a| a != "none") {
                sf.bitrate
                    .or(sf.average_bitrate)
                    .map(|b| b as f64 / 1000.0)
            } else {
                None
            };

            let vbr = if is_video_container && vcodec.as_deref().map_or(false, |v| v != "none") {
                if is_audio_container || acodec.as_deref().map_or(true, |a| a == "none") {
                    sf.bitrate.or(sf.average_bitrate).map(|b| b as f64 / 1000.0)
                } else {
                    // Combined: estimate video bitrate
                    sf.average_bitrate.map(|b| b as f64 / 1000.0 * 0.85)
                }
            } else {
                None
            };

            let tbr = sf
                .average_bitrate
                .or(sf.bitrate)
                .map(|b| b as f64 / 1000.0);

            let filesize = sf.content_length.as_deref().and_then(|s| s.parse().ok());
            let filesize_approx = sf.approx_duration_ms.as_deref().and_then(|ms| {
                let ms: f64 = ms.parse().ok()?;
                let bitrate = sf.average_bitrate.or(sf.bitrate)?;
                Some((ms / 1000.0 * bitrate as f64 / 8.0) as u64)
            });

            let asr: Option<u32> = sf.audio_sample_rate.as_deref().and_then(|s| s.parse().ok());

            // Dynamic range
            let dynamic_range = sf.color_info.as_ref().and_then(|ci| {
                ci.primaries.as_deref().map(|p| match p {
                    "COLOR_PRIMARIES_BT2020" => "HDR10",
                    _ => "SDR",
                })
            });

            let quality_note = sf.quality_label.clone().or_else(|| {
                sf.audio_quality.as_deref().map(|q| match q {
                    "AUDIO_QUALITY_LOW" => "low",
                    "AUDIO_QUALITY_MEDIUM" => "medium",
                    "AUDIO_QUALITY_HIGH" => "high",
                    other => other,
                }.to_string())
            });

            let mut http_headers = HashMap::new();
            http_headers.insert(
                "User-Agent".to_string(),
                INNERTUBE_UA.to_string(),
            );

            formats.push(Format {
                format_id: sf.itag.to_string(),
                format_note: quality_note,
                url,
                manifest_url: None,
                ext: final_ext,
                width: sf.width,
                height: sf.height,
                fps: sf.fps.map(|f| f as f64),
                tbr,
                abr,
                vbr,
                acodec,
                vcodec,
                asr,
                audio_channels: sf.audio_channels,
                container: None,
                filesize,
                filesize_approx,
                protocol: "https".to_string(),
                language: None,
                quality: Some(sf.height.unwrap_or(0) as f64),
                source_preference: None,
                has_drm: false,
                dynamic_range: dynamic_range.map(|s| s.to_string()),
                http_headers,
            });
        }

        // Add HLS formats if available
        if let Some(hls_url) = &streaming_data.hls_manifest_url {
            let mut headers = HashMap::new();
            headers.insert("User-Agent".to_string(), INNERTUBE_UA.to_string());
            formats.push(Format {
                format_id: "hls-best".to_string(),
                format_note: Some("HLS best".to_string()),
                url: hls_url.clone(),
                ext: "mp4".to_string(),
                protocol: "hls".to_string(),
                vcodec: Some("avc1".to_string()),
                acodec: Some("mp4a".to_string()),
                quality: Some(720.0),
                http_headers: headers,
                ..Format::default()
            });
        }

        formats
    }
}

fn parse_cipher_url(cipher: &str) -> Option<String> {
    let params: HashMap<String, String> = url::form_urlencoded::parse(cipher.as_bytes())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    params.get("url").cloned()
}

fn extract_like_count(html: &str) -> Option<u64> {
    let re = Regex::new(r#""defaultText":\{"accessibility":\{"accessibilityData":\{"label":"([0-9,]+) likes"\}\}"#)
        .ok()?;
    let caps = re.captures(html)?;
    let s = caps.get(1)?.as_str().replace(',', "");
    s.parse().ok()
}

fn extract_categories(html: &str) -> Vec<String> {
    let re = Regex::new(r#""category":"([^"]+)""#).ok();
    re.and_then(|re| {
        re.captures(html)
            .and_then(|c| c.get(1))
            .map(|m| vec![m.as_str().to_string()])
    })
    .unwrap_or_default()
}

fn extract_channel_url(html: &str) -> Option<String> {
    let re = Regex::new(r#""channelUrl":"(https://www\.youtube\.com/channel/[^"]+)""#).ok()?;
    re.captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn parse_upload_date(date: &str) -> Option<String> {
    // Input: "2023-04-15T..." or "20230415" → output "20230415"
    if date.len() >= 10 && date.contains('-') {
        Some(date[..10].replace('-', ""))
    } else if date.len() == 8 {
        Some(date.to_string())
    } else {
        None
    }
}

#[async_trait]
impl Extractor for YoutubeExtractor {
    fn name(&self) -> &str {
        "youtube"
    }

    fn suitable(&self, url: &str) -> bool {
        Self::extract_video_id(url).is_some()
    }

    async fn extract(&self, url: &str, client: &Client) -> Result<VideoInfo> {
        let video_id = Self::extract_video_id(url)
            .ok_or_else(|| anyhow!("Could not extract YouTube video ID from: {}", url))?;

        let player = self
            .fetch_player_response(&video_id, client)
            .await
            .context("Failed to fetch player response")?;

        // Check playability
        if let Some(ps) = &player.playability_status {
            match ps.status.as_str() {
                "OK" => {}
                "LOGIN_REQUIRED" => {
                    return Err(anyhow!(
                        "This video requires login. Reason: {}",
                        ps.reason.as_deref().unwrap_or("unknown")
                    ));
                }
                "UNPLAYABLE" => {
                    return Err(anyhow!(
                        "Video is unplayable: {}",
                        ps.reason.as_deref().unwrap_or("unknown")
                    ));
                }
                "ERROR" | "CONTENT_CHECK_REQUIRED" => {
                    return Err(anyhow!(
                        "Video error: {}",
                        ps.reason.as_deref().unwrap_or(ps.status.as_str())
                    ));
                }
                other => {
                    return Err(anyhow!("Unexpected playability status: {}", other));
                }
            }
        }

        let vd = player.video_details.as_ref()
            .ok_or_else(|| anyhow!("No video details in response"))?;

        let duration = vd
            .length_seconds
            .as_deref()
            .and_then(|s| s.parse::<f64>().ok());

        let formats = player
            .streaming_data
            .as_ref()
            .map(|sd| self.parse_formats(sd, duration))
            .unwrap_or_default();

        if formats.is_empty() {
            return Err(anyhow!(
                "No downloadable formats found. The video may be age-restricted or unavailable."
            ));
        }

        let thumbnails: Vec<Thumbnail> = vd
            .thumbnail
            .as_ref()
            .map(|tl| {
                tl.thumbnails
                    .iter()
                    .enumerate()
                    .map(|(i, t)| Thumbnail {
                        url: t.url.clone(),
                        width: t.width,
                        height: t.height,
                        id: Some(i.to_string()),
                        preference: Some(i as i32),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let best_thumbnail = thumbnails.last().map(|t| t.url.clone())
            .or_else(|| {
                Some(format!(
                    "https://i.ytimg.com/vi/{}/maxresdefault.jpg",
                    video_id
                ))
            });

        // Parse subtitles
        let mut subtitles: HashMap<String, Vec<Subtitle>> = HashMap::new();
        let mut auto_captions: HashMap<String, Vec<Subtitle>> = HashMap::new();

        if let Some(captions) = &player.captions {
            if let Some(renderer) = &captions.player_captions_tracklist_renderer {
                if let Some(tracks) = &renderer.caption_tracks {
                    for track in tracks {
                        let lang = track
                            .language_code
                            .clone()
                            .unwrap_or_else(|| "und".to_string());
                        let is_auto = track
                            .vss_id
                            .as_deref()
                            .map_or(false, |id| id.starts_with('a'));

                        // Add different subtitle formats via URL manipulation
                        for fmt in &["vtt", "srv1", "ttml"] {
                            let sub_url = format!(
                                "{}&fmt={}",
                                track.base_url, fmt
                            );
                            let entry = Subtitle {
                                url: sub_url,
                                ext: fmt.to_string(),
                                name: track.name_simple.clone(),
                            };
                            if is_auto {
                                auto_captions.entry(lang.clone()).or_default().push(entry);
                            } else {
                                subtitles.entry(lang.clone()).or_default().push(entry);
                            }
                        }
                    }
                }
            }
        }

        // Extract upload date from microformat
        let upload_date = player
            .microformat
            .as_ref()
            .and_then(|m| m.player_microformat_renderer.as_ref())
            .and_then(|r| r.upload_date.as_deref().or(r.publish_date.as_deref()))
            .and_then(parse_upload_date);

        let categories = player
            .microformat
            .as_ref()
            .and_then(|m| m.player_microformat_renderer.as_ref())
            .and_then(|r| r.category.as_ref())
            .map(|c| vec![c.clone()])
            .unwrap_or_default();

        let view_count = vd
            .view_count
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok());

        // Fetch supplemental page info for likes/channel URL
        let webpage = self.fetch_webpage_info(&video_id, client).await;

        let channel_url = webpage.as_ref().and_then(|w| w.channel_url.clone()).or_else(|| {
            vd.channel_id.as_deref().map(|id| {
                format!("https://www.youtube.com/channel/{}", id)
            })
        });

        let channel_id = vd.channel_id.clone();
        let author = vd.author.clone();

        Ok(VideoInfo {
            id: video_id.clone(),
            title: vd.title.clone(),
            webpage_url: format!("https://www.youtube.com/watch?v={}", video_id),
            extractor: "youtube".to_string(),
            description: vd.short_description.clone(),
            duration,
            uploader: author.clone(),
            uploader_id: None,
            channel: author,
            channel_id,
            channel_url,
            view_count,
            like_count: webpage.and_then(|w| w.like_count),
            comment_count: None,
            upload_date,
            timestamp: None,
            thumbnail: best_thumbnail,
            thumbnails,
            formats,
            subtitles,
            automatic_captions: auto_captions,
            tags: vd.keywords.clone().unwrap_or_default(),
            categories,
            age_limit: if vd.is_crawlable == Some(false) { Some(18) } else { Some(0) },
            is_live: Some(vd.is_live_content.unwrap_or(false)),
            live_status: if vd.is_live_content.unwrap_or(false) {
                Some("is_live".to_string())
            } else {
                Some("not_live".to_string())
            },
            ..VideoInfo::default()
        })
    }
}

#[allow(dead_code)]
struct WebpageInfo {
    like_count: Option<u64>,
    categories: Vec<String>,
    channel_url: Option<String>,
}

// ─── InnerTube API deserialization types ─────────────────────────────────────

#[derive(Deserialize, Debug)]
struct PlayerResponse {
    #[serde(rename = "videoDetails")]
    video_details: Option<VideoDetails>,
    #[serde(rename = "streamingData")]
    streaming_data: Option<StreamingData>,
    #[serde(rename = "playabilityStatus")]
    playability_status: Option<PlayabilityStatus>,
    captions: Option<Captions>,
    microformat: Option<Microformat>,
}

#[derive(Deserialize, Debug)]
struct PlayabilityStatus {
    status: String,
    reason: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct VideoDetails {
    #[serde(rename = "videoId")]
    video_id: String,
    title: String,
    #[serde(rename = "shortDescription")]
    short_description: Option<String>,
    #[serde(rename = "lengthSeconds")]
    length_seconds: Option<String>,
    #[serde(rename = "viewCount")]
    view_count: Option<String>,
    author: Option<String>,
    #[serde(rename = "channelId")]
    channel_id: Option<String>,
    #[serde(rename = "isLiveContent")]
    is_live_content: Option<bool>,
    thumbnail: Option<ThumbnailList>,
    keywords: Option<Vec<String>>,
    #[serde(rename = "isCrawlable")]
    is_crawlable: Option<bool>,
}

#[derive(Deserialize, Debug)]
struct ThumbnailList {
    thumbnails: Vec<ThumbnailItem>,
}

#[derive(Deserialize, Debug)]
struct ThumbnailItem {
    url: String,
    width: Option<u32>,
    height: Option<u32>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct StreamingData {
    formats: Option<Vec<StreamingFormat>>,
    #[serde(rename = "adaptiveFormats")]
    adaptive_formats: Option<Vec<StreamingFormat>>,
    #[serde(rename = "hlsManifestUrl")]
    hls_manifest_url: Option<String>,
    #[serde(rename = "dashManifestUrl")]
    dash_manifest_url: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct StreamingFormat {
    itag: u32,
    url: Option<String>,
    #[serde(rename = "signatureCipher")]
    signature_cipher: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: String,
    bitrate: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<u32>,
    #[serde(rename = "qualityLabel")]
    quality_label: Option<String>,
    quality: Option<String>,
    #[serde(rename = "audioQuality")]
    audio_quality: Option<String>,
    #[serde(rename = "audioSampleRate")]
    audio_sample_rate: Option<String>,
    #[serde(rename = "audioChannels")]
    audio_channels: Option<u8>,
    #[serde(rename = "contentLength")]
    content_length: Option<String>,
    #[serde(rename = "averageBitrate")]
    average_bitrate: Option<u64>,
    #[serde(rename = "approxDurationMs")]
    approx_duration_ms: Option<String>,
    #[serde(rename = "colorInfo")]
    color_info: Option<ColorInfo>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct ColorInfo {
    primaries: Option<String>,
    #[serde(rename = "transferCharacteristics")]
    transfer_characteristics: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Captions {
    #[serde(rename = "playerCaptionsTracklistRenderer")]
    player_captions_tracklist_renderer: Option<CaptionsTracklistRenderer>,
}

#[derive(Deserialize, Debug)]
struct CaptionsTracklistRenderer {
    #[serde(rename = "captionTracks")]
    caption_tracks: Option<Vec<CaptionTrack>>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct CaptionTrack {
    #[serde(rename = "baseUrl")]
    base_url: String,
    name: Option<serde_json::Value>,
    #[serde(rename = "simpleText")]
    name_simple: Option<String>,
    #[serde(rename = "vssId")]
    vss_id: Option<String>,
    #[serde(rename = "languageCode")]
    language_code: Option<String>,
    kind: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Microformat {
    #[serde(rename = "playerMicroformatRenderer")]
    player_microformat_renderer: Option<PlayerMicroformatRenderer>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct PlayerMicroformatRenderer {
    #[serde(rename = "uploadDate")]
    upload_date: Option<String>,
    #[serde(rename = "publishDate")]
    publish_date: Option<String>,
    category: Option<String>,
    #[serde(rename = "isFamilySafe")]
    is_family_safe: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── extract_video_id ─────────────────────────────────────────────────

    #[test]
    fn bare_11_char_id() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn watch_url() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn watch_url_with_extra_params() {
        assert_eq!(
            YoutubeExtractor::extract_video_id(
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=30s&list=PLabc"
            ),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn youtu_be_url() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("https://youtu.be/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn youtu_be_with_timestamp() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("https://youtu.be/dQw4w9WgXcQ?t=42"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn shorts_url() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn live_url() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("https://www.youtube.com/live/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn embed_url() {
        assert_eq!(
            YoutubeExtractor::extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn mobile_url() {
        // m.youtube.com watch URL
        assert_eq!(
            YoutubeExtractor::extract_video_id("https://m.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn non_youtube_url_returns_none() {
        assert_eq!(YoutubeExtractor::extract_video_id("https://vimeo.com/12345"), None);
    }

    #[test]
    fn too_short_id_returns_none() {
        assert_eq!(YoutubeExtractor::extract_video_id("short"), None);
    }

    #[test]
    fn too_long_id_returns_none() {
        assert_eq!(YoutubeExtractor::extract_video_id("toolongidhere12"), None);
    }

    #[test]
    fn suitable_returns_true_for_youtube() {
        let ext = YoutubeExtractor::new();
        assert!(ext.suitable("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(ext.suitable("dQw4w9WgXcQ"));
        assert!(ext.suitable("https://youtu.be/dQw4w9WgXcQ"));
    }

    #[test]
    fn suitable_returns_false_for_non_youtube() {
        let ext = YoutubeExtractor::new();
        assert!(!ext.suitable("https://vimeo.com/12345678"));
        assert!(!ext.suitable("https://example.com"));
    }
}
