//! The domain model: one Rust type per on-disk file (PLAN.md §4).
//!
//! Every type derives `Serialize`/`Deserialize` (YAML via serde) and `specta::Type` (generated
//! TypeScript types), so the UI edits exactly the same shape that is written to disk.

mod collection;
mod common;
mod environment;
mod request;
mod workspace;

pub use collection::{Collection, Folder};
pub use common::{
    ApiKeyLocation, Auth, AuthScheme, CollectionSettings, FORMAT_VERSION, KeyValue,
    OAuth2ClientAuth, OAuth2Config, OAuth2Grant, Scripts, Settings, TlsSettings, TokenPlacement,
};
pub use environment::{EnvVariable, Environment, SecretSource};
pub use request::{Body, BodyMode, MultipartField, MultipartKind, Params, PathParam, Request};
pub use workspace::{CollectionRef, Workspace};
