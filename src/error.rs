use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum YtDplError {
    #[error("Extraction failed: {0}")]
    Extraction(String),

    #[error("No suitable format found for selector: {0}")]
    NoFormat(String),

    #[error("Format is DRM-protected and cannot be downloaded")]
    DrmProtected,

    #[error("No extractor found for URL: {0}")]
    NoExtractor(String),

    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    #[error("Download failed: {0}")]
    Download(String),

    #[error("FFmpeg error: {0}")]
    Ffmpeg(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("{0}")]
    Other(String),
}

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, YtDplError>;
