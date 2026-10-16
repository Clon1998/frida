//! The response as the UI sees it, plus a small cache of full bodies for "Save to file".

use std::collections::VecDeque;

use base64::Engine;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::send::HttpResponse;
use crate::model::KeyValue;

/// Bodies larger than this are shown truncated (PLAN.md §14). The full body can still be saved.
pub const MAX_DISPLAY_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResponseView {
    /// Identifies the full body in the [`ResponseCache`].
    pub id: String,
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KeyValue>,
    pub content_type: Option<String>,
    pub size_bytes: u32,
    pub time_ms: u32,
    pub final_url: String,
    /// The body as text when it is valid UTF-8 (or truncated, see `truncated`).
    pub body_text: Option<String>,
    /// The body as base64 when it isn't text (e.g. images), and only when small enough.
    pub body_base64: Option<String>,
    /// `true` when the body was larger than [`MAX_DISPLAY_BYTES`] and only the start is shown.
    pub truncated: bool,
}

impl ResponseView {
    pub fn new(id: String, response: &HttpResponse) -> Self {
        let body = &response.body;
        let content_type = response
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.clone());

        let truncated = body.len() > MAX_DISPLAY_BYTES;
        let (body_text, body_base64) = if truncated {
            let shown = &body[..MAX_DISPLAY_BYTES];
            (Some(String::from_utf8_lossy(shown).to_string()), None)
        } else {
            match std::str::from_utf8(body) {
                Ok(text) => (Some(text.to_string()), None),
                Err(_) => (
                    None,
                    Some(base64::engine::general_purpose::STANDARD.encode(body)),
                ),
            }
        };

        Self {
            id,
            status: response.status,
            status_text: response.status_text.clone(),
            headers: response
                .headers
                .iter()
                .map(|(name, value)| KeyValue::new(name.clone(), value.clone()))
                .collect(),
            content_type,
            size_bytes: u32::try_from(body.len()).unwrap_or(u32::MAX),
            time_ms: u32::try_from(response.elapsed.as_millis()).unwrap_or(u32::MAX),
            final_url: response.final_url.clone(),
            body_text,
            body_base64,
            truncated,
        }
    }
}

/// Keeps the full bodies of the most recent responses in memory, so "Save to file" writes the
/// complete body even when the UI only shows a truncated view.
#[derive(Debug, Default)]
pub struct ResponseCache {
    entries: VecDeque<(String, Vec<u8>)>,
}

impl ResponseCache {
    /// How many bodies are kept. Older ones are dropped first.
    pub const CAPACITY: usize = 20;

    pub fn insert(&mut self, id: String, body: Vec<u8>) {
        if self.entries.len() == Self::CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((id, body));
    }

    pub fn get(&self, id: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|(entry_id, _)| entry_id == id)
            .map(|(_, body)| body.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn response(body: Vec<u8>, content_type: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: vec![("content-type".to_string(), content_type.to_string())],
            body,
            elapsed: Duration::from_millis(12),
            final_url: "http://x/".to_string(),
            http_version: "HTTP/1.1".into(),
        }
    }

    #[test]
    fn text_bodies_are_text_and_binary_bodies_are_base64() {
        let text = ResponseView::new(
            "1".into(),
            &response(b"{\"a\":1}".to_vec(), "application/json"),
        );
        assert_eq!(text.body_text.as_deref(), Some("{\"a\":1}"));
        assert_eq!(text.body_base64, None);
        assert_eq!(text.content_type.as_deref(), Some("application/json"));
        assert_eq!(text.time_ms, 12);

        let png = ResponseView::new(
            "2".into(),
            &response(vec![0x89, b'P', b'N', b'G', 0xff], "image/png"),
        );
        assert_eq!(png.body_text, None);
        assert_eq!(png.body_base64.as_deref(), Some("iVBOR/8="));
    }

    #[test]
    fn large_bodies_are_truncated() {
        let big = vec![b'a'; MAX_DISPLAY_BYTES + 10];
        let view = ResponseView::new("1".into(), &response(big, "text/plain"));
        assert!(view.truncated);
        assert_eq!(view.body_text.unwrap().len(), MAX_DISPLAY_BYTES);
        assert_eq!(view.size_bytes as usize, MAX_DISPLAY_BYTES + 10);
    }

    #[test]
    fn cache_keeps_only_the_newest_bodies() {
        let mut cache = ResponseCache::default();
        for i in 0..=ResponseCache::CAPACITY {
            cache.insert(i.to_string(), vec![i as u8]);
        }
        assert_eq!(cache.get("0"), None);
        assert_eq!(cache.get("1"), Some(&[1u8][..]));
    }
}
