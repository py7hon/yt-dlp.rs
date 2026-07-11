pub mod youtube;

use crate::types::VideoInfo;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;

pub use youtube::YoutubeExtractor;

#[async_trait(?Send)]
pub trait Extractor: Send + Sync {
    fn name(&self) -> &str;
    fn suitable(&self, url: &str) -> bool;
    async fn extract(&self, url: &str, client: &Client) -> Result<VideoInfo>;
}

pub fn get_extractor(url: &str) -> Option<Box<dyn Extractor>> {
    let extractors: Vec<Box<dyn Extractor>> = vec![Box::new(YoutubeExtractor::new())];
    extractors.into_iter().find(|e| e.suitable(url))
}
