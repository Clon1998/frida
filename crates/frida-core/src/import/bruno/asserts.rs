//! Bruno's declarative `assert` rows (`res.status: eq 200`) turned into Frida tests
//! (`test("res.status: eq 200", () => expect(res.status).to.eql(200))`). Operators Frida can't
//! express are kept as comments and reported.

use super::model::Kv;

/// Turns enabled assertions into test code. Returns the code and the rows that couldn't be
/// converted.
pub(crate) fn to_tests(asserts: &[Kv]) -> (String, Vec<String>) {
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    for a in asserts {
        let label = format!("{}: {}", a.name, a.value);
        if !a.enabled {
            lines.push(format!("// (disabled) {label}"));
            continue;
        }
        match assertion(&a.name, &a.value) {
            Some(check) => lines.push(format!("test({}, () => {check});", js_string(&label))),
            None => {
                lines.push(format!("// Not converted: {label}"));
                skipped.push(label);
            }
        }
    }
    if lines.is_empty() {
        return (String::new(), skipped);
    }
    (
        format!("// Converted from Bruno assertions.\n{}", lines.join("\n")),
        skipped,
    )
}

/// The `expect(...)` call for one assertion.
fn assertion(lhs: &str, rhs: &str) -> Option<String> {
    let lhs = js_path(lhs.trim());
    if lhs.is_empty() {
        return None;
    }
    let rhs = rhs.trim();
    let (op, arg) = match rhs.split_once(' ') {
        Some((op, arg)) if is_operator(op) => (op, arg.trim()),
        _ if is_operator(rhs) => (rhs, ""),
        // No operator: Bruno means equality.
        _ => ("eq", rhs),
    };
    let e = |chain: &str| Some(format!("expect({lhs}).{chain}"));
    match op {
        "eq" => e(&format!("to.eql({})", value(arg))),
        "neq" => e(&format!("to.not.eql({})", value(arg))),
        "gt" => e(&format!("to.be.above({})", value(arg))),
        "gte" => e(&format!("to.be.at.least({})", value(arg))),
        "lt" => e(&format!("to.be.below({})", value(arg))),
        "lte" => e(&format!("to.be.at.most({})", value(arg))),
        "in" => e(&format!("to.be.oneOf({})", list(arg))),
        "notIn" => e(&format!("to.not.be.oneOf({})", list(arg))),
        "contains" => e(&format!("to.include({})", value(arg))),
        "notContains" => e(&format!("to.not.include({})", value(arg))),
        "length" => e(&format!("to.have.lengthOf({})", value(arg))),
        "matches" => e(&format!(
            "to.match(new RegExp({}))",
            js_string(unquote(arg))
        )),
        "notMatches" => e(&format!(
            "to.not.match(new RegExp({}))",
            js_string(unquote(arg))
        )),
        "startsWith" => Some(format!(
            "expect(String({lhs}).startsWith({})).to.be.true",
            js_string(unquote(arg))
        )),
        "endsWith" => Some(format!(
            "expect(String({lhs}).endsWith({})).to.be.true",
            js_string(unquote(arg))
        )),
        "between" => {
            let (lo, hi) = arg.split_once(',')?;
            e(&format!(
                "to.be.within({}, {})",
                value(lo.trim()),
                value(hi.trim())
            ))
        }
        "isEmpty" => e("to.be.empty"),
        "isNotEmpty" => e("to.not.be.empty"),
        "isNull" => e("to.be.null"),
        "isUndefined" => e("to.be.undefined"),
        "isDefined" => e("to.not.be.undefined"),
        "isTruthy" => e("to.be.ok"),
        "isFalsy" => e("to.not.be.ok"),
        "isNumber" => e("to.be.a(\"number\")"),
        "isString" => e("to.be.a(\"string\")"),
        "isBoolean" => e("to.be.a(\"boolean\")"),
        "isArray" => e("to.be.a(\"array\")"),
        "isJson" => e("to.be.a(\"object\")"),
        _ => None,
    }
}

/// `res.body.headers.x-tenant` → `res.body.headers["x-tenant"]`: plain dotted paths with
/// segments that aren't JS identifiers get bracket notation. Anything else is left as written.
fn js_path(lhs: &str) -> String {
    let simple = lhs
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.' | '-'));
    if !simple {
        return lhs.to_string();
    }
    let mut out = String::new();
    for (i, segment) in lhs.split('.').enumerate() {
        let identifier = segment
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
            && !segment.contains('-');
        if i == 0 || identifier {
            if i > 0 {
                out.push('.');
            }
            out.push_str(segment);
        } else if segment.chars().all(|c| c.is_ascii_digit()) {
            out.push_str(&format!("[{segment}]"));
        } else {
            out.push_str(&format!("[{}]", js_string(segment)));
        }
    }
    out
}

fn is_operator(word: &str) -> bool {
    matches!(
        word,
        "eq" | "neq"
            | "gt"
            | "gte"
            | "lt"
            | "lte"
            | "in"
            | "notIn"
            | "contains"
            | "notContains"
            | "length"
            | "matches"
            | "notMatches"
            | "startsWith"
            | "endsWith"
            | "between"
            | "isEmpty"
            | "isNotEmpty"
            | "isNull"
            | "isUndefined"
            | "isDefined"
            | "isTruthy"
            | "isFalsy"
            | "isNumber"
            | "isString"
            | "isBoolean"
            | "isArray"
            | "isJson"
    )
}

/// A Bruno assertion value as a JS literal: numbers, booleans and `null` stay as they are,
/// quoted text is a string, `{{var}}` is interpolated, anything else is a string.
fn value(raw: &str) -> String {
    let raw = raw.trim();
    if raw == "true" || raw == "false" || raw == "null" || raw == "undefined" {
        return raw.to_string();
    }
    if is_number(raw) {
        return raw.to_string();
    }
    if (raw.starts_with('\'') && raw.ends_with('\'') || raw.starts_with('"') && raw.ends_with('"'))
        && raw.len() >= 2
    {
        return js_string(&raw[1..raw.len() - 1]);
    }
    if raw.starts_with("{{") && raw.ends_with("}}") {
        return format!("fri.interpolate({})", js_string(raw));
    }
    js_string(raw)
}

/// A plain JS number literal. A leading zero (`0123`) is text, like a zip code.
fn is_number(raw: &str) -> bool {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    let leading_zero = digits.len() > 1 && digits.starts_with('0') && !digits.starts_with("0.");
    !leading_zero
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && digits.chars().any(|c| c.is_ascii_digit())
        && raw.parse::<f64>().is_ok()
}

fn list(raw: &str) -> String {
    let items: Vec<String> = raw
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| value(item.trim()))
        .collect();
    format!("[{}]", items.join(", "))
}

fn unquote(raw: &str) -> &str {
    let raw = raw.trim();
    if raw.len() >= 2
        && (raw.starts_with('\'') && raw.ends_with('\'')
            || raw.starts_with('"') && raw.ends_with('"'))
    {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

/// A JS string literal (JSON escaping is valid JS).
fn js_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(name: &str, value: &str) -> Kv {
        Kv {
            name: name.into(),
            value: value.into(),
            enabled: true,
            description: String::new(),
        }
    }

    #[test]
    fn converts_common_assertions() {
        let (code, skipped) = to_tests(&[
            kv("res.status", "200"),
            kv("res.status", "eq 200"),
            kv("res.body.string", "eq 'foo'"),
            kv("res.body.zip", "eq 0123"),
            kv("res.body.items", "length 2"),
            kv("res.body.id", "isNumber"),
            kv("res.status", "in 200, 201"),
            kv("res.body.name", "magic"),
            kv("res.body.x", "frobnicate 1"),
        ]);
        let lines: Vec<&str> = code.lines().collect();
        assert_eq!(
            lines[1],
            r#"test("res.status: 200", () => expect(res.status).to.eql(200));"#
        );
        assert_eq!(
            lines[3],
            r#"test("res.body.string: eq 'foo'", () => expect(res.body.string).to.eql("foo"));"#
        );
        assert!(lines[4].ends_with(r#"to.eql("0123"));"#), "{}", lines[4]);
        assert!(lines[5].ends_with("to.have.lengthOf(2));"));
        assert!(lines[6].ends_with(r#"to.be.a("number"));"#));
        assert!(lines[7].ends_with("to.be.oneOf([200, 201]));"));
        assert!(lines[8].ends_with(r#"to.eql("magic"));"#));
        // An unknown word is treated as the expected value, like Bruno does.
        assert!(lines[9].ends_with(r#"to.eql("frobnicate 1"));"#));
        assert!(skipped.is_empty());
    }

    #[test]
    fn non_identifier_path_segments_use_brackets() {
        assert_eq!(
            js_path("res.body.headers.x-tenant"),
            "res.body.headers[\"x-tenant\"]"
        );
        assert_eq!(js_path("res.body.items.0.id"), "res.body.items[0].id");
        assert_eq!(js_path("res('a.b')"), "res('a.b')");
    }
}
