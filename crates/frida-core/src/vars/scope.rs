//! The layers a variable can be defined in (PLAN.md §5.1), highest priority first.

use std::collections::HashMap;

use super::VarSource;
use crate::model::{Collection, Environment, Folder, KeyValue, Request};

/// One variable definition in a layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDef {
    pub name: String,
    /// `None` for a secret whose value isn't set (yet).
    pub value: Option<String>,
    pub is_secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarLayer {
    pub source: VarSource,
    pub vars: Vec<VarDef>,
}

/// All layers, ordered from highest to lowest priority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VarScope {
    pub layers: Vec<VarLayer>,
}

/// Everything a request's variables can come from.
///
/// Environments must already have their secret values filled in (from the keychain) by the
/// caller, because reading the keychain is blocking I/O that must not happen on the async
/// runtime (see `frida_core::secrets::fill_secret_values`).
pub struct ScopeInput<'a> {
    pub runtime: &'a HashMap<String, String>,
    pub request: &'a Request,
    /// The folders containing the request as `(path, folder)`, outermost first.
    pub folders: &'a [(String, Folder)],
    pub collection_env: Option<&'a Environment>,
    pub collection: &'a Collection,
    pub global_env: Option<&'a Environment>,
}

impl VarScope {
    /// Builds the layers in precedence order: runtime, request, folders (nearest first),
    /// collection environment, collection, global environment.
    pub fn build(input: &ScopeInput<'_>) -> Self {
        let mut layers = Vec::new();

        let mut runtime: Vec<VarDef> = input
            .runtime
            .iter()
            .map(|(name, value)| VarDef {
                name: name.clone(),
                value: Some(value.clone()),
                is_secret: false,
            })
            .collect();
        // HashMap order is random; sort so results are deterministic.
        runtime.sort_by(|a, b| a.name.cmp(&b.name));
        layers.push(VarLayer {
            source: VarSource::Runtime,
            vars: runtime,
        });

        layers.push(VarLayer {
            source: VarSource::Request,
            vars: key_values(&input.request.vars),
        });

        // Nearest folder first, i.e. the reverse of the outermost-first input order.
        for (path, folder) in input.folders.iter().rev() {
            layers.push(VarLayer {
                source: VarSource::Folder {
                    path: path.clone(),
                    name: folder.name.clone(),
                },
                vars: key_values(&folder.vars),
            });
        }

        if let Some(env) = input.collection_env {
            layers.push(VarLayer {
                source: VarSource::CollectionEnv {
                    name: env.name.clone(),
                },
                vars: env_vars(env),
            });
        }

        layers.push(VarLayer {
            source: VarSource::Collection,
            vars: key_values(&input.collection.vars),
        });

        if let Some(env) = input.global_env {
            layers.push(VarLayer {
                source: VarSource::GlobalEnv {
                    name: env.name.clone(),
                },
                vars: env_vars(env),
            });
        }

        Self { layers }
    }

    /// Every definition of `name`, highest priority first.
    pub fn definitions<'s>(
        &'s self,
        name: &'s str,
    ) -> impl Iterator<Item = (&'s VarSource, &'s VarDef)> {
        self.layers.iter().flat_map(move |layer| {
            layer
                .vars
                .iter()
                .filter(move |def| def.name == name)
                .map(move |def| (&layer.source, def))
        })
    }
}

/// Enabled rows only (PLAN.md §5.2: disabled rows are ignored).
fn key_values(rows: &[KeyValue]) -> Vec<VarDef> {
    rows.iter()
        .filter(|row| row.enabled && !row.name.trim().is_empty())
        .map(|row| VarDef {
            name: row.name.trim().to_string(),
            value: Some(row.value.clone()),
            is_secret: false,
        })
        .collect()
}

fn env_vars(env: &Environment) -> Vec<VarDef> {
    env.variables
        .iter()
        .filter(|var| !var.name.trim().is_empty())
        .map(|var| VarDef {
            name: var.name.trim().to_string(),
            value: var.value.clone(),
            is_secret: var.secret,
        })
        .collect()
}
