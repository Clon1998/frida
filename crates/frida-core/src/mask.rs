//! Masking secret values (PLAN.md §6) before data leaves the request pipeline: history, the
//! console and curl export never contain them.

pub(crate) const MASK: &str = "***";

/// Replaces known secret values with `***` in text and in UTF-8 bodies.
pub(crate) struct Masker {
    /// Longest first, so a secret containing another one is masked as a whole.
    secrets: Vec<String>,
}

impl Masker {
    pub(crate) fn new(sensitive: &[String]) -> Self {
        let mut secrets: Vec<String> = sensitive
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        secrets.dedup();
        Self { secrets }
    }

    pub(crate) fn text(&self, text: &str) -> String {
        let mut out = text.to_string();
        for secret in &self.secrets {
            out = out.replace(secret.as_str(), MASK);
        }
        out
    }

    /// Text bodies are masked; binary bodies are kept as they are (a secret could only be found
    /// in them by accident, and replacing bytes would corrupt them).
    pub(crate) fn bytes(&self, bytes: &[u8]) -> Vec<u8> {
        match std::str::from_utf8(bytes) {
            Ok(text) => self.text(text).into_bytes(),
            Err(_) => bytes.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_longest_secret_first() {
        let m = Masker::new(&["abc".into(), "abcdef".into(), String::new()]);
        assert_eq!(m.text("x abcdef y abc"), "x *** y ***");
        assert_eq!(m.bytes(&[0xff, b'a']), vec![0xff, b'a']);
    }
}
