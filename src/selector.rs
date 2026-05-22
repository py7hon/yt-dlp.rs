use crate::types::Format;
use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub enum FormatSelector {
    Best,
    Worst,
    BestVideo,
    WorstVideo,
    BestAudio,
    WorstAudio,
    ById(String),
    Merge(Box<FormatSelector>, Box<FormatSelector>),
    Fallback(Box<FormatSelector>, Box<FormatSelector>),
    Filtered(Box<FormatSelector>, Vec<FormatFilter>),
}

#[derive(Debug, Clone)]
pub struct FormatFilter {
    pub field: String,
    pub op: FilterOp,
    pub value: FilterValue,
}

#[derive(Debug, Clone)]
pub enum FilterOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
pub enum FilterValue {
    Str(String),
    Num(f64),
}

/// Result of format selection — either a single format or two to merge.
#[derive(Debug)]
pub enum SelectedFormats<'a> {
    Single(&'a Format),
    Merge(&'a Format, &'a Format),
}

pub fn parse_selector(spec: &str) -> Result<FormatSelector> {
    // Handle fallback: "bestvideo+bestaudio/best"
    if let Some(slash_pos) = find_slash(spec) {
        let left = parse_selector(spec[..slash_pos].trim())?;
        let right = parse_selector(spec[slash_pos + 1..].trim())?;
        return Ok(FormatSelector::Fallback(Box::new(left), Box::new(right)));
    }

    // Handle merge: "bestvideo+bestaudio"
    if let Some(plus_pos) = find_plus(spec) {
        let left = parse_selector(spec[..plus_pos].trim())?;
        let right = parse_selector(spec[plus_pos + 1..].trim())?;
        return Ok(FormatSelector::Merge(Box::new(left), Box::new(right)));
    }

    // Handle filters: "best[ext=mp4]" or "bestvideo[height<=1080]"
    if let Some(bracket_pos) = spec.find('[') {
        let base = &spec[..bracket_pos];
        let rest = &spec[bracket_pos..];
        let filters = parse_filters(rest)?;
        let base_sel = parse_selector_keyword(base)?;
        return Ok(FormatSelector::Filtered(Box::new(base_sel), filters));
    }

    parse_selector_keyword(spec)
}

fn parse_selector_keyword(s: &str) -> Result<FormatSelector> {
    match s.trim() {
        "best" | "b" => Ok(FormatSelector::Best),
        "worst" | "w" => Ok(FormatSelector::Worst),
        "bestvideo" | "bv" => Ok(FormatSelector::BestVideo),
        "worstvideo" | "wv" => Ok(FormatSelector::WorstVideo),
        "bestaudio" | "ba" => Ok(FormatSelector::BestAudio),
        "worstaudio" | "wa" => Ok(FormatSelector::WorstAudio),
        id => Ok(FormatSelector::ById(id.to_string())),
    }
}

fn find_slash(s: &str) -> Option<usize> {
    // Find '/' not inside brackets
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '/' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn find_plus(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '+' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn parse_filters(s: &str) -> Result<Vec<FormatFilter>> {
    // s looks like "[ext=mp4][height<=1080]"
    let mut filters = Vec::new();
    let mut remaining = s;

    while let Some(open) = remaining.find('[') {
        let close = remaining[open..]
            .find(']')
            .ok_or_else(|| anyhow!("Unclosed '[' in filter"))?
            + open;
        let filter_str = &remaining[open + 1..close];
        filters.push(parse_filter(filter_str)?);
        remaining = &remaining[close + 1..];
    }

    Ok(filters)
}

fn parse_filter(s: &str) -> Result<FormatFilter> {
    let ops = ["<=", ">=", "!=", "=", "<", ">"];
    for op_str in ops {
        if let Some(pos) = s.find(op_str) {
            let field = s[..pos].trim().to_string();
            let value_str = s[pos + op_str.len()..].trim();
            let op = match op_str {
                "=" => FilterOp::Eq,
                "!=" => FilterOp::Ne,
                "<" => FilterOp::Lt,
                "<=" => FilterOp::Le,
                ">" => FilterOp::Gt,
                ">=" => FilterOp::Ge,
                _ => unreachable!(),
            };
            let value = if let Ok(n) = value_str.parse::<f64>() {
                FilterValue::Num(n)
            } else {
                FilterValue::Str(value_str.to_string())
            };
            return Ok(FormatFilter { field, op, value });
        }
    }
    Err(anyhow!("Cannot parse filter: {}", s))
}

fn format_matches_filter(fmt: &Format, filter: &FormatFilter) -> bool {
    let field_val: Option<FilterValue> = match filter.field.as_str() {
        "ext" | "extension" => Some(FilterValue::Str(fmt.ext.clone())),
        "height" => fmt.height.map(|h| FilterValue::Num(h as f64)),
        "width" => fmt.width.map(|w| FilterValue::Num(w as f64)),
        "fps" => fmt.fps.map(FilterValue::Num),
        "tbr" => fmt.tbr.map(FilterValue::Num),
        "abr" => fmt.abr.map(FilterValue::Num),
        "vbr" => fmt.vbr.map(FilterValue::Num),
        "asr" => fmt.asr.map(|a| FilterValue::Num(a as f64)),
        "vcodec" => fmt.vcodec.clone().map(FilterValue::Str),
        "acodec" => fmt.acodec.clone().map(FilterValue::Str),
        "protocol" => Some(FilterValue::Str(fmt.protocol.clone())),
        "format_id" => Some(FilterValue::Str(fmt.format_id.clone())),
        _ => None,
    };

    let Some(fv) = field_val else { return false };

    match (&fv, &filter.value, &filter.op) {
        (FilterValue::Str(a), FilterValue::Str(b), FilterOp::Eq) => a == b,
        (FilterValue::Str(a), FilterValue::Str(b), FilterOp::Ne) => a != b,
        (FilterValue::Num(a), FilterValue::Num(b), FilterOp::Eq) => a == b,
        (FilterValue::Num(a), FilterValue::Num(b), FilterOp::Ne) => a != b,
        (FilterValue::Num(a), FilterValue::Num(b), FilterOp::Lt) => a < b,
        (FilterValue::Num(a), FilterValue::Num(b), FilterOp::Le) => a <= b,
        (FilterValue::Num(a), FilterValue::Num(b), FilterOp::Gt) => a > b,
        (FilterValue::Num(a), FilterValue::Num(b), FilterOp::Ge) => a >= b,
        _ => false,
    }
}

pub fn select_formats<'a>(
    selector: &FormatSelector,
    formats: &'a [Format],
) -> Result<SelectedFormats<'a>> {
    match selector {
        FormatSelector::Fallback(left, right) => {
            if let Ok(result) = select_formats(left, formats) {
                Ok(result)
            } else {
                select_formats(right, formats)
            }
        }

        FormatSelector::Merge(video_sel, audio_sel) => {
            // For merge, we need one video and one audio
            // Convert to single selections
            let video = select_best_single(video_sel, formats)?;
            let audio = select_best_single(audio_sel, formats)?;
            Ok(SelectedFormats::Merge(video, audio))
        }

        _ => {
            let fmt = select_best_single(selector, formats)?;
            Ok(SelectedFormats::Single(fmt))
        }
    }
}

fn select_best_single<'a>(
    selector: &FormatSelector,
    formats: &'a [Format],
) -> Result<&'a Format> {
    match selector {
        FormatSelector::Best => {
            // Prefer combined (video+audio) formats; fall back to any if none exist
            let combined: Vec<&Format> = formats
                .iter()
                .filter(|f| !f.has_drm && f.is_combined())
                .collect();
            let pool = if combined.is_empty() { formats.iter().filter(|f| !f.has_drm).collect::<Vec<_>>() } else { combined };
            pool.into_iter()
                .max_by(|a, b| {
                    a.total_score()
                        .partial_cmp(&b.total_score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or_else(|| anyhow!("No formats available"))
        }

        FormatSelector::Worst => {
            let combined: Vec<&Format> = formats
                .iter()
                .filter(|f| !f.has_drm && f.is_combined())
                .collect();
            let pool = if combined.is_empty() { formats.iter().filter(|f| !f.has_drm).collect::<Vec<_>>() } else { combined };
            pool.into_iter()
                .min_by(|a, b| {
                    a.total_score()
                        .partial_cmp(&b.total_score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or_else(|| anyhow!("No formats available"))
        }

        FormatSelector::BestVideo => formats
            .iter()
            .filter(|f| !f.has_drm && (f.is_video_only() || f.is_combined()))
            .max_by(|a, b| {
                a.video_score()
                    .partial_cmp(&b.video_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| anyhow!("No video formats available")),

        FormatSelector::WorstVideo => formats
            .iter()
            .filter(|f| !f.has_drm && (f.is_video_only() || f.is_combined()))
            .min_by(|a, b| {
                a.video_score()
                    .partial_cmp(&b.video_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| anyhow!("No video formats available")),

        FormatSelector::BestAudio => {
            // Prefer audio-only (matches yt-dlp behaviour); fall back to combined
            let audio_only: Vec<&Format> = formats
                .iter()
                .filter(|f| !f.has_drm && f.is_audio_only())
                .collect();
            let pool: Vec<&Format> = if audio_only.is_empty() {
                formats.iter().filter(|f| !f.has_drm && f.is_combined()).collect()
            } else {
                audio_only
            };
            pool.into_iter()
                .max_by(|a, b| {
                    a.audio_score()
                        .partial_cmp(&b.audio_score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or_else(|| anyhow!("No audio formats available"))
        }

        FormatSelector::WorstAudio => {
            let audio_only: Vec<&Format> = formats
                .iter()
                .filter(|f| !f.has_drm && f.is_audio_only())
                .collect();
            let pool: Vec<&Format> = if audio_only.is_empty() {
                formats.iter().filter(|f| !f.has_drm && f.is_combined()).collect()
            } else {
                audio_only
            };
            pool.into_iter()
                .min_by(|a, b| {
                    a.audio_score()
                        .partial_cmp(&b.audio_score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or_else(|| anyhow!("No audio formats available"))
        }

        FormatSelector::ById(id) => {
            // Try comma-separated list like "137+140" — take first match
            let ids: Vec<&str> = id.split('+').collect();
            if ids.len() > 1 {
                // This is actually a merge — shouldn't reach here directly
                return formats
                    .iter()
                    .find(|f| f.format_id == ids[0])
                    .ok_or_else(|| anyhow!("Format {} not found", ids[0]));
            }
            formats
                .iter()
                .find(|f| f.format_id == id.as_str())
                .ok_or_else(|| anyhow!("Format {} not found", id))
        }

        FormatSelector::Filtered(base, filters) => {
            let filtered: Vec<&Format> = formats
                .iter()
                .filter(|f| filters.iter().all(|filter| format_matches_filter(f, filter)))
                .collect();
            select_best_single(base, &filtered.into_iter().cloned().collect::<Vec<_>>())
                // Re-borrow from original since we cloned
                .and_then(|found| {
                    formats
                        .iter()
                        .find(|f| f.format_id == found.format_id)
                        .ok_or_else(|| anyhow!("Format not found after filter"))
                })
        }

        FormatSelector::Merge(_, _) | FormatSelector::Fallback(_, _) => {
            unreachable!("select_best_single called with compound selector")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Format;

    fn fmt(
        id: &str,
        ext: &str,
        vcodec: &str,
        acodec: &str,
        height: Option<u32>,
        abr: Option<f64>,
        tbr: Option<f64>,
    ) -> Format {
        Format {
            format_id: id.to_string(),
            ext: ext.to_string(),
            vcodec: Some(vcodec.to_string()),
            acodec: Some(acodec.to_string()),
            height,
            tbr,
            abr,
            ..Format::default()
        }
    }

    fn sample_formats() -> Vec<Format> {
        vec![
            // video-only adaptive
            fmt("313", "webm", "vp9", "none", Some(2160), None, Some(13000.0)),
            fmt("137", "mp4", "avc1", "none", Some(1080), None, Some(3000.0)),
            fmt("136", "mp4", "avc1", "none", Some(720), None, Some(1000.0)),
            fmt("134", "mp4", "avc1", "none", Some(360), None, Some(300.0)),
            // audio-only adaptive
            fmt("251", "webm", "none", "opus", None, Some(130.0), Some(130.0)),
            fmt("140", "m4a", "none", "mp4a", None, Some(128.0), Some(128.0)),
            fmt("249", "webm", "none", "opus", None, Some(48.0), Some(48.0)),
            // combined (video+audio)
            fmt("18", "mp4", "avc1", "mp4a", Some(360), Some(96.0), Some(444.0)),
            fmt("22", "mp4", "avc1", "mp4a", Some(720), Some(192.0), Some(1100.0)),
        ]
    }

    // ─── parse_selector ───────────────────────────────────────────────────

    #[test]
    fn parse_keywords() {
        assert!(matches!(parse_selector("best").unwrap(), FormatSelector::Best));
        assert!(matches!(parse_selector("worst").unwrap(), FormatSelector::Worst));
        assert!(matches!(parse_selector("bestvideo").unwrap(), FormatSelector::BestVideo));
        assert!(matches!(parse_selector("worstvideo").unwrap(), FormatSelector::WorstVideo));
        assert!(matches!(parse_selector("bestaudio").unwrap(), FormatSelector::BestAudio));
        assert!(matches!(parse_selector("worstaudio").unwrap(), FormatSelector::WorstAudio));
    }

    #[test]
    fn parse_short_keywords() {
        assert!(matches!(parse_selector("b").unwrap(), FormatSelector::Best));
        assert!(matches!(parse_selector("bv").unwrap(), FormatSelector::BestVideo));
        assert!(matches!(parse_selector("ba").unwrap(), FormatSelector::BestAudio));
        assert!(matches!(parse_selector("wv").unwrap(), FormatSelector::WorstVideo));
        assert!(matches!(parse_selector("wa").unwrap(), FormatSelector::WorstAudio));
    }

    #[test]
    fn parse_merge() {
        let sel = parse_selector("bestvideo+bestaudio").unwrap();
        assert!(matches!(sel, FormatSelector::Merge(_, _)));
    }

    #[test]
    fn parse_fallback() {
        let sel = parse_selector("bestvideo+bestaudio/best").unwrap();
        assert!(matches!(sel, FormatSelector::Fallback(_, _)));
    }

    #[test]
    fn parse_by_id() {
        let sel = parse_selector("137").unwrap();
        assert!(matches!(sel, FormatSelector::ById(ref id) if id == "137"));
    }

    #[test]
    fn parse_filter_eq() {
        let sel = parse_selector("best[ext=mp4]").unwrap();
        assert!(matches!(sel, FormatSelector::Filtered(_, ref filters) if
            filters.len() == 1 && filters[0].field == "ext"
        ));
    }

    #[test]
    fn parse_filter_le() {
        let sel = parse_selector("bestvideo[height<=1080]").unwrap();
        assert!(matches!(sel, FormatSelector::Filtered(_, ref f) if
            f[0].field == "height" && matches!(f[0].op, FilterOp::Le)
        ));
    }

    #[test]
    fn parse_filter_chained() {
        let sel = parse_selector("bestvideo[height<=1080][ext=mp4]").unwrap();
        assert!(matches!(sel, FormatSelector::Filtered(_, ref f) if f.len() == 2));
    }

    #[test]
    fn parse_fallback_contains_merge() {
        // "bestvideo+bestaudio/best" — Fallback wrapping a Merge
        let sel = parse_selector("bestvideo+bestaudio/best").unwrap();
        if let FormatSelector::Fallback(left, _right) = sel {
            assert!(matches!(*left, FormatSelector::Merge(_, _)));
        } else {
            panic!("expected Fallback");
        }
    }

    // ─── select_formats ───────────────────────────────────────────────────

    #[test]
    fn best_picks_combined() {
        let fmts = sample_formats();
        let sel = parse_selector("best").unwrap();
        let res = select_formats(&sel, &fmts).unwrap();
        // best must be combined (has both codecs)
        if let SelectedFormats::Single(f) = res {
            assert!(f.is_combined(), "expected combined, got {:?}", f.format_id);
        } else {
            panic!("expected Single");
        }
    }

    #[test]
    fn best_picks_highest_combined() {
        let fmts = sample_formats();
        let sel = parse_selector("best").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            // "22" (720p combined) should score higher than "18" (360p combined)
            assert_eq!(f.format_id, "22");
        }
    }

    #[test]
    fn bestvideo_picks_video_only_over_combined() {
        let fmts = sample_formats();
        let sel = parse_selector("bestvideo").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            // 2160p vp9 should win
            assert_eq!(f.format_id, "313");
        }
    }

    #[test]
    fn bestaudio_picks_highest_abr() {
        let fmts = sample_formats();
        let sel = parse_selector("bestaudio").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            // 251 (130kbps opus) beats 140 (128kbps m4a)
            assert_eq!(f.format_id, "251");
        }
    }

    #[test]
    fn worstaudio_picks_lowest_abr() {
        let fmts = sample_formats();
        let sel = parse_selector("worstaudio").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            assert_eq!(f.format_id, "249");
        }
    }

    #[test]
    fn select_by_id() {
        let fmts = sample_formats();
        let sel = parse_selector("137").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            assert_eq!(f.format_id, "137");
        }
    }

    #[test]
    fn select_by_id_missing_returns_err() {
        let fmts = sample_formats();
        let sel = parse_selector("9999").unwrap();
        assert!(select_formats(&sel, &fmts).is_err());
    }

    #[test]
    fn merge_returns_video_and_audio() {
        let fmts = sample_formats();
        let sel = parse_selector("bestvideo+bestaudio").unwrap();
        if let SelectedFormats::Merge(video, audio) = select_formats(&sel, &fmts).unwrap() {
            assert!(video.is_video_only() || video.is_combined());
            assert!(audio.is_audio_only() || audio.is_combined());
        } else {
            panic!("expected Merge");
        }
    }

    #[test]
    fn fallback_uses_right_when_left_fails() {
        let fmts = sample_formats();
        // "9999" doesn't exist, so it should fall back to "best"
        let sel = parse_selector("9999/best").unwrap();
        let res = select_formats(&sel, &fmts).unwrap();
        assert!(matches!(res, SelectedFormats::Single(_)));
    }

    #[test]
    fn filter_by_ext() {
        let fmts = sample_formats();
        let sel = parse_selector("bestvideo[ext=mp4]").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            assert_eq!(f.ext, "mp4");
        }
    }

    #[test]
    fn filter_by_height_le() {
        let fmts = sample_formats();
        let sel = parse_selector("bestvideo[height<=720]").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            assert!(f.height.unwrap_or(9999) <= 720);
        }
    }

    #[test]
    fn drm_formats_are_excluded() {
        let fmts = vec![Format {
            format_id: "1".to_string(),
            ext: "mp4".to_string(),
            height: Some(1080),
            vcodec: Some("avc1".to_string()),
            acodec: Some("mp4a".to_string()),
            has_drm: true,
            ..Format::default()
        }];
        let sel = parse_selector("best").unwrap();
        assert!(select_formats(&sel, &fmts).is_err());
    }

    #[test]
    fn empty_format_list_returns_err() {
        let fmts: Vec<Format> = vec![];
        let sel = parse_selector("best").unwrap();
        assert!(select_formats(&sel, &fmts).is_err());
    }

    #[test]
    fn worstvideo_picks_lowest_res() {
        let fmts = sample_formats();
        let sel = parse_selector("worstvideo").unwrap();
        if let SelectedFormats::Single(f) = select_formats(&sel, &fmts).unwrap() {
            // 360p avc1 or 360p combined — both at height=360
            assert!(f.height.unwrap_or(9999) <= 360);
        }
    }
}
