//! This module owns effect cost parameters (#341).
//!
//! An `edict.target-profile.intrinsics/v3` catalog declares, in its top-level
//! `parameters` map, where each named template parameter lives in the
//! application's operation configuration and what kind of value it is: a
//! `uint`, or a `helper` naming one of the module's source helpers. The
//! compiler reads the configuration only at those declared paths and only for
//! the parameters a selected effect's template uses. A missing value or a
//! value of the wrong kind refuses the effect.
use crate::canonical::CanonicalValue;
use std::collections::BTreeMap;

const MAX_DECLARATIONS: usize = 64;
const MAX_PATH_FIELDS: usize = 16;
/// The provider refuses a longer helper name.
const MAX_HELPER_NAME_BYTES: usize = 1024;

/// What a declared parameter's configuration value must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterKind {
    /// An unsigned 64-bit number.
    Uint,
    /// The name of a source helper in the compiled module.
    Helper,
}

/// One parameter read from the operation configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParameterValue {
    Uint(u64),
    Helper(String),
}

struct Declaration {
    path: Vec<String>,
    kind: ParameterKind,
}

/// A catalog's validated `parameters` map.
pub(crate) struct ParameterDeclarations(BTreeMap<String, Declaration>);

fn text(value: &CanonicalValue) -> Option<&str> {
    match value {
        CanonicalValue::Text(text) if !text.is_empty() => Some(text),
        _ => None,
    }
}

/// The value of `name` in a map whose keys are distinct text strings.
fn field<'v>(value: &'v CanonicalValue, name: &str) -> Option<&'v CanonicalValue> {
    let CanonicalValue::Map(entries) = value else {
        return None;
    };
    let mut matching = entries
        .iter()
        .filter(|(key, _)| matches!(key, CanonicalValue::Text(key) if key == name));
    let found = matching.next()?;
    matching.next().is_none().then_some(&found.1)
}

fn declaration(value: &CanonicalValue) -> Option<Declaration> {
    let CanonicalValue::Map(entries) = value else {
        return None;
    };
    if entries.len() != 2 {
        return None;
    }
    let CanonicalValue::Array(names) = field(value, "path")? else {
        return None;
    };
    if !(1..=MAX_PATH_FIELDS).contains(&names.len()) {
        return None;
    }
    let path = names
        .iter()
        .map(|name| text(name).map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    let kind = match text(field(value, "kind")?)? {
        "uint" => ParameterKind::Uint,
        "helper" => ParameterKind::Helper,
        _ => return None,
    };
    Some(Declaration { path, kind })
}

impl ParameterDeclarations {
    /// Parse a catalog's `parameters` map; an absent map declares nothing.
    /// `None` when the map or any declaration is malformed.
    pub(crate) fn parse(value: Option<&CanonicalValue>) -> Option<Self> {
        let Some(value) = value else {
            return Some(Self(BTreeMap::new()));
        };
        let CanonicalValue::Map(entries) = value else {
            return None;
        };
        if entries.len() > MAX_DECLARATIONS {
            return None;
        }
        let mut declarations = BTreeMap::new();
        for (name, declared) in entries {
            let previous = declarations.insert(text(name)?.to_owned(), declaration(declared)?);
            if previous.is_some() {
                return None;
            }
        }
        Some(Self(declarations))
    }

    /// Read every used parameter from the decoded operation configuration.
    ///
    /// `None` when a use names an undeclared parameter or one of another
    /// kind, when the configuration is absent, or when the declared path does
    /// not lead to a value of the declared kind.
    pub(crate) fn resolve(
        &self,
        uses: &[(&str, ParameterKind)],
        configuration: Option<&CanonicalValue>,
    ) -> Option<BTreeMap<String, ParameterValue>> {
        let mut resolved = BTreeMap::new();
        for (name, kind) in uses {
            let declared = self
                .0
                .get(*name)
                .filter(|declared| declared.kind == *kind)?;
            let mut value = configuration?;
            for name in &declared.path {
                value = field(value, name)?;
            }
            let value = match (declared.kind, value) {
                (ParameterKind::Uint, CanonicalValue::Integer(number)) => {
                    ParameterValue::Uint(u64::try_from(*number).ok()?)
                }
                (ParameterKind::Helper, value) => ParameterValue::Helper(
                    text(value)
                        .filter(|name| name.len() <= MAX_HELPER_NAME_BYTES)?
                        .to_owned(),
                ),
                _ => return None,
            };
            resolved.insert((*name).to_owned(), value);
        }
        Some(resolved)
    }
}
