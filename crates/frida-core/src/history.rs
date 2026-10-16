//! Request history (PLAN.md §13): a local SQLite database in the app data dir.
//!
//! Every send is stored with the resolved request and the response, bodies capped at 1 MB.
//! Secret values (resolved secrets and auth values) are replaced by `***` *before* anything is
//! written, so history never contains them (PLAN.md §6).
//!
//! All methods do blocking I/O; async callers use `spawn_blocking`.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{CoreError, Result};
use crate::http::{HttpResponse, PreparedBody, PreparedRequest, ResponseView};
use crate::mask::Masker;
use crate::model::KeyValue;
use crate::vars::iso_timestamp;

/// Request and response bodies are stored up to this size (PLAN.md §13).
pub const MAX_STORED_BODY: usize = 1024 * 1024;
/// Default number of entries kept (PLAN.md §13; configurable).
pub const DEFAULT_RETENTION: usize = 500;

/// One send, ready to be stored. Built with [`NewEntry::from_send`].
pub struct NewEntry {
    pub collection_id: String,
    pub request_path: String,
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<Vec<u8>>,
    pub response: Option<HttpResponse>,
    pub error: Option<String>,
}

impl NewEntry {
    /// From the request as sent and its outcome.
    pub fn from_send(
        collection_id: &str,
        request_path: &str,
        request_name: &str,
        sent: &PreparedRequest,
        outcome: std::result::Result<&HttpResponse, String>,
    ) -> Self {
        let request_body = match &sent.body {
            PreparedBody::None => None,
            PreparedBody::Bytes(bytes) => Some(bytes.clone()),
            PreparedBody::Form(fields) => Some(
                fields
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("&")
                    .into_bytes(),
            ),
            PreparedBody::Multipart(_) => Some(b"(multipart form)".to_vec()),
        };
        let (response, error) = match outcome {
            Ok(response) => (Some(response.clone()), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            collection_id: collection_id.to_string(),
            request_path: request_path.to_string(),
            request_name: request_name.to_string(),
            method: sent.method.clone(),
            url: sent.url.clone(),
            request_headers: sent.headers.clone(),
            request_body,
            response,
            error,
        }
    }
}

impl HistoryEntry {
    /// A new request with this entry's method, URL, headers and body ("Restore as new
    /// request"). Masked secrets stay `***`; the user fills them in again.
    pub fn to_request(&self) -> crate::model::Request {
        use crate::model::{BodyMode, Request};
        let mut request = Request::new(format!("{} (from history)", self.summary.request_name));
        request.method = self.summary.method.clone();
        request.url = self.summary.url.clone();
        request.params.query = crate::import::query_params(&request.url);
        let content_type = self
            .request_headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.to_lowercase())
            .unwrap_or_default();
        request.headers = self.request_headers.clone();
        if let Some(body) = self.request_body.clone().filter(|b| !b.is_empty()) {
            if content_type.contains("json") {
                request.body.mode = BodyMode::Json;
                request.body.json = body;
            } else if content_type.contains("xml") {
                request.body.mode = BodyMode::Xml;
                request.body.xml = body;
            } else {
                request.body.mode = BodyMode::Text;
                request.body.text = body;
            }
        }
        request
    }
}

/// A row in the history list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub id: u32,
    pub timestamp: String,
    pub collection_id: String,
    pub request_path: String,
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub time_ms: Option<u32>,
    pub error: Option<String>,
}

/// A full entry, opened read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub summary: HistorySummary,
    pub request_headers: Vec<KeyValue>,
    /// The body as text, if it was text.
    pub request_body: Option<String>,
    pub request_body_truncated: bool,
    /// The response in the same shape as a live one. Its `id` is `history-<id>`.
    pub response: Option<ResponseView>,
}

pub struct HistoryStore {
    /// SQLite connections aren't thread-safe for concurrent use, so one connection is shared
    /// behind a mutex.
    conn: Mutex<Connection>,
}

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        }
        let conn = Connection::open(path).map_err(db)?;
        Self::init(conn)
    }

    /// An in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(db)?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                collection_id TEXT NOT NULL,
                request_path TEXT NOT NULL,
                request_name TEXT NOT NULL,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                request_headers TEXT NOT NULL,
                request_body BLOB,
                request_body_truncated INTEGER NOT NULL DEFAULT 0,
                status INTEGER,
                status_text TEXT,
                error TEXT,
                time_ms INTEGER,
                final_url TEXT,
                response_headers TEXT,
                response_body BLOB,
                response_size INTEGER
            );
            CREATE INDEX IF NOT EXISTS history_collection ON history (collection_id, id);",
        )
        .map_err(db)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Stores an entry with every `sensitive` value masked, then trims to `retention` entries.
    pub fn add(&self, entry: &NewEntry, sensitive: &[String], retention: usize) -> Result<u32> {
        let m = Masker::new(sensitive);
        let headers: Vec<(String, String)> = entry
            .request_headers
            .iter()
            .map(|(n, v)| (n.clone(), m.text(v)))
            .collect();
        let (request_body, request_truncated) = match &entry.request_body {
            Some(body) => {
                let (bytes, truncated) = cap(&m.bytes(body));
                (Some(bytes), truncated)
            }
            None => (None, false),
        };
        let response = entry.response.as_ref();
        let response_headers = response.map(|r| {
            let masked: Vec<(String, String)> = r
                .headers
                .iter()
                .map(|(n, v)| (n.clone(), m.text(v)))
                .collect();
            serde_json::to_string(&masked).unwrap_or_default()
        });
        let response_body = response.map(|r| cap(&m.bytes(&r.body)).0);

        let conn = self.conn.lock().expect("history lock poisoned");
        conn.execute(
            "INSERT INTO history (timestamp, collection_id, request_path, request_name, method, url,
                request_headers, request_body, request_body_truncated, status, status_text, error,
                time_ms, final_url, response_headers, response_body, response_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                iso_timestamp(now_millis()),
                entry.collection_id,
                entry.request_path,
                entry.request_name,
                entry.method,
                m.text(&entry.url),
                serde_json::to_string(&headers).unwrap_or_default(),
                request_body,
                request_truncated,
                response.map(|r| r.status),
                response.map(|r| r.status_text.clone()),
                entry.error.as_deref().map(|e| m.text(e)),
                response.map(|r| u32::try_from(r.elapsed.as_millis()).unwrap_or(u32::MAX)),
                response.map(|r| m.text(&r.final_url)),
                response_headers,
                response_body,
                response.map(|r| r.body.len() as i64),
            ],
        )
        .map_err(db)?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT ?1)",
            params![retention as i64],
        )
        .map_err(db)?;
        Ok(id as u32)
    }

    /// Newest first. `search` matches name, URL and method; `collection_id` filters.
    pub fn list(
        &self,
        collection_id: Option<&str>,
        search: &str,
        limit: usize,
    ) -> Result<Vec<HistorySummary>> {
        let conn = self.conn.lock().expect("history lock poisoned");
        let pattern = format!("%{}%", search.trim());
        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, collection_id, request_path, request_name, method, url, status, time_ms, error
                 FROM history
                 WHERE (?1 IS NULL OR collection_id = ?1)
                   AND (request_name LIKE ?2 OR url LIKE ?2 OR method LIKE ?2)
                 ORDER BY id DESC LIMIT ?3",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map(
                params![collection_id, pattern, limit as i64],
                summary_from_row,
            )
            .map_err(db)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(db)
    }

    pub fn get(&self, id: u32) -> Result<HistoryEntry> {
        let conn = self.conn.lock().expect("history lock poisoned");
        let row = conn
            .query_row(
                "SELECT id, timestamp, collection_id, request_path, request_name, method, url, status, time_ms, error,
                        request_headers, request_body, request_body_truncated, status_text, final_url,
                        response_headers, response_body
                 FROM history WHERE id = ?1",
                params![id],
                |row| {
                    let summary = summary_from_row(row)?;
                    let request_headers: String = row.get(10)?;
                    let request_body: Option<Vec<u8>> = row.get(11)?;
                    let truncated: bool = row.get(12)?;
                    let status_text: Option<String> = row.get(13)?;
                    let final_url: Option<String> = row.get(14)?;
                    let response_headers: Option<String> = row.get(15)?;
                    let response_body: Option<Vec<u8>> = row.get(16)?;
                    Ok((summary, request_headers, request_body, truncated, status_text, final_url, response_headers, response_body))
                },
            )
            .optional()
            .map_err(db)?
            .ok_or_else(|| CoreError::InvalidInput(format!("history entry {id} not found")))?;
        let (
            summary,
            request_headers,
            request_body,
            truncated,
            status_text,
            final_url,
            response_headers,
            response_body,
        ) = row;

        let pairs = |json: &str| -> Vec<(String, String)> {
            serde_json::from_str(json).unwrap_or_default()
        };
        let response = summary.status.map(|status| {
            let http = HttpResponse {
                status,
                status_text: status_text.unwrap_or_default(),
                headers: pairs(response_headers.as_deref().unwrap_or("[]")),
                body: response_body.unwrap_or_default(),
                elapsed: Duration::from_millis(u64::from(summary.time_ms.unwrap_or(0))),
                final_url: final_url.unwrap_or_default(),
                http_version: "HTTP/1.1".into(),
            };
            ResponseView::new(format!("history-{}", summary.id), &http)
        });
        Ok(HistoryEntry {
            request_headers: pairs(&request_headers)
                .into_iter()
                .map(|(n, v)| KeyValue::new(n, v))
                .collect(),
            request_body: request_body.map(|b| String::from_utf8_lossy(&b).to_string()),
            request_body_truncated: truncated,
            response,
            summary,
        })
    }

    /// The stored response body of an entry, for "Save to file".
    pub fn response_body(&self, id: u32) -> Result<Vec<u8>> {
        let conn = self.conn.lock().expect("history lock poisoned");
        conn.query_row(
            "SELECT response_body FROM history WHERE id = ?1",
            params![id],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .map(Option::unwrap_or_default)
        .map_err(db)
    }

    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().expect("history lock poisoned");
        conn.execute("DELETE FROM history", []).map_err(db)?;
        Ok(())
    }
}

fn summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistorySummary> {
    Ok(HistorySummary {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        collection_id: row.get(2)?,
        request_path: row.get(3)?,
        request_name: row.get(4)?,
        method: row.get(5)?,
        url: row.get(6)?,
        status: row.get(7)?,
        time_ms: row.get(8)?,
        error: row.get(9)?,
    })
}

fn cap(bytes: &[u8]) -> (Vec<u8>, bool) {
    if bytes.len() > MAX_STORED_BODY {
        (bytes[..MAX_STORED_BODY].to_vec(), true)
    } else {
        (bytes.to_vec(), false)
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn db(e: rusqlite::Error) -> CoreError {
    CoreError::InvalidInput(format!("history database error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(url: &str, auth: &str, body: &str) -> PreparedRequest {
        PreparedRequest {
            method: "POST".into(),
            url: url.into(),
            headers: vec![("Authorization".into(), format!("Bearer {auth}"))],
            body: PreparedBody::Bytes(body.as_bytes().to_vec()),
            timeout: Duration::from_secs(1),
            tls_verify: true,
            ca_cert_paths: vec![],
        }
    }

    fn response(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.as_bytes().to_vec(),
            elapsed: Duration::from_millis(42),
            final_url: "http://x/".into(),
            http_version: "HTTP/1.1".into(),
        }
    }

    #[test]
    fn entries_never_contain_secret_values() {
        let store = HistoryStore::open_in_memory().unwrap();
        let secret = "s3cr3t-token".to_string();
        let req = sent(
            &format!("http://x/?key={secret}"),
            &secret,
            &format!("{{\"t\":\"{secret}\"}}"),
        );
        let entry = NewEntry::from_send(
            "col",
            "a.yaml",
            "A",
            &req,
            Ok(&response(&format!("echo {secret}"))),
        );
        let id = store
            .add(&entry, std::slice::from_ref(&secret), DEFAULT_RETENTION)
            .unwrap();

        let full = store.get(id).unwrap();
        let everything = serde_json::to_string(&full).unwrap();
        assert!(!everything.contains("s3cr3t"), "{everything}");
        assert!(everything.contains("***"));
        assert_eq!(full.summary.status, Some(200));
        assert_eq!(full.response.unwrap().time_ms, 42);
    }

    #[test]
    fn retention_keeps_only_the_newest_entries() {
        let store = HistoryStore::open_in_memory().unwrap();
        for i in 0..5 {
            let req = sent(&format!("http://x/{i}"), "t", "");
            store
                .add(
                    &NewEntry::from_send("col", "a.yaml", "A", &req, Ok(&response(""))),
                    &[],
                    3,
                )
                .unwrap();
        }
        let urls: Vec<String> = store
            .list(None, "", 10)
            .unwrap()
            .into_iter()
            .map(|s| s.url)
            .collect();
        assert_eq!(urls, ["http://x/4", "http://x/3", "http://x/2"]);
    }

    #[test]
    fn search_filters_and_failures_are_stored() {
        let store = HistoryStore::open_in_memory().unwrap();
        let ok = sent("http://x/orders", "t", "");
        store
            .add(
                &NewEntry::from_send("col", "o.yaml", "Orders", &ok, Ok(&response(""))),
                &[],
                10,
            )
            .unwrap();
        let failed = sent("http://x/users", "t", "");
        store
            .add(
                &NewEntry::from_send(
                    "other",
                    "u.yaml",
                    "Users",
                    &failed,
                    Err("connection refused".into()),
                ),
                &[],
                10,
            )
            .unwrap();

        assert_eq!(store.list(None, "order", 10).unwrap().len(), 1);
        let users = store.list(Some("other"), "", 10).unwrap();
        assert_eq!(users[0].error.as_deref(), Some("connection refused"));
        assert!(store.get(users[0].id).unwrap().response.is_none());

        store.clear().unwrap();
        assert!(store.list(None, "", 10).unwrap().is_empty());
    }

    #[test]
    fn large_bodies_are_capped() {
        let store = HistoryStore::open_in_memory().unwrap();
        let big = "a".repeat(MAX_STORED_BODY + 10);
        let req = sent("http://x/", "t", &big);
        let id = store
            .add(
                &NewEntry::from_send("c", "a.yaml", "A", &req, Ok(&response(&big))),
                &[],
                10,
            )
            .unwrap();
        let entry = store.get(id).unwrap();
        assert!(entry.request_body_truncated);
        assert_eq!(entry.request_body.unwrap().len(), MAX_STORED_BODY);
        assert_eq!(store.response_body(id).unwrap().len(), MAX_STORED_BODY);
    }
}
