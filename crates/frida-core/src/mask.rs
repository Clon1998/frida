//! Masking secret values (PLAN.md §6) before data leaves the request pipeline: history, the
//! console and curl export never contain them.

pub const MASK: &str = "***";

/// Replaces known secret values with `***` in text and in UTF-8 bodies.
pub struct Masker {
    /// Longest first, so a secret containing another one is masked as a whole.
    secrets: Vec<String>,
}

impl Masker {
    /// Masks each value in `sensitive` and its URL-encoded forms: a secret in the query string
    /// travels encoded (`a b/ä` → `a%20b/%C3%A4`), and servers echo it back that way, e.g. in
    /// the final URL or a response body.
    pub fn new(sensitive: &[String]) -> Self {
        let mut secrets: Vec<String> = sensitive
            .iter()
            .filter(|s| !s.is_empty())
            .flat_map(|s| encoded_forms(s))
            .collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        secrets.dedup();
        Self { secrets }
    }

    pub fn text(&self, text: &str) -> String {
        let mut out = text.to_string();
        for secret in &self.secrets {
            out = out.replace(secret.as_str(), MASK);
        }
        out
    }

    /// Text bodies are masked; binary bodies are kept as they are (a secret could only be found
    /// in them by accident, and replacing bytes would corrupt them).
    pub fn bytes(&self, bytes: &[u8]) -> Vec<u8> {
        match std::str::from_utf8(bytes) {
            Ok(text) => self.text(text).into_bytes(),
            Err(_) => bytes.to_vec(),
        }
    }
}

/// `secret` plus the ways it may appear URL-encoded (only the ones that differ from it).
fn encoded_forms(secret: &str) -> Vec<String> {
    let unreserved = |b: u8| b.is_ascii_alphanumeric() || b"-._~".contains(&b);
    // What HTTP clients (WHATWG URL, the `url` crate) leave as is in a query or a path.
    let query_safe = |b: u8| b > 0x20 && b < 0x7f && !b"\"#<>'".contains(&b);
    let path_safe = |b: u8| query_safe(b) && !b"?`{}".contains(&b);
    let mut forms = vec![
        secret.to_string(),
        percent_encode(secret, unreserved, false),
        percent_encode(secret, query_safe, false),
        percent_encode(secret, path_safe, false),
        // `application/x-www-form-urlencoded`: space as `+`.
        percent_encode(
            secret,
            |b| b.is_ascii_alphanumeric() || b"*-._".contains(&b),
            true,
        ),
    ];
    forms.dedup();
    forms.sort();
    forms.dedup();
    forms
}

/// `%XX` (upper-case hex, like most clients) for every byte `keep` doesn't accept.
fn percent_encode(text: &str, keep: impl Fn(u8) -> bool, space_as_plus: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if space_as_plus && b == b' ' {
            out.push('+');
        } else if keep(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
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

    #[test]
    fn masks_url_encoded_forms_of_a_secret() {
        let m = Masker::new(&["q s3cret/äö&x".into()]);
        for encoded in [
            "q%20s3cret/%C3%A4%C3%B6&x",     // query (what reqwest sends)
            "q%20s3cret%2F%C3%A4%C3%B6%26x", // strict percent-encoding
            "q+s3cret%2F%C3%A4%C3%B6%26x",   // form-encoded
            "q s3cret/äö&x",                 // raw
        ] {
            assert_eq!(
                m.text(&format!("url?key={encoded}")),
                "url?key=***",
                "{encoded}"
            );
        }
        // Plain secrets have no other forms; nothing else gets masked.
        let plain = Masker::new(&["abc123".into()]);
        assert_eq!(plain.secrets, vec!["abc123".to_string()]);
    }
}
