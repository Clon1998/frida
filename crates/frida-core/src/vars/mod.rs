//! Variable resolution (PLAN.md §5): `{{name}}` placeholders, layered scopes, recursion with
//! cycle detection, dynamic variables and `process.env`, plus the structured report the
//! inspector shows.

mod apply;
mod dynamic;
mod resolver;
mod scope;

use serde::{Deserialize, Serialize};
use specta::Type;

pub use apply::{ResolvedContext, resolve_context, resolve_request};
pub use dynamic::{DYNAMIC_VARIABLES, iso_timestamp};
pub use resolver::{MAX_DEPTH, Resolver};
pub use scope::{ScopeInput, VarDef, VarLayer, VarScope};

/// Where a variable's value came from. Ordered from highest to lowest priority, plus the two
/// separate namespaces `process.env.*` and `$dynamic`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VarSource {
    /// Set by a script with `bru.setVar` (M5). Cleared on restart.
    Runtime,
    Request,
    /// `path` is the folder's collection-relative path.
    Folder {
        path: String,
        name: String,
    },
    CollectionEnv {
        name: String,
    },
    Collection,
    GlobalEnv {
        name: String,
    },
    /// `{{process.env.NAME}}`
    ProcessEnv,
    /// `{{$uuid}}` etc.
    Dynamic,
}

/// Where in the request a variable is used.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UsageLocation {
    Url,
    PathParam {
        name: String,
    },
    Header {
        name: String,
    },
    Body,
    Auth,
    /// Inside another variable's value, e.g. `baseUrl = {{host}}/api`.
    Variable {
        name: String,
    },
}

/// A lower-priority definition of a variable that lost to a higher one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShadowedVar {
    pub source: VarSource,
    /// `None` for secrets in a masked report, or a secret that has no value yet.
    pub value: Option<String>,
    pub is_secret: bool,
}

/// One variable as the inspector shows it (PLAN.md §5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedVar {
    pub name: String,
    /// The fully resolved value. `None` when unresolved, or when masked because it is secret.
    pub value: Option<String>,
    /// `None` when no layer defines the variable.
    pub source: Option<VarSource>,
    pub resolved: bool,
    /// The variable is secret, or its value contains a secret.
    pub is_secret: bool,
    /// The secret's value was removed from this report (see [`ResolveReport::masked`]).
    pub masked: bool,
    pub shadowed: Vec<ShadowedVar>,
    pub used_in: Vec<UsageLocation>,
}

/// Everything the resolver found while resolving one request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveReport {
    /// Each variable once, in order of first use.
    pub vars: Vec<ResolvedVar>,
    /// Problems such as cycles (`a → b → a`) or nesting deeper than [`MAX_DEPTH`].
    pub errors: Vec<String>,
    /// Values of secrets that ended up in the request, so they can be masked in history and
    /// exports. Never sent to the UI ([`ResolveReport::masked`] empties it).
    #[serde(skip)]
    #[specta(skip)]
    pub secret_values: Vec<String>,
}

impl ResolveReport {
    /// A copy that is safe to show in the UI: secret values are removed (PLAN.md §3, the UI
    /// sees secret values only after an explicit "reveal").
    pub fn masked(&self) -> Self {
        let vars = self
            .vars
            .iter()
            .map(|var| {
                let mut var = var.clone();
                if var.is_secret && var.value.is_some() {
                    var.value = None;
                    var.masked = true;
                }
                for shadowed in &mut var.shadowed {
                    if shadowed.is_secret {
                        shadowed.value = None;
                    }
                }
                var
            })
            .collect();
        Self {
            vars,
            errors: self.errors.clone(),
            secret_values: Vec::new(),
        }
    }

    /// Unresolved variable names, for warnings.
    pub fn unresolved(&self) -> impl Iterator<Item = &str> {
        self.vars
            .iter()
            .filter(|v| !v.resolved)
            .map(|v| v.name.as_str())
    }
}
