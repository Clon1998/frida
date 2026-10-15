//! Importers (PLAN.md §11). curl for now; OpenAPI (M6) and Bruno (M9) follow.

mod curl;

pub use curl::{CurlImport, import_curl};
