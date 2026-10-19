//! Bounded authentication of explicitly supplied source compilation units.
//!
//! No path lookup or ambient discovery occurs here. Authentication does not
//! itself type-check or link the modules, and grants no executable authority.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ast::{ImportKind, Module, PackageRef};
use crate::canonical::{
    digest_canonical_artifact, encode_canonical_cbor, CanonicalError, CanonicalValue, CoreDigest,
};
use crate::{parse_module, validate_surface};

/// Digest domain for one exact UTF-8 source encoded as a canonical byte string.
pub const SOURCE_MODULE_DIGEST_DOMAIN: &str = "edict.source/v1";
/// Digest domain and schema identity for the complete source closure.
pub const SOURCE_CLOSURE_DIGEST_DOMAIN: &str = "edict.source-closure/v1";

/// Caller apertures, intersected with fixed compiler ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceClosureLimits {
    /// Maximum supplied compilation units, including the root. Ceiling: 64.
    pub max_modules: usize,
    /// Maximum total UTF-8 source bytes before parsing. Ceiling: 1 MiB.
    pub max_source_bytes: usize,
    /// Maximum units on any root-to-leaf import path. Ceiling: 32.
    pub max_depth: usize,
}

impl Default for SourceClosureLimits {
    fn default() -> Self {
        Self {
            max_modules: 64,
            max_source_bytes: 1024 * 1024,
            max_depth: 32,
        }
    }
}

/// Stable refusal categories at the source-closure authentication boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceClosureErrorKind {
    InvalidSource,
    DuplicateModule,
    MissingModule,
    DigestMismatch,
    DisconnectedModule,
    ImportCycle,
    BudgetExceeded,
    Encoding,
}

/// One refusal, identifying its package when package parsing has succeeded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceClosureError {
    kind: SourceClosureErrorKind,
    coordinate: String,
}

impl SourceClosureError {
    /// Stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> SourceClosureErrorKind {
        self.kind
    }

    /// Package at which the refusal occurred, or empty before package parsing.
    #[must_use]
    pub fn coordinate(&self) -> &str {
        &self.coordinate
    }
}

impl fmt::Display for SourceClosureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.coordinate)
    }
}

impl std::error::Error for SourceClosureError {}

/// An exact source and its parsed surface, retained under one package identity.
#[derive(Debug, Clone)]
pub struct SourceModule {
    source: String,
    parsed: Module,
    digest: CoreDigest,
}

impl SourceModule {
    /// Exact original UTF-8 text, including comments and whitespace.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Parsed source, not a caller-authored replacement AST.
    #[must_use]
    pub const fn parsed(&self) -> &Module {
        &self.parsed
    }

    /// Domain-framed identity of this source's canonical byte string.
    #[must_use]
    pub const fn digest(&self) -> CoreDigest {
        self.digest
    }
}

/// Complete root-reachable graph with every source import authenticated.
///
/// Private fields prevent a caller from substituting a parsed tree or bytes
/// after obtaining this witness.
#[derive(Debug, Clone)]
pub struct SourceClosure {
    root: String,
    modules: BTreeMap<String, SourceModule>,
    canonical_bytes: Vec<u8>,
    digest: CoreDigest,
}

impl SourceClosure {
    /// Explicit root package identity.
    #[must_use]
    pub fn root_coordinate(&self) -> &str {
        &self.root
    }

    /// Complete authenticated source units, in deterministic package order.
    #[must_use]
    pub const fn modules(&self) -> &BTreeMap<String, SourceModule> {
        &self.modules
    }

    /// Canonical `edict.source-closure/v1` artifact bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    /// Domain-framed closure identity, binding root selection and every unit.
    #[must_use]
    pub const fn digest(&self) -> CoreDigest {
        self.digest
    }
}

/// Compute source identity using the established `edict.source/v1` byte schema.
///
/// # Errors
/// Returns a canonical encoding error. This helper does not validate syntax.
pub fn source_module_digest(source: &str) -> Result<CoreDigest, CanonicalError> {
    let bytes = encode_canonical_cbor(&CanonicalValue::Bytes(source.as_bytes().to_vec()))?;
    digest_canonical_artifact(SOURCE_MODULE_DIGEST_DOMAIN, &bytes)
}

/// Authenticate a complete, explicit source graph without filesystem discovery.
///
/// Resource checks precede parsing. Topology checks precede digest comparison;
/// no successful witness exists until both pass. Every supplied dependency must
/// be root-reachable, and every dependency edge must carry its exact source pin.
///
/// # Errors
/// Returns a typed refusal for invalid sources, graph/pin mismatches, exceeded
/// apertures, or canonical encoding failures.
pub fn resolve_source_closure(
    root: &str,
    dependencies: &[&str],
    limits: SourceClosureLimits,
) -> Result<SourceClosure, SourceClosureError> {
    let ceiling = SourceClosureLimits::default();
    let budget = || failure(SourceClosureErrorKind::BudgetExceeded, "");
    if dependencies.len().saturating_add(1) > limits.max_modules.min(ceiling.max_modules) {
        return Err(budget());
    }
    let bytes = dependencies
        .iter()
        .try_fold(root.len(), |total, source| total.checked_add(source.len()))
        .ok_or_else(budget)?;
    if bytes > limits.max_source_bytes.min(ceiling.max_source_bytes) {
        return Err(budget());
    }
    let mut modules = BTreeMap::new();
    let mut root_coordinate = None;
    for source in std::iter::once(root).chain(dependencies.iter().copied()) {
        let parsed =
            parse_module(source).map_err(|_| failure(SourceClosureErrorKind::InvalidSource, ""))?;
        let coordinate = coordinate(&parsed.package);
        validate_surface(&parsed)
            .map_err(|_| failure(SourceClosureErrorKind::InvalidSource, &coordinate))?;
        let digest = source_module_digest(source)
            .map_err(|_| failure(SourceClosureErrorKind::Encoding, &coordinate))?;
        if root_coordinate.is_none() {
            root_coordinate = Some(coordinate.clone());
        }
        if modules
            .insert(
                coordinate.clone(),
                SourceModule {
                    source: source.to_owned(),
                    parsed,
                    digest,
                },
            )
            .is_some()
        {
            return Err(failure(
                SourceClosureErrorKind::DuplicateModule,
                &coordinate,
            ));
        }
    }
    let root_coordinate = root_coordinate.ok_or_else(budget)?;
    let mut heights = BTreeMap::new();
    let height = visit(
        &root_coordinate,
        &modules,
        &mut BTreeSet::new(),
        &mut heights,
    )?;
    if height > limits.max_depth.min(ceiling.max_depth) {
        return Err(budget());
    }
    if let Some(coordinate) = modules
        .keys()
        .find(|coordinate| !heights.contains_key(*coordinate))
    {
        return Err(failure(
            SourceClosureErrorKind::DisconnectedModule,
            coordinate,
        ));
    }
    for module in modules.values() {
        for import in module
            .parsed
            .imports
            .iter()
            .filter(|import| import.kind == ImportKind::Module)
        {
            let imported = import
                .package
                .as_ref()
                .ok_or_else(|| failure(SourceClosureErrorKind::InvalidSource, ""))?;
            let imported_coordinate = coordinate(imported);
            let expected = modules.get(&imported_coordinate).ok_or_else(|| {
                failure(SourceClosureErrorKind::MissingModule, &imported_coordinate)
            })?;
            if import.digest.as_deref() != Some(expected.digest.to_review_string().as_str()) {
                return Err(failure(
                    SourceClosureErrorKind::DigestMismatch,
                    &imported_coordinate,
                ));
            }
        }
    }
    let value = closure_value(&root_coordinate, &modules);
    let canonical_bytes = encode_canonical_cbor(&value)
        .map_err(|_| failure(SourceClosureErrorKind::Encoding, &root_coordinate))?;
    let digest = digest_canonical_artifact(SOURCE_CLOSURE_DIGEST_DOMAIN, &canonical_bytes)
        .map_err(|_| failure(SourceClosureErrorKind::Encoding, &root_coordinate))?;
    Ok(SourceClosure {
        root: root_coordinate,
        modules,
        canonical_bytes,
        digest,
    })
}

fn closure_value(root: &str, modules: &BTreeMap<String, SourceModule>) -> CanonicalValue {
    map([
        ("apiVersion", text(SOURCE_CLOSURE_DIGEST_DOMAIN)),
        ("root", text(root)),
        (
            "modules",
            CanonicalValue::Map(
                modules
                    .iter()
                    .map(|(coordinate, module)| {
                        (
                            text(coordinate),
                            map([
                                (
                                    "digest",
                                    CanonicalValue::Array(vec![
                                        text("sha256"),
                                        CanonicalValue::Bytes(module.digest.bytes().to_vec()),
                                    ]),
                                ),
                                (
                                    "source",
                                    CanonicalValue::Bytes(module.source.as_bytes().to_vec()),
                                ),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
    ])
}

fn visit(
    name: &str,
    modules: &BTreeMap<String, SourceModule>,
    visiting: &mut BTreeSet<String>,
    heights: &mut BTreeMap<String, usize>,
) -> Result<usize, SourceClosureError> {
    if let Some(height) = heights.get(name) {
        return Ok(*height);
    }
    if !visiting.insert(name.to_owned()) {
        return Err(failure(SourceClosureErrorKind::ImportCycle, name));
    }
    let module = modules
        .get(name)
        .ok_or_else(|| failure(SourceClosureErrorKind::MissingModule, name))?;
    let mut height = 1;
    for import in module
        .parsed
        .imports
        .iter()
        .filter(|import| import.kind == ImportKind::Module)
    {
        let package = import
            .package
            .as_ref()
            .ok_or_else(|| failure(SourceClosureErrorKind::InvalidSource, name))?;
        // Heights are context-free, so a shallow cached visit cannot bless a
        // deeper occurrence of the same shared dependency.
        height = height.max(1 + visit(&coordinate(package), modules, visiting, heights)?);
    }
    visiting.remove(name);
    heights.insert(name.to_owned(), height);
    Ok(height)
}

fn coordinate(package: &PackageRef) -> String {
    format!("{}@{}", package.path.join("."), package.version)
}
fn failure(kind: SourceClosureErrorKind, coordinate: &str) -> SourceClosureError {
    SourceClosureError {
        kind,
        coordinate: coordinate.to_owned(),
    }
}
fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}
fn map<const N: usize>(values: [(&str, CanonicalValue); N]) -> CanonicalValue {
    CanonicalValue::Map(
        values
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}
