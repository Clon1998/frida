//! Basic product information. Used by the M0 example command to prove the
//! Rust → TypeScript type generation works end to end.

use serde::Serialize;
use specta::Type;

// `#[derive(Serialize)]` lets serde turn this into JSON (like a Jackson-annotated DTO), and
// `#[derive(Type)]` lets specta describe its shape so tauri-specta can generate a matching
// TypeScript type. `rename_all = "camelCase"` makes the JSON/TS fields idiomatic for JS.
// (Plain `//` comments here, because `///` doc comments are copied into the generated TS.)
/// Product name and version of the core library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub product_name: String,
    pub core_version: String,
}

/// Returns information about this build of Frida.
pub fn about() -> AboutInfo {
    AboutInfo {
        product_name: "Frida".to_string(),
        // `env!` reads an environment variable at *compile* time. Cargo sets
        // `CARGO_PKG_VERSION` to the crate's version from Cargo.toml.
        core_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_reports_product_name_and_crate_version() {
        let info = about();
        assert_eq!(info.product_name, "Frida");
        assert_eq!(info.core_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn about_serializes_with_camel_case_fields() {
        let json = serde_json::to_value(about()).unwrap();
        assert!(json.get("productName").is_some());
        assert!(json.get("coreVersion").is_some());
    }
}
