//! The interpolation engine: replaces `{{name}}` placeholders and records what it did.

use std::collections::HashMap;

use super::scope::VarScope;
use super::{ResolveReport, ResolvedVar, ShadowedVar, UsageLocation, VarSource, dynamic};

/// Variables may reference other variables up to this depth (PLAN.md §5.2): in
/// `{{v0}}` → `{{v1}}` → … → `{{v10}}`, `v10` is 10 levels below the variable used in the request.
pub const MAX_DEPTH: usize = 10;

/// Resolves text against a [`VarScope`] and collects a [`ResolveReport`] along the way.
///
/// Use one resolver per send (or per inspection), call [`Resolver::resolve`] for every piece of
/// text, then [`Resolver::finish`] to get the report.
pub struct Resolver<'a> {
    scope: &'a VarScope,
    report: ResolveReport,
    /// Position of each variable in `report.vars`, for quick updates.
    index: HashMap<String, usize>,
}

/// The result of resolving one placeholder or one piece of text.
struct Resolved {
    text: String,
    /// The text contains a secret value.
    secret: bool,
}

impl<'a> Resolver<'a> {
    pub fn new(scope: &'a VarScope) -> Self {
        Self {
            scope,
            report: ResolveReport::default(),
            index: HashMap::new(),
        }
    }

    /// Replaces every `{{name}}` in `text`. Unresolved placeholders stay as written
    /// (PLAN.md §5.2), so the request can still be sent and the user sees what's missing.
    pub fn resolve(&mut self, text: &str, location: UsageLocation) -> String {
        let mut stack = Vec::new();
        self.interpolate(text, &location, &mut stack).text
    }

    pub fn finish(self) -> ResolveReport {
        self.report
    }

    /// `stack` holds the variables currently being expanded, to detect cycles.
    fn interpolate(
        &mut self,
        text: &str,
        location: &UsageLocation,
        stack: &mut Vec<String>,
    ) -> Resolved {
        let mut out = String::with_capacity(text.len());
        let mut secret = false;
        let mut rest = text;

        while let Some(start) = rest.find("{{") {
            let Some(len) = rest[start + 2..].find("}}") else {
                break; // no closing braces: the rest is plain text
            };
            out.push_str(&rest[..start]);
            let placeholder = &rest[start..start + 2 + len + 2];
            let name = rest[start + 2..start + 2 + len].trim();
            rest = &rest[start + 2 + len + 2..];

            match self.placeholder(name, location, stack) {
                Some(resolved) => {
                    secret |= resolved.secret;
                    out.push_str(&resolved.text);
                }
                None => out.push_str(placeholder),
            }
        }
        out.push_str(rest);
        Resolved { text: out, secret }
    }

    /// The value for one placeholder, or `None` to leave it as written.
    fn placeholder(
        &mut self,
        name: &str,
        location: &UsageLocation,
        stack: &mut Vec<String>,
    ) -> Option<Resolved> {
        if name.is_empty() {
            return None;
        }

        if let Some(var) = name.strip_prefix("process.env.") {
            let value = std::env::var(var).ok();
            self.record(
                name,
                value.clone(),
                Some(VarSource::ProcessEnv),
                false,
                Vec::new(),
                location,
            );
            return value.map(|text| Resolved {
                text,
                secret: false,
            });
        }

        if let Some(dynamic_name) = name.strip_prefix('$') {
            let value = dynamic::generate(dynamic_name);
            self.record(
                name,
                value.clone(),
                Some(VarSource::Dynamic),
                false,
                Vec::new(),
                location,
            );
            return value.map(|text| Resolved {
                text,
                secret: false,
            });
        }

        // The highest layer that defines the name wins; the others are "shadowed".
        let mut definitions = self.scope.definitions(name);
        let Some((source, winner)) = definitions.next() else {
            self.record(name, None, None, false, Vec::new(), location);
            return None;
        };
        let source = source.clone();
        let winner = winner.clone();
        let shadowed: Vec<ShadowedVar> = definitions
            .map(|(source, def)| ShadowedVar {
                source: source.clone(),
                value: def.value.clone(),
                is_secret: def.is_secret,
            })
            .collect();

        let Some(raw_value) = winner.value else {
            // A secret without a value: flagged as unresolved, never falls back to a lower layer.
            self.record(
                name,
                None,
                Some(source),
                winner.is_secret,
                shadowed,
                location,
            );
            return None;
        };

        if let Some(pos) = stack.iter().position(|n| n == name) {
            let mut cycle: Vec<&str> = stack[pos..].iter().map(String::as_str).collect();
            cycle.push(name);
            self.error(format!("variable cycle: {}", cycle.join(" → ")));
            return None;
        }
        // `stack` holds the variables being expanded above this one, so its length is this
        // variable's nesting level (0 for a variable used directly in the request).
        if stack.len() > MAX_DEPTH {
            self.error(format!(
                "variable {name:?} is nested more than {MAX_DEPTH} levels deep"
            ));
            return None;
        }

        // Resolve variables inside the value. Their usage is recorded as "inside `name`".
        stack.push(name.to_string());
        let inner_location = UsageLocation::Variable {
            name: stack[0].clone(),
        };
        let inner = self.interpolate(&raw_value, &inner_location, stack);
        stack.pop();

        let secret = winner.is_secret || inner.secret;
        if secret && !inner.text.is_empty() && !self.report.secret_values.contains(&inner.text) {
            self.report.secret_values.push(inner.text.clone());
        }
        self.record(
            name,
            Some(inner.text.clone()),
            Some(source),
            secret,
            shadowed,
            location,
        );
        Some(Resolved {
            text: inner.text,
            secret,
        })
    }

    fn record(
        &mut self,
        name: &str,
        value: Option<String>,
        source: Option<VarSource>,
        is_secret: bool,
        shadowed: Vec<ShadowedVar>,
        location: &UsageLocation,
    ) {
        let resolved = value.is_some();
        match self.index.get(name) {
            Some(&i) => {
                let var = &mut self.report.vars[i];
                // Dynamic variables change per use; show the latest value.
                if value.is_some() {
                    var.value = value;
                }
                if !var.used_in.contains(location) {
                    var.used_in.push(location.clone());
                }
            }
            None => {
                self.index.insert(name.to_string(), self.report.vars.len());
                self.report.vars.push(ResolvedVar {
                    name: name.to_string(),
                    value,
                    source,
                    resolved,
                    is_secret,
                    masked: false,
                    shadowed,
                    used_in: vec![location.clone()],
                });
            }
        }
    }

    fn error(&mut self, message: String) {
        if !self.report.errors.contains(&message) {
            self.report.errors.push(message);
        }
    }
}
