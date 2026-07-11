use wasm_bindgen::prelude::*;
use std::sync::Arc;
use crate::extractor::YoutubeExtractor;
use crate::downloader::build_client;
use crate::types::DownloadOptions;

#[wasm_bindgen]
pub async fn wasm_extract_info(url: String, cors_proxy: Option<String>) -> Result<JsValue, JsValue> {
    let opts = DownloadOptions {
        ..DownloadOptions::default()
    };
    let client = build_client(&opts).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let extractor = YoutubeExtractor::new();
    let info = extractor
        .extract_with_proxy(&url, &client, cors_proxy.as_deref())
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        
    serde_wasm_bindgen::to_value(&info).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub async fn wasm_download_format(
    url: String,
    protocol: String,
    headers_json: String,
    cors_proxy: Option<String>,
    progress_cb: js_sys::Function,
) -> Result<js_sys::Uint8Array, JsValue> {
    let opts = DownloadOptions {
        ..DownloadOptions::default()
    };
    let client = build_client(&opts).map_err(|e| JsValue::from_str(&e.to_string()))?;
    
    let headers: std::collections::HashMap<String, String> = if headers_json.is_empty() {
        std::collections::HashMap::new()
    } else {
        serde_json::from_str(&headers_json).map_err(|e| JsValue::from_str(&e.to_string()))?
    };
    
    let progress_cb = Arc::new(progress_cb);
    let cb = {
        let progress_cb = Arc::clone(&progress_cb);
        move |pos: u64, total: u64| {
            let this = JsValue::NULL;
            let arg1 = JsValue::from_f64(pos as f64);
            let arg2 = JsValue::from_f64(total as f64);
            let _ = progress_cb.call2(&this, &arg1, &arg2);
        }
    };
    
    let data = match protocol.as_str() {
        "hls" | "m3u8" | "m3u8_native" => {
            let downloader = crate::downloader::HlsDownloader::new(client, 4, 10, false, false);
            downloader.download_to_vec_with_progress(&url, cors_proxy.as_deref(), cb)
                .await
                .map_err(|e| JsValue::from_str(&e.to_string()))?
        }
        _ => {
            let downloader = crate::downloader::HttpDownloader::new(client, 10, None, false, false);
            downloader.download_to_vec_with_progress(&url, &headers, cors_proxy.as_deref(), cb)
                .await
                .map_err(|e| JsValue::from_str(&e.to_string()))?
        }
    };
    
    Ok(js_sys::Uint8Array::from(&data[..]))
}
