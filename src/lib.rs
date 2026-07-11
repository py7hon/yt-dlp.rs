pub mod downloader;
pub mod error;
pub mod extractor;
pub mod output;
pub mod postprocessor;
pub mod selector;
pub mod types;
pub mod utils;

#[cfg(target_arch = "wasm32")]
pub mod wasm;
