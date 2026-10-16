//! A parser for Bruno's `.bru` block format (the grammar is in Bruno's
//! `packages/bruno-lang/v2/src/bruToJson.js`). A file is a list of blocks:
//!
//! ```text
//! meta {                      dictionary block: `key: value` pairs, `~key` = disabled
//!   name: Login
//! }
//! body:json {                 text block: everything inside, outdented by 2 spaces
//!   {"user": "ann"}
//! }
//! vars:secret [               list block
//!   token
//! ]
//! ```
//!
//! Dictionary values can span lines between `'''` markers, and pairs can carry annotations
//! (`@description('...')`, `@number`) on the lines above them, which are skipped.

/// Blocks whose content is free text rather than `key: value` pairs.
const TEXT_BLOCKS: &[&str] = &[
    "body",
    "body:json",
    "body:text",
    "body:xml",
    "body:sparql",
    "body:graphql",
    "body:graphql:vars",
    "script:pre-request",
    "script:post-response",
    "script:grpc:before-call-start",
    "script:grpc:before-message-send",
    "script:grpc:after-message-receive",
    "script:grpc:after-call-end",
    "tests",
    "docs",
    "example",
];

/// One `key: value` line of a dictionary block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pair {
    pub name: String,
    pub value: String,
    /// `false` when the key starts with `~`.
    pub enabled: bool,
    /// From a `@description('...')` annotation on the line(s) above, if any.
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Content {
    Dict(Vec<Pair>),
    Text(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Block {
    pub name: String,
    pub content: Content,
}

/// A parsed `.bru` file.
#[derive(Debug, Clone, Default)]
pub(crate) struct BruFile {
    pub blocks: Vec<Block>,
}

impl BruFile {
    fn block(&self, name: &str) -> Option<&Content> {
        self.blocks
            .iter()
            .find(|b| b.name == name)
            .map(|b| &b.content)
    }

    /// The pairs of a dictionary block (empty when missing).
    pub fn dict(&self, name: &str) -> Vec<Pair> {
        match self.block(name) {
            Some(Content::Dict(pairs)) => pairs.clone(),
            _ => Vec::new(),
        }
    }

    /// One value of a dictionary block (first enabled or not).
    pub fn value(&self, block: &str, key: &str) -> Option<String> {
        self.dict(block)
            .into_iter()
            .find(|p| p.name == key)
            .map(|p| p.value)
    }

    pub fn text(&self, name: &str) -> Option<String> {
        match self.block(name) {
            Some(Content::Text(text)) => Some(text.clone()),
            _ => None,
        }
    }

    pub fn list(&self, name: &str) -> Vec<String> {
        match self.block(name) {
            Some(Content::List(items)) => items.clone(),
            _ => Vec::new(),
        }
    }

    /// The HTTP method block (`get`, `post`, ..., or `http` with a `method` key).
    pub fn http(&self) -> Option<(String, Vec<Pair>)> {
        const METHODS: [&str; 9] = [
            "get", "post", "put", "delete", "patch", "options", "head", "connect", "trace",
        ];
        for block in &self.blocks {
            if let Content::Dict(pairs) = &block.content {
                if METHODS.contains(&block.name.as_str()) {
                    return Some((block.name.to_uppercase(), pairs.clone()));
                }
                if block.name == "http" {
                    let method = pairs
                        .iter()
                        .find(|p| p.name == "method")
                        .map(|p| p.value.to_uppercase())
                        .unwrap_or_else(|| "GET".into());
                    return Some((method, pairs.clone()));
                }
            }
        }
        None
    }
}

/// Parses a `.bru` file. Unknown or malformed parts are skipped rather than failing: an import
/// should get as much as possible out of a collection.
pub(crate) fn parse(text: &str) -> BruFile {
    let text = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        i += 1;
        let Some((name, opener)) = block_header(line) else {
            continue;
        };
        // Collect the block's lines up to the closing `}` / `]` at the start of a line.
        let closer = if opener == '[' { "]" } else { "}" };
        let start = i;
        while i < lines.len() && lines[i].trim_end() != closer {
            i += 1;
        }
        let body = &lines[start..i.min(lines.len())];
        i += 1; // skip the closer
        let content = if opener == '[' {
            Content::List(
                body.iter()
                    .map(|l| l.trim().trim_end_matches(',').to_string())
                    .filter(|l| !l.is_empty())
                    .collect(),
            )
        } else if TEXT_BLOCKS.contains(&name.as_str()) {
            Content::Text(outdent(body, 2))
        } else {
            Content::Dict(parse_pairs(body))
        };
        blocks.push(Block { name, content });
    }
    BruFile { blocks }
}

/// `name {` or `name [` at the start of a line.
fn block_header(line: &str) -> Option<(String, char)> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let opener = line.chars().last().filter(|c| *c == '{' || *c == '[')?;
    let name = line[..line.len() - 1].trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    Some((name.to_string(), opener))
}

/// Removes `spaces` leading spaces from every line (Bruno's `outdentString`).
fn outdent(lines: &[&str], spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    lines
        .iter()
        .map(|l| l.strip_prefix(prefix.as_str()).unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_pairs(lines: &[&str]) -> Vec<Pair> {
    let mut pairs = Vec::new();
    // A `@description` waiting for the pair below it.
    let mut description = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Annotations (`@number`, `@description('...')`) belong to the next pair. Descriptions
        // are kept; others are skipped, including multi-line `('''...''')` arguments.
        if is_annotation(trimmed) {
            let mut text = vec![trimmed.to_string()];
            if trimmed.contains("'''") && trimmed.matches("'''").count() == 1 {
                while i < lines.len() && !lines[i].contains("'''") {
                    text.push(lines[i].trim().to_string());
                    i += 1;
                }
                if i < lines.len() {
                    text.push(lines[i].trim().to_string());
                }
                i += 1;
            }
            if let Some(found) = annotation_description(&text) {
                description = found;
            }
            continue;
        }
        let Some((key, rest)) = split_key(trimmed) else {
            continue;
        };
        let (name, enabled) = match key.strip_prefix('~') {
            Some(name) => (name.to_string(), false),
            None => (key, true),
        };
        let mut value = rest.trim().to_string();
        // A `'''` value continues until the closing `'''`; content lines are indented by 4.
        // Text after the closing marker (e.g. `@contentType(...)`) is kept, after a space.
        if let Some(after) = value.strip_prefix("'''").map(str::to_string) {
            match after.find("'''") {
                Some(end) => value = join_tail(after[..end].to_string(), after[end + 3..].trim()),
                None => {
                    let mut collected: Vec<String> = Vec::new();
                    if !after.trim().is_empty() {
                        collected.push(after.trim().to_string());
                    }
                    let mut tail = String::new();
                    while i < lines.len() {
                        let l = lines[i];
                        i += 1;
                        if let Some(pos) = l.find("'''") {
                            let before = &l[..pos];
                            if !before.trim().is_empty() {
                                collected.push(strip_indent(before, 4));
                            }
                            tail = l[pos + 3..].trim().to_string();
                            break;
                        }
                        collected.push(strip_indent(l, 4));
                    }
                    value = join_tail(collected.join("\n"), &tail);
                }
            }
        }
        pairs.push(Pair {
            name,
            value,
            enabled,
            description: std::mem::take(&mut description),
        });
    }
    pairs
}

/// The text of a `@description(...)` annotation. `lines` are the annotation's lines (trimmed),
/// the first starting with `@`: `@description('text')`, `@description('''text''')`, or the
/// multi-line form `@description('''`, the text lines, `''')`.
fn annotation_description(lines: &[String]) -> Option<String> {
    let first = lines.first()?;
    let arg = first.strip_prefix("@description(")?.trim();
    if let Some(rest) = arg.strip_prefix("'''") {
        if let Some(end) = rest.find("'''") {
            return Some(rest[..end].trim().to_string());
        }
        let body: Vec<&str> = lines[1..]
            .iter()
            .map(String::as_str)
            .take_while(|l| !l.starts_with("'''"))
            .collect();
        return Some(body.join("\n").trim().to_string());
    }
    // `@description('text')` or `@description("text")`.
    let quote = arg.chars().next()?;
    let inner = arg.strip_prefix(quote)?.strip_suffix(')')?.trim_end();
    Some(inner.strip_suffix(quote)?.to_string())
}

/// `@number` or `@description(...)`, but not a local variable `@name: value`.
fn is_annotation(line: &str) -> bool {
    let Some(rest) = line.strip_prefix('@') else {
        return false;
    };
    let name_end = rest
        .find(|c: char| c == '(' || c == ':' || c.is_whitespace())
        .unwrap_or(rest.len());
    name_end > 0 && (name_end == rest.len() || rest[name_end..].starts_with('('))
}

fn join_tail(value: String, tail: &str) -> String {
    if tail.is_empty() {
        value
    } else {
        format!("{value} {tail}")
    }
}

/// Removes up to `n` leading spaces.
fn strip_indent(line: &str, n: usize) -> String {
    let spaces = line.chars().take(n).take_while(|c| *c == ' ').count();
    line[spaces..].to_string()
}

/// Splits `key: value`. Keys can be quoted (`"a:b": 1`), possibly with `~` before the quote.
fn split_key(line: &str) -> Option<(String, &str)> {
    let (disabled, rest) = match line.strip_prefix('~') {
        Some(rest) => ("~", rest),
        None => ("", line),
    };
    if let Some(quoted) = rest.strip_prefix('"') {
        let mut escaped = false;
        for (pos, c) in quoted.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => {
                    let key = quoted[..pos].replace("\\\"", "\"");
                    let after = quoted[pos + 1..].trim_start();
                    let value = after.strip_prefix(':')?;
                    return Some((format!("{disabled}{key}"), value));
                }
                _ => escaped = false,
            }
        }
        return None;
    }
    let colon = line.find(':')?;
    Some((line[..colon].trim().to_string(), &line[colon + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"meta {
  name: Login
  type: http
  seq: 2
}

post {
  url: {{host}}/login?x=1
  body: json
  auth: inherit
}

headers {
  @description('''
    multi
    line
  ''')
  X-Api: 1
  ~X-Off: 2
  "a:b": 3
}

body:json {
  {
    "user": "ann"
  }
}

body:multipart-form {
  note: '''
    line one
    line two
  ''' @contentType(text/plain)
  file: @file(a.png|b.png)
}

vars:secret [
  token,
  password
]

script:pre-request {
  if (x) {
    bru.setVar("a", 1);
  }
}
"#;

    #[test]
    fn parses_every_block_kind() {
        let file = parse(SAMPLE);
        assert_eq!(file.value("meta", "name").as_deref(), Some("Login"));
        let (method, http) = file.http().unwrap();
        assert_eq!(method, "POST");
        assert_eq!(http[0].value, "{{host}}/login?x=1");

        let headers = file.dict("headers");
        assert_eq!(headers.len(), 3);
        assert!(!headers[1].enabled);
        assert_eq!(headers[1].name, "X-Off");
        assert_eq!(headers[2].name, "a:b");

        assert_eq!(
            file.text("body:json").as_deref(),
            Some("{\n  \"user\": \"ann\"\n}")
        );
        let multipart = file.dict("body:multipart-form");
        assert_eq!(
            multipart[0].value,
            "line one\nline two @contentType(text/plain)"
        );
        assert_eq!(multipart[1].value, "@file(a.png|b.png)");

        assert_eq!(file.list("vars:secret"), vec!["token", "password"]);
        assert_eq!(
            file.text("script:pre-request").as_deref(),
            Some("if (x) {\n  bru.setVar(\"a\", 1);\n}")
        );
    }

    #[test]
    fn descriptions_belong_to_the_pair_below_them() {
        let headers = parse(SAMPLE).dict("headers");
        // The multi-line `@description('''...''')` in SAMPLE is above `X-Api`.
        assert_eq!(headers[0].name, "X-Api");
        assert_eq!(headers[0].description, "multi\nline");
        assert_eq!(headers[1].description, "");

        let file = parse(
            "params:query {\n  @description('Page size')\n  limit: 20\n  @number\n  page: 1\n  @description(\"say \\\"hi\\\"\")\n  q: x\n}\n",
        );
        let params = file.dict("params:query");
        let found: Vec<(&str, &str)> = params
            .iter()
            .map(|p| (p.name.as_str(), p.description.as_str()))
            .collect();
        assert_eq!(found[0], ("limit", "Page size"));
        assert_eq!(found[1], ("page", ""));
        assert_eq!(found[2].0, "q");
    }

    #[test]
    fn a_closing_brace_inside_text_must_be_indented() {
        let file = parse("tests {\n  test(\"x\", () => {\n  });\n}\n");
        assert_eq!(
            file.text("tests").as_deref(),
            Some("test(\"x\", () => {\n});")
        );
    }
}
