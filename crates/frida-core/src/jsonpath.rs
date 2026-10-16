//! The JSONPath filter of the response viewer (PLAN.md §14), e.g. `$.items[*].id`.
//!
//! Uses `serde_json_path`, an implementation of RFC 9535. Filter expressions such as
//! `$.items[?@.price < 10]` are parsed, never run as code, so a response from an untrusted
//! server can't do anything through them.

use serde_json::Value;
use serde_json_path::JsonPath;

use crate::error::{CoreError, Result};

/// Applies `path` to the JSON `body` and returns the result, pretty-printed.
///
/// Exactly one match gives that value (`$.items[0]` → the object), otherwise an array of all
/// matches (`$.items[*].id` → `[1, 2]`, no match → `[]`).
pub fn filter_json(body: &[u8], path: &str) -> Result<String> {
    let path = JsonPath::parse(path.trim())
        .map_err(|e| CoreError::InvalidInput(format!("invalid JSONPath: {e}")))?;
    let json: Value = serde_json::from_slice(body)
        .map_err(|e| CoreError::InvalidInput(format!("the response body is not JSON: {e}")))?;
    // `query` borrows from `json`: the matches are references into it, nothing is copied
    // until we build the result.
    let matches = path.query(&json).all();
    let result = match matches.as_slice() {
        [single] => (*single).clone(),
        _ => Value::Array(matches.into_iter().cloned().collect()),
    };
    Ok(serde_json::to_string_pretty(&result).expect("a serde_json Value always serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &[u8] =
        br#"{"items":[{"id":1,"price":5},{"id":2,"price":20}],"total":2,"meta":{"id":"m"}}"#;

    fn filter(path: &str) -> String {
        filter_json(BODY, path).unwrap()
    }

    #[test]
    fn one_match_is_the_value_itself() {
        assert_eq!(filter("$.total"), "2");
        assert_eq!(filter("$.items[0].id"), "1");
        assert_eq!(filter("$.meta"), "{\n  \"id\": \"m\"\n}");
    }

    #[test]
    fn several_or_no_matches_are_an_array() {
        assert_eq!(filter("$.items[*].id"), "[\n  1,\n  2\n]");
        assert_eq!(filter("$..id"), "[\n  1,\n  2,\n  \"m\"\n]");
        assert_eq!(filter("$.nothing"), "[]");
    }

    #[test]
    fn filter_expressions_work() {
        assert_eq!(filter("$.items[?@.price < 10].id"), "1");
    }

    #[test]
    fn errors_say_what_is_wrong() {
        let bad_path = filter_json(BODY, "$.items[").unwrap_err().to_string();
        assert!(bad_path.contains("invalid JSONPath"), "{bad_path}");
        let not_json = filter_json(b"<html>", "$").unwrap_err().to_string();
        assert!(not_json.contains("not JSON"), "{not_json}");
    }
}
