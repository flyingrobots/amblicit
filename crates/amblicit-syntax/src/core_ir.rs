//! In-memory Edict Core IR value model for the compiler-spine stage.
//!
//! These Rust values mirror the `edict.core/v1` semantic shape closely enough
//! for source-to-Core lowering tests. They are not canonical bytes, do not carry
//! their own digest, and do not represent target IR or admission bundles.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

mod source_functions;
mod variant_expressions;
pub(crate) use source_functions::validate_function_graph;
pub(crate) use variant_expressions::{core_variant_case, core_variant_identity_matches};

/// The Core ABI identifier emitted by this crate.
pub const CORE_API_VERSION: &str = "edict.core/v1";

/// Compiler-owned local identity for the single application intent input.
pub(crate) const CORE_APPLICATION_INPUT_LOCAL_ID: &str = "arg.0";

/// Shared ceiling for structural syntax and fully expanded semantic type graphs.
pub(crate) const MAX_CORE_TYPE_DEPTH: usize = 128;

/// Shared ceiling for expression, predicate and nested-block descent.
pub(crate) const MAX_CORE_GRAPH_DEPTH: usize = 128;

/// A lowered in-memory Core module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreModule {
    pub api_version: String,
    pub coordinate: String,
    pub imports: Vec<CoreImport>,
    pub types: BTreeMap<String, CoreType>,
    pub functions: BTreeMap<String, CoreFunction>,
    pub intents: BTreeMap<String, CoreIntent>,
    pub required_core_capabilities: Vec<String>,
}

/// One source-owned pure function, authenticated by the containing Core module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreFunction {
    pub params: Vec<LocalRef>,
    pub return_type: String,
    pub body: CorePureBlock,
}

/// Ordered immutable pure bindings followed by one result expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePureBlock {
    pub locals: Vec<LocalRef>,
    pub bindings: Vec<CorePureBinding>,
    pub result: CoreExpr,
}

/// A pure local definition, unable to contain an effect or guard node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePureBinding {
    pub binding: LocalRef,
    pub value: CoreExpr,
}

/// A Core module proven to satisfy the complete type-integrity judgment.
///
/// The witness borrows the raw module, so safe Rust cannot mutate that module
/// while a consumer relies on the validation result. Its private field makes
/// successful [`validate_core_module_type_integrity`] the only constructor.
#[derive(Debug, Clone, Copy)]
pub struct ValidatedCoreModule<'a> {
    module: &'a CoreModule,
}

impl<'a> ValidatedCoreModule<'a> {
    /// Borrow the exact raw module covered by this witness.
    #[must_use]
    pub const fn module(self) -> &'a CoreModule {
        self.module
    }
}

/// Stable categories for whole-module Core type-integrity failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreTypeIntegrityFailureKind {
    InvalidTableKey,
    InvalidDefinition,
    InvalidReference,
    UnresolvedNamedReference,
    NominalContractMismatch,
    ReferenceCycle,
    DepthExceeded,
}

/// Structured failure from the authoritative Core type-integrity judgment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTypeIntegrityFailure {
    kind: CoreTypeIntegrityFailureKind,
    path: String,
}

impl CoreTypeIntegrityFailure {
    /// Return the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> CoreTypeIntegrityFailureKind {
        self.kind
    }

    /// Return the structural path to the invalid definition, reference or graph node.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    fn new(kind: CoreTypeIntegrityFailureKind, path: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.into(),
        }
    }
}

impl fmt::Display for CoreTypeIntegrityFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.path)
    }
}

impl std::error::Error for CoreTypeIntegrityFailure {}

/// A Core import that survives source resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreImport {
    pub kind: CoreImportKind,
    pub resource: ResourceRef,
    pub alias: Option<String>,
}

/// Core import kinds. Shape imports are source-only and do not lower to Core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreImportKind {
    Lawpack,
    Target,
    Core,
    Capability,
}

impl CoreImportKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lawpack => "lawpack",
            Self::Target => "target",
            Self::Core => "core",
            Self::Capability => "capability",
        }
    }
}

/// Digest-locked external artifact reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRef {
    pub coordinate: String,
    pub digest: Option<String>,
}

impl ResourceRef {
    #[must_use]
    pub(crate) fn is_digest_locked(&self) -> bool {
        !self.coordinate.is_empty() && self.digest.as_deref().is_some_and(is_sha256_review_digest)
    }
}

pub(crate) fn is_sha256_review_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn is_lowercase_sha256_review_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn parse_core_integer(width: &str, value: &str) -> Option<i128> {
    match width {
        "I8" => parse_signed_integer(value, i8::MIN.into(), i8::MAX.into()),
        "I16" => parse_signed_integer(value, i16::MIN.into(), i16::MAX.into()),
        "I32" => parse_signed_integer(value, i32::MIN.into(), i32::MAX.into()),
        "I64" => parse_signed_integer(value, i64::MIN.into(), i64::MAX.into()),
        "U8" => parse_unsigned_integer(value, u8::MAX.into()),
        "U16" => parse_unsigned_integer(value, u16::MAX.into()),
        "U32" => parse_unsigned_integer(value, u32::MAX.into()),
        "U64" => parse_unsigned_integer(value, u64::MAX.into()),
        _ => None,
    }
}

fn parse_signed_integer(value: &str, min: i128, max: i128) -> Option<i128> {
    value
        .parse::<i128>()
        .ok()
        .filter(|value| (min..=max).contains(value))
}

fn parse_unsigned_integer(value: &str, max: u128) -> Option<i128> {
    value
        .parse::<u128>()
        .ok()
        .filter(|value| *value <= max)
        .and_then(|value| i128::try_from(value).ok())
}

/// Core type model for the initial compiler-spine subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreType {
    Bool,
    Unit,
    Int {
        width: String,
    },
    String {
        max: u64,
        canonical: String,
    },
    Bytes {
        min: Option<u64>,
        max: u64,
    },
    Nominal {
        contract: String,
        representation: String,
    },
    Record {
        fields: BTreeMap<String, String>,
    },
    Variant {
        cases: BTreeMap<String, Option<String>>,
    },
    Option {
        item: String,
    },
    List {
        item: String,
        max: u64,
    },
    Map {
        key: String,
        value: String,
        max: u64,
    },
    CapabilityRef {
        item: String,
    },
    ExternalActionRequest {
        settlement: String,
    },
}

/// State-free classification of a Core type reference.
///
/// Intrinsic and structural references carry their own meaning. Named
/// references acquire meaning only from an exact entry in [`CoreModule::types`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoreTypeReference {
    Intrinsic(CoreType),
    Structural(CoreType),
    Named,
}

/// Classify one canonical Core type reference without consulting module state.
pub(crate) fn classify_core_type_reference(reference: &str) -> Option<CoreTypeReference> {
    classify_core_type_reference_at_depth(reference, 0)
}

fn classify_core_type_reference_at_depth(
    reference: &str,
    depth: usize,
) -> Option<CoreTypeReference> {
    if depth > MAX_CORE_TYPE_DEPTH {
        return None;
    }
    if reference == "Bool" {
        return Some(CoreTypeReference::Intrinsic(CoreType::Bool));
    }
    if reference == "Unit" {
        return Some(CoreTypeReference::Intrinsic(CoreType::Unit));
    }
    if matches!(
        reference,
        "I8" | "I16" | "I32" | "I64" | "U8" | "U16" | "U32" | "U64"
    ) {
        return Some(CoreTypeReference::Intrinsic(CoreType::Int {
            width: reference.to_owned(),
        }));
    }
    if looks_like_structural_type_reference(reference) {
        let parsed = parse_structural_core_type_at_depth(reference, depth)?;
        return Some(CoreTypeReference::Structural(parsed));
    }
    if is_named_core_type_reference(reference) {
        Some(CoreTypeReference::Named)
    } else {
        None
    }
}

fn looks_like_structural_type_reference(reference: &str) -> bool {
    [
        "String<",
        "Bytes<",
        "Record<",
        "Option<",
        "List<",
        "Map<",
        "CapabilityRef<",
        "ExternalActionRequest<",
        "edict.external-action.request/v1<",
        "Nominal<",
        "Variant<",
    ]
    .into_iter()
    .any(|prefix| reference.starts_with(prefix))
}

fn is_named_core_type_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference != "anonymous.record"
        && !matches!(
            reference,
            "String"
                | "Bytes"
                | "Record"
                | "Option"
                | "List"
                | "Map"
                | "CapabilityRef"
                | "ExternalActionRequest"
                | "Nominal"
                | "Variant"
        )
        && reference.chars().all(|character| {
            !character.is_whitespace() && !matches!(character, '<' | '>' | ',' | ':' | '=')
        })
}

fn parse_canonical_u64(value: &str) -> Option<u64> {
    let parsed: u64 = value.parse().ok()?;
    (parsed.to_string() == value).then_some(parsed)
}

fn parse_structural_string_type(reference: &str) -> Option<CoreType> {
    let inner = reference.strip_prefix("String<max=")?.strip_suffix('>')?;
    let (max, canonical) = inner.split_once(",canonical=")?;
    if !matches!(canonical, "raw-utf8" | "unicode-scalar-nfc") {
        return None;
    }
    Some(CoreType::String {
        max: parse_canonical_u64(max)?,
        canonical: canonical.to_owned(),
    })
}

fn parse_structural_bytes_type(reference: &str) -> Option<CoreType> {
    let inner = reference.strip_prefix("Bytes<")?.strip_suffix('>')?;
    if let Some(bounds) = inner.strip_prefix("min=") {
        let (min, max) = bounds.split_once(",max=")?;
        let min = parse_canonical_u64(min)?;
        let max = parse_canonical_u64(max)?;
        return (min < max).then_some(CoreType::Bytes {
            min: Some(min),
            max,
        });
    }
    if let Some(max) = inner.strip_prefix("max=") {
        return Some(CoreType::Bytes {
            min: None,
            max: parse_canonical_u64(max)?,
        });
    }
    let exact = parse_canonical_u64(inner.strip_prefix("exact=")?)?;
    Some(CoreType::Bytes {
        min: Some(exact),
        max: exact,
    })
}

fn parse_structural_core_type_at_depth(reference: &str, depth: usize) -> Option<CoreType> {
    if reference.starts_with("String<") {
        return parse_structural_string_type(reference);
    }
    if reference.starts_with("Bytes<") {
        return parse_structural_bytes_type(reference);
    }
    if let Some(inner) = reference
        .strip_prefix("Record<")
        .and_then(|value| value.strip_suffix('>'))
    {
        let mut fields = BTreeMap::new();
        let mut previous_name = None;
        for field in split_top_level_type_parts(inner)? {
            let (name, ty) = field.split_once(':')?;
            if !is_core_field_name(name)
                || ty.is_empty()
                || previous_name.is_some_and(|previous| previous >= name)
                || classify_core_type_reference_at_depth(ty, depth + 1).is_none()
                || fields.insert(name.to_owned(), ty.to_owned()).is_some()
            {
                return None;
            }
            previous_name = Some(name);
        }
        return Some(CoreType::Record { fields });
    }
    if let Some(item) = reference
        .strip_prefix("Option<")
        .and_then(|value| value.strip_suffix('>'))
    {
        classify_core_type_reference_at_depth(item, depth + 1)?;
        return Some(CoreType::Option {
            item: item.to_owned(),
        });
    }
    if let Some(inner) = reference
        .strip_prefix("List<")
        .and_then(|value| value.strip_suffix('>'))
    {
        let parts = split_top_level_type_parts(inner)?;
        let [item, max] = parts.as_slice() else {
            return None;
        };
        classify_core_type_reference_at_depth(item, depth + 1)?;
        let max = max.strip_prefix("max=")?;
        let parsed_max: u64 = max.parse().ok()?;
        if parsed_max.to_string() != max {
            return None;
        }
        return Some(CoreType::List {
            item: (*item).to_owned(),
            max: parsed_max,
        });
    }
    if let Some(inner) = reference
        .strip_prefix("Map<")
        .and_then(|value| value.strip_suffix('>'))
    {
        let parts = split_top_level_type_parts(inner)?;
        let [key, value, max] = parts.as_slice() else {
            return None;
        };
        classify_core_type_reference_at_depth(key, depth + 1)?;
        classify_core_type_reference_at_depth(value, depth + 1)?;
        let max = max.strip_prefix("max=")?;
        let parsed_max: u64 = max.parse().ok()?;
        if parsed_max.to_string() != max {
            return None;
        }
        return Some(CoreType::Map {
            key: (*key).to_owned(),
            value: (*value).to_owned(),
            max: parsed_max,
        });
    }
    if let Some(item) = reference
        .strip_prefix("CapabilityRef<")
        .and_then(|value| value.strip_suffix('>'))
    {
        classify_core_type_reference_at_depth(item, depth + 1)?;
        return Some(CoreType::CapabilityRef {
            item: item.to_owned(),
        });
    }
    let settlement = reference
        .strip_prefix("edict.external-action.request/v1<")?
        .strip_suffix('>')?;
    classify_core_type_reference_at_depth(settlement, depth + 1)?;
    Some(CoreType::ExternalActionRequest {
        settlement: settlement.to_owned(),
    })
}

fn split_top_level_type_parts(value: &str) -> Option<Vec<&str>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let mut depth = 0_usize;
    let mut start = 0_usize;
    let mut parts = Vec::new();
    for (index, character) in value.char_indices() {
        match character {
            '<' => depth = depth.checked_add(1)?,
            '>' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                parts.push(&value[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    parts.push(&value[start..]);
    Some(parts)
}

fn is_core_field_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Render an intrinsic or structural Core type as its canonical reference.
pub(crate) fn render_self_describing_core_type(ty: &CoreType) -> Option<String> {
    render_self_describing_core_type_at_depth(ty, 0)
}

fn render_self_describing_core_type_at_depth(ty: &CoreType, depth: usize) -> Option<String> {
    if depth > MAX_CORE_TYPE_DEPTH {
        return None;
    }
    match ty {
        CoreType::Bool => Some("Bool".to_owned()),
        CoreType::Unit => Some("Unit".to_owned()),
        CoreType::Int { width }
            if matches!(
                width.as_str(),
                "I8" | "I16" | "I32" | "I64" | "U8" | "U16" | "U32" | "U64"
            ) =>
        {
            Some(width.clone())
        }
        CoreType::String { max, canonical }
            if matches!(canonical.as_str(), "raw-utf8" | "unicode-scalar-nfc") =>
        {
            Some(format!("String<max={max},canonical={canonical}>"))
        }
        CoreType::Bytes { min, max } if min.is_none_or(|min| min <= *max) => match min {
            Some(min) if min == max => Some(format!("Bytes<exact={max}>")),
            Some(min) => Some(format!("Bytes<min={min},max={max}>")),
            None => Some(format!("Bytes<max={max}>")),
        },
        CoreType::Record { fields } => {
            let mut rendered = Vec::with_capacity(fields.len());
            for (name, ty) in fields {
                if !is_core_field_name(name) {
                    return None;
                }
                rendered.push(format!(
                    "{name}:{}",
                    canonical_core_type_reference_at_depth(ty, depth + 1)?
                ));
            }
            Some(format!("Record<{}>", rendered.join(",")))
        }
        CoreType::Option { item } => Some(format!(
            "Option<{}>",
            canonical_core_type_reference_at_depth(item, depth + 1)?
        )),
        CoreType::List { item, max } => Some(format!(
            "List<{},max={max}>",
            canonical_core_type_reference_at_depth(item, depth + 1)?
        )),
        CoreType::Map { key, value, max } => Some(format!(
            "Map<{},{},max={max}>",
            canonical_core_type_reference_at_depth(key, depth + 1)?,
            canonical_core_type_reference_at_depth(value, depth + 1)?
        )),
        CoreType::CapabilityRef { item } => Some(format!(
            "CapabilityRef<{}>",
            canonical_core_type_reference_at_depth(item, depth + 1)?
        )),
        CoreType::ExternalActionRequest { settlement } => Some(format!(
            "edict.external-action.request/v1<{}>",
            canonical_core_type_reference_at_depth(settlement, depth + 1)?
        )),
        CoreType::Int { .. }
        | CoreType::String { .. }
        | CoreType::Bytes { .. }
        | CoreType::Nominal { .. }
        | CoreType::Variant { .. } => None,
    }
}

fn canonical_core_type_reference_at_depth(reference: &str, depth: usize) -> Option<String> {
    // Classification accepts only canonical spellings and validates every child
    // at this occurrence depth. Rendering that parsed subtree again repeats the
    // same recursive validation at each parent without changing any bytes.
    classify_core_type_reference_at_depth(reference, depth)?;
    Some(reference.to_owned())
}

/// Whether a Core type-table key is a valid named identity.
pub(crate) fn core_type_table_key_is_named(reference: &str) -> bool {
    matches!(
        classify_core_type_reference(reference),
        Some(CoreTypeReference::Named)
    )
}

pub(crate) fn core_type_fits(core: &CoreModule, source: &str, target: &str) -> bool {
    core_type_fits_at_depth(core, source, target, 0)
}

fn core_type_fits_at_depth(core: &CoreModule, source: &str, target: &str, depth: usize) -> bool {
    if depth > MAX_CORE_TYPE_DEPTH {
        return false;
    }
    let (Some(left), Some(right)) = (
        resolved_core_type(core, source),
        resolved_core_type(core, target),
    ) else {
        return false;
    };
    if let Some(fits) = nominal_types_fit(&left, &right) {
        return fits;
    }
    if let Some(fits) = scalar_types_fit(&left, &right) {
        return fits;
    }
    match (&left, &right) {
        (
            CoreType::Record {
                fields: left_fields,
            },
            CoreType::Record {
                fields: right_fields,
            },
        ) => {
            left_fields.keys().eq(right_fields.keys())
                && left_fields.iter().all(|(field, left_type)| {
                    core_type_fits_at_depth(
                        core,
                        left_type,
                        right_fields.get(field).expect("equal field keys"),
                        depth + 1,
                    )
                })
        }
        (CoreType::Variant { cases: left }, CoreType::Variant { cases: right }) => {
            core_variant_identity_matches(core, source, target)
                && left.keys().eq(right.keys())
                && left.iter().all(|(case, left_payload)| {
                    match (
                        left_payload,
                        right.get(case).expect("equal variant case keys"),
                    ) {
                        (None, None) => true,
                        (Some(left), Some(right)) => {
                            core_type_fits_at_depth(core, left, right, depth + 1)
                        }
                        (None, Some(_)) | (Some(_), None) => false,
                    }
                })
        }
        (CoreType::Option { item: left }, CoreType::Option { item: right })
        | (CoreType::CapabilityRef { item: left }, CoreType::CapabilityRef { item: right }) => {
            core_type_fits_at_depth(core, left, right, depth + 1)
        }
        (
            CoreType::ExternalActionRequest { settlement: left },
            CoreType::ExternalActionRequest { settlement: right },
        ) => core_type_fits_at_depth(core, left, right, depth + 1),
        (
            CoreType::List {
                item: left,
                max: left_max,
            },
            CoreType::List {
                item: right,
                max: right_max,
            },
        ) => left_max <= right_max && core_type_fits_at_depth(core, left, right, depth + 1),
        (
            CoreType::Map {
                key: left_key,
                value: left_value,
                max: left_max,
            },
            CoreType::Map {
                key: right_key,
                value: right_value,
                max: right_max,
            },
        ) => {
            left_max <= right_max
                && core_type_fits_at_depth(core, left_key, right_key, depth + 1)
                && core_type_fits_at_depth(core, left_value, right_value, depth + 1)
        }
        _ => false,
    }
}

pub(crate) fn resolved_core_type(core: &CoreModule, coordinate: &str) -> Option<CoreType> {
    match classify_core_type_reference(coordinate)? {
        CoreTypeReference::Intrinsic(ty) | CoreTypeReference::Structural(ty) => Some(ty),
        CoreTypeReference::Named => core
            .types
            .get(coordinate)
            .or_else(|| {
                coordinate
                    .strip_prefix(core.coordinate.as_str())
                    .and_then(|relative| relative.strip_prefix('.'))
                    .and_then(|relative| core.types.get(relative))
            })
            .cloned(),
    }
}

#[cfg(test)]
fn builtin_core_type(coordinate: &str) -> Option<CoreType> {
    match classify_core_type_reference(coordinate)? {
        CoreTypeReference::Intrinsic(ty) | CoreTypeReference::Structural(ty) => Some(ty),
        CoreTypeReference::Named => None,
    }
}

/// Reconstruct the exact reachable named type closure from signature roots.
///
/// Intrinsic and structural references are traversed transparently. Only named
/// definitions enter the returned closure, and every named coordinate must be
/// accepted by the caller-provided authority predicate.
pub(crate) fn named_core_type_closure<'a, I, F>(
    core: &CoreModule,
    roots: I,
    named_coordinate_allowed: F,
) -> Option<BTreeMap<String, CoreType>>
where
    I: IntoIterator<Item = &'a str>,
    F: Fn(&str) -> bool,
{
    let roots = roots.into_iter().collect::<Vec<_>>();
    let mut integrity = CoreTypeIntegrityState::default();
    for root in &roots {
        validate_core_type_reference(core, &mut integrity, root, "typeClosure.root", 0).ok()?;
    }

    // Depth validity is complete above. This pass computes only the reachable
    // named set, so closure membership is a context-free reachability cache.
    let mut closure = BTreeMap::new();
    let mut visiting = std::collections::BTreeSet::new();
    for root in roots {
        collect_named_core_type_reference(
            core,
            root,
            &named_coordinate_allowed,
            &mut closure,
            &mut visiting,
        )?;
    }
    Some(closure)
}

fn collect_named_core_type_reference<F>(
    core: &CoreModule,
    reference: &str,
    named_coordinate_allowed: &F,
    closure: &mut BTreeMap<String, CoreType>,
    visiting: &mut std::collections::BTreeSet<String>,
) -> Option<()>
where
    F: Fn(&str) -> bool,
{
    match classify_core_type_reference(reference)? {
        CoreTypeReference::Intrinsic(ty) | CoreTypeReference::Structural(ty) => {
            collect_named_core_type_children(core, &ty, named_coordinate_allowed, closure, visiting)
        }
        CoreTypeReference::Named => {
            let table_key = resolved_named_core_type_table_key(core, reference)?;
            if closure.contains_key(table_key) {
                return Some(());
            }
            if !named_coordinate_allowed(table_key) || !visiting.insert(table_key.to_owned()) {
                return None;
            }
            let definition = core.types.get(table_key)?.clone();
            if matches!(
                &definition,
                CoreType::Nominal { contract, .. } if contract != table_key
            ) {
                return None;
            }
            collect_named_core_type_children(
                core,
                &definition,
                named_coordinate_allowed,
                closure,
                visiting,
            )?;
            visiting.remove(table_key);
            closure.insert(table_key.to_owned(), definition);
            Some(())
        }
    }
}

fn collect_named_core_type_children<F>(
    core: &CoreModule,
    ty: &CoreType,
    named_coordinate_allowed: &F,
    closure: &mut BTreeMap<String, CoreType>,
    visiting: &mut std::collections::BTreeSet<String>,
) -> Option<()>
where
    F: Fn(&str) -> bool,
{
    let mut collect = |reference: &str| {
        collect_named_core_type_reference(
            core,
            reference,
            named_coordinate_allowed,
            closure,
            visiting,
        )
    };
    match ty {
        CoreType::Bool
        | CoreType::Unit
        | CoreType::Int { .. }
        | CoreType::String { .. }
        | CoreType::Bytes { .. } => Some(()),
        CoreType::Nominal { representation, .. } => collect(representation),
        CoreType::Record { fields } => {
            for field in fields.values() {
                collect(field)?;
            }
            Some(())
        }
        CoreType::Variant { cases } => {
            for payload in cases.values().flatten() {
                collect(payload)?;
            }
            Some(())
        }
        CoreType::Option { item }
        | CoreType::List { item, .. }
        | CoreType::CapabilityRef { item } => collect(item),
        CoreType::Map { key, value, .. } => {
            collect(key)?;
            collect(value)
        }
        CoreType::ExternalActionRequest { settlement } => collect(settlement),
    }
}

/// Validate Core type integrity and bound executable graph descent.
///
/// This judgment is deliberately eager: valid unused named definitions remain
/// legal and hash-significant, while malformed unused definitions still reject
/// because they are part of the module's semantic preimage.
/// Expression, predicate and nested-block edges share a separate depth budget
/// so switching node families cannot bypass the executable graph limit.
///
/// # Errors
///
/// Returns one deterministic structured failure for the first invalid table
/// key, definition, reference, cycle, or depth boundary in canonical map and
/// source order.
pub fn validate_core_module_type_integrity(
    module: &CoreModule,
) -> Result<ValidatedCoreModule<'_>, CoreTypeIntegrityFailure> {
    for key in module.types.keys() {
        if !core_type_table_key_is_named(key) {
            return Err(CoreTypeIntegrityFailure::new(
                CoreTypeIntegrityFailureKind::InvalidTableKey,
                format!("types.{key}"),
            ));
        }
    }

    let mut state = CoreTypeIntegrityState::default();
    for key in module.types.keys() {
        validate_named_core_type_definition(module, &mut state, key, &format!("types.{key}"), 0)?;
    }
    source_functions::validate_functions(module, &mut state)?;
    for (intent_name, intent) in &module.intents {
        validate_core_intent_types(
            module,
            &mut state,
            intent,
            &format!("intents.{intent_name}"),
        )?;
    }

    Ok(ValidatedCoreModule { module })
}

#[derive(Default)]
struct CoreTypeIntegrityState {
    /// Context-free theorem keyed by the resolved `core.types` table identity.
    named_expansion_height: BTreeMap<String, usize>,
    /// Separate cycle judgment; partial or failed heights are never cached.
    visiting_named: BTreeSet<String>,
    /// Non-memoized structural descent guard while constructing a first summary.
    expanding_depth: usize,
}

fn validate_named_core_type_definition(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    table_key: &str,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_expansion_at_occurrence(
        named_core_type_expansion_height(module, state, table_key, path),
        depth,
        path,
    )
}

fn named_core_type_expansion_height(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    table_key: &str,
    path: &str,
) -> Result<usize, CoreTypeIntegrityFailure> {
    if let Some(height) = state.named_expansion_height.get(table_key) {
        validate_core_type_height_at_depth(*height, state.expanding_depth, path)?;
        return Ok(*height);
    }
    if !state.visiting_named.insert(table_key.to_owned()) {
        return Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::ReferenceCycle,
            path,
        ));
    }

    let height = module
        .types
        .get(table_key)
        .ok_or_else(|| {
            CoreTypeIntegrityFailure::new(
                CoreTypeIntegrityFailureKind::UnresolvedNamedReference,
                path,
            )
        })
        .and_then(|definition| {
            core_type_definition_expansion_height(module, state, Some(table_key), definition, path)
        });
    state.visiting_named.remove(table_key);
    let height = height.and_then(|height| {
        validate_core_type_height_at_depth(height, state.expanding_depth, path)?;
        Ok(height)
    });
    if let Ok(height) = height {
        state
            .named_expansion_height
            .insert(table_key.to_owned(), height);
    }
    height
}

fn core_type_definition_expansion_height(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    table_key: Option<&str>,
    definition: &CoreType,
    path: &str,
) -> Result<usize, CoreTypeIntegrityFailure> {
    match definition {
        CoreType::Bool | CoreType::Unit => Ok(0),
        CoreType::Int { width }
            if matches!(
                width.as_str(),
                "I8" | "I16" | "I32" | "I64" | "U8" | "U16" | "U32" | "U64"
            ) =>
        {
            Ok(0)
        }
        CoreType::Int { .. } => Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidDefinition,
            format!("{path}.width"),
        )),
        CoreType::String { canonical, .. }
            if matches!(canonical.as_str(), "raw-utf8" | "unicode-scalar-nfc") =>
        {
            Ok(0)
        }
        CoreType::String { .. } => Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidDefinition,
            format!("{path}.canonical"),
        )),
        CoreType::Bytes { min, max } if min.is_none_or(|min| min <= *max) => Ok(0),
        CoreType::Bytes { .. } => Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidDefinition,
            format!("{path}.bounds"),
        )),
        CoreType::Nominal {
            contract,
            representation,
        } => {
            if table_key != Some(contract.as_str()) {
                return Err(CoreTypeIntegrityFailure::new(
                    CoreTypeIntegrityFailureKind::NominalContractMismatch,
                    format!("{path}.contract"),
                ));
            }
            let child_height = core_structural_child_expansion_height(
                module,
                state,
                representation,
                &format!("{path}.representation"),
            )?;
            structural_parent_height(child_height, path)
        }
        CoreType::Record { fields } => core_record_expansion_height(module, state, fields, path),
        CoreType::Variant { cases } => core_variant_expansion_height(module, state, cases, path),
        CoreType::Option { item }
        | CoreType::List { item, .. }
        | CoreType::CapabilityRef { item } => {
            let child_height = core_structural_child_expansion_height(
                module,
                state,
                item,
                &format!("{path}.item"),
            )?;
            structural_parent_height(child_height, path)
        }
        CoreType::Map { key, value, .. } => {
            let key_height =
                core_structural_child_expansion_height(module, state, key, &format!("{path}.key"))?;
            let value_height = core_structural_child_expansion_height(
                module,
                state,
                value,
                &format!("{path}.value"),
            )?;
            structural_parent_height(key_height.max(value_height), path)
        }
        CoreType::ExternalActionRequest { settlement } => {
            let child_height = core_structural_child_expansion_height(
                module,
                state,
                settlement,
                &format!("{path}.settlement"),
            )?;
            structural_parent_height(child_height, path)
        }
    }
}

fn core_record_expansion_height(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    fields: &BTreeMap<String, String>,
    path: &str,
) -> Result<usize, CoreTypeIntegrityFailure> {
    let mut maximum_child_height = None;
    for (field, reference) in fields {
        if !is_core_field_name(field) {
            return Err(CoreTypeIntegrityFailure::new(
                CoreTypeIntegrityFailureKind::InvalidDefinition,
                format!("{path}.fields.{field}"),
            ));
        }
        let child_height = core_structural_child_expansion_height(
            module,
            state,
            reference,
            &format!("{path}.fields.{field}"),
        )?;
        maximum_child_height = Some(
            maximum_child_height.map_or(child_height, |height: usize| height.max(child_height)),
        );
    }
    maximum_child_height.map_or(Ok(0), |height| structural_parent_height(height, path))
}

fn core_variant_expansion_height(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    cases: &BTreeMap<String, Option<String>>,
    path: &str,
) -> Result<usize, CoreTypeIntegrityFailure> {
    if cases.is_empty() {
        return Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidDefinition,
            format!("{path}.cases"),
        ));
    }
    let mut maximum_payload_height = None;
    for (case, payload) in cases {
        if let Some(reference) = payload {
            let payload_height = core_structural_child_expansion_height(
                module,
                state,
                reference,
                &format!("{path}.cases.{case}.payload"),
            )?;
            maximum_payload_height = Some(
                maximum_payload_height
                    .map_or(payload_height, |height: usize| height.max(payload_height)),
            );
        }
    }
    maximum_payload_height.map_or(Ok(0), |height| structural_parent_height(height, path))
}

fn validate_core_type_reference(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    reference: &str,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_expansion_at_occurrence(
        core_type_reference_expansion_height(module, state, reference, path),
        depth,
        path,
    )
}

fn core_type_reference_expansion_height(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    reference: &str,
    path: &str,
) -> Result<usize, CoreTypeIntegrityFailure> {
    let Some(classification) = classify_core_type_reference(reference) else {
        let kind = if structural_reference_exceeds_depth(reference, 0) {
            CoreTypeIntegrityFailureKind::DepthExceeded
        } else {
            CoreTypeIntegrityFailureKind::InvalidReference
        };
        return Err(CoreTypeIntegrityFailure::new(kind, path));
    };
    match classification {
        CoreTypeReference::Intrinsic(definition) | CoreTypeReference::Structural(definition) => {
            core_type_definition_expansion_height(module, state, None, &definition, path)
        }
        CoreTypeReference::Named => {
            let table_key =
                resolved_named_core_type_table_key(module, reference).ok_or_else(|| {
                    CoreTypeIntegrityFailure::new(
                        CoreTypeIntegrityFailureKind::UnresolvedNamedReference,
                        path,
                    )
                })?;
            named_core_type_expansion_height(module, state, table_key, path)
        }
    }
}

/// A structural child consumes one edge; expanding a name consumes none.
/// Restore traversal state on failure as well as success. Only the independent
/// height theorem, never this descent budget, is retained in the named cache.
fn core_structural_child_expansion_height(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    reference: &str,
    path: &str,
) -> Result<usize, CoreTypeIntegrityFailure> {
    let child_depth = structural_parent_height(state.expanding_depth, path)?;
    validate_core_type_height_at_depth(0, child_depth, path)?;
    let parent_depth = state.expanding_depth;
    state.expanding_depth = child_depth;
    let height = core_type_reference_expansion_height(module, state, reference, path);
    state.expanding_depth = parent_depth;
    height
}

fn validate_core_expansion_at_occurrence(
    height: Result<usize, CoreTypeIntegrityFailure>,
    depth: usize,
    path: &str,
) -> Result<(), CoreTypeIntegrityFailure> {
    height
        .and_then(|height| validate_core_type_height_at_depth(height, depth, path))
        .map_err(|failure| {
            if failure.kind() == CoreTypeIntegrityFailureKind::DepthExceeded {
                // Identify the checked occurrence regardless of cache history.
                CoreTypeIntegrityFailure::new(CoreTypeIntegrityFailureKind::DepthExceeded, path)
            } else {
                failure
            }
        })
}

fn structural_parent_height(
    child_height: usize,
    path: &str,
) -> Result<usize, CoreTypeIntegrityFailure> {
    child_height.checked_add(1).ok_or_else(|| {
        CoreTypeIntegrityFailure::new(CoreTypeIntegrityFailureKind::DepthExceeded, path)
    })
}

fn validate_core_type_height_at_depth(
    expansion_height: usize,
    starting_depth: usize,
    path: &str,
) -> Result<(), CoreTypeIntegrityFailure> {
    if starting_depth
        .checked_add(expansion_height)
        .is_some_and(|depth| depth <= MAX_CORE_TYPE_DEPTH)
    {
        Ok(())
    } else {
        Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::DepthExceeded,
            path,
        ))
    }
}

fn structural_reference_exceeds_depth(reference: &str, starting_depth: usize) -> bool {
    let mut depth = starting_depth;
    for character in reference.chars() {
        match character {
            '<' => {
                let Some(next) = depth.checked_add(1) else {
                    return true;
                };
                depth = next;
                if depth > MAX_CORE_TYPE_DEPTH {
                    return true;
                }
            }
            '>' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

fn resolved_named_core_type_table_key<'a>(
    module: &'a CoreModule,
    reference: &str,
) -> Option<&'a str> {
    module
        .types
        .get_key_value(reference)
        .map(|(key, _)| key.as_str())
        .or_else(|| {
            reference
                .strip_prefix(module.coordinate.as_str())
                .and_then(|relative| relative.strip_prefix('.'))
                .and_then(|relative| module.types.get_key_value(relative))
                .map(|(key, _)| key.as_str())
        })
}

fn validate_core_intent_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    intent: &CoreIntent,
    path: &str,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_type_reference(module, state, &intent.input, &format!("{path}.input"), 0)?;
    validate_core_type_reference(module, state, &intent.output, &format!("{path}.output"), 0)?;
    if let Some(basis) = &intent.basis {
        validate_core_expression_types(module, state, basis, &format!("{path}.basis"), 0)?;
    }
    for (index, constraint) in intent.input_constraints.iter().enumerate() {
        validate_core_predicate_types(
            module,
            state,
            &constraint.predicate,
            &format!("{path}.inputConstraints[{index}].predicate"),
            0,
        )?;
    }
    validate_core_block_types(module, state, &intent.body, &format!("{path}.body"), 0)
}

fn validate_core_graph_depth(depth: usize, path: &str) -> Result<(), CoreTypeIntegrityFailure> {
    if depth > MAX_CORE_GRAPH_DEPTH {
        Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::DepthExceeded,
            path,
        ))
    } else {
        Ok(())
    }
}

fn validate_core_block_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    block: &CoreBlock,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_graph_depth(depth, path)?;
    for (index, local) in block.locals.iter().enumerate() {
        validate_core_local_type(
            module,
            state,
            local,
            &format!("{path}.locals[{index}].type"),
        )?;
    }
    for (index, node) in block.nodes.iter().enumerate() {
        validate_core_node_types(
            module,
            state,
            node,
            &format!("{path}.nodes[{index}]"),
            depth,
        )?;
    }
    validate_core_expression_types(
        module,
        state,
        &block.result,
        &format!("{path}.result"),
        depth,
    )
}

fn validate_core_local_type(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    local: &LocalRef,
    path: &str,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_type_reference(module, state, &local.ty, path, 0)
}

fn validate_core_node_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    node: &CoreNode,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    match node {
        CoreNode::Let { binding, value } => {
            validate_core_local_type(module, state, binding, &format!("{path}.let.binding.type"))?;
            validate_core_expression_types(
                module,
                state,
                value,
                &format!("{path}.let.value"),
                depth,
            )
        }
        CoreNode::Require { predicate, arm } => {
            validate_core_predicate_types(
                module,
                state,
                predicate,
                &format!("{path}.require.predicate"),
                depth,
            )?;
            let reason = match arm {
                CoreRequireFailureArm::Terminal { reason }
                | CoreRequireFailureArm::ContinueObstructed { reason } => reason,
            };
            validate_core_obstruction_reason_types(
                module,
                state,
                reason,
                &format!("{path}.require.reason"),
                depth,
            )
        }
        CoreNode::Effect { .. } => validate_core_effect_types(module, state, node, path, depth),
        CoreNode::ExternalActionRequest { .. } => {
            validate_external_action_request_types(module, state, node, path, depth)
        }
        CoreNode::For {
            binder, iter, body, ..
        } => {
            validate_core_local_type(module, state, binder, &format!("{path}.for.binder.type"))?;
            validate_core_expression_types(
                module,
                state,
                iter,
                &format!("{path}.for.iter"),
                depth,
            )?;
            validate_core_block_types(module, state, body, &format!("{path}.for.body"), depth + 1)
        }
        CoreNode::Branch {
            binding,
            predicate,
            then_block,
            else_block,
        } => {
            if let Some(binding) = binding {
                validate_core_local_type(
                    module,
                    state,
                    binding,
                    &format!("{path}.branch.binding.type"),
                )?;
            }
            validate_core_predicate_types(
                module,
                state,
                predicate,
                &format!("{path}.branch.predicate"),
                depth,
            )?;
            validate_core_block_types(
                module,
                state,
                then_block,
                &format!("{path}.branch.then"),
                depth + 1,
            )?;
            validate_core_block_types(
                module,
                state,
                else_block,
                &format!("{path}.branch.else"),
                depth + 1,
            )
        }
    }
}

fn validate_core_effect_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    node: &CoreNode,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    let CoreNode::Effect {
        binding,
        input,
        obstruction_map,
        ..
    } = node
    else {
        unreachable!("caller selects the effect variant");
    };
    validate_core_local_type(
        module,
        state,
        binding,
        &format!("{path}.effect.binding.type"),
    )?;
    validate_core_expression_types(module, state, input, &format!("{path}.effect.input"), depth)?;
    for (failure, arm) in obstruction_map {
        validate_core_local_type(
            module,
            state,
            &arm.binder,
            &format!("{path}.effect.obstructionMap.{failure}.binder.type"),
        )?;
        validate_core_expression_types(
            module,
            state,
            &arm.value,
            &format!("{path}.effect.obstructionMap.{failure}.value"),
            depth,
        )?;
    }
    Ok(())
}

fn validate_external_action_request_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    node: &CoreNode,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    let CoreNode::ExternalActionRequest {
        binding,
        input_type,
        settlement_type,
        input,
        authority_scope,
        basis,
        budget,
        ..
    } = node
    else {
        unreachable!("caller selects the external-action request variant");
    };
    let request_path = format!("{path}.externalActionRequest");
    validate_core_local_type(
        module,
        state,
        binding,
        &format!("{request_path}.binding.type"),
    )?;
    validate_core_type_reference(
        module,
        state,
        input_type,
        &format!("{request_path}.inputType"),
        0,
    )?;
    validate_core_type_reference(
        module,
        state,
        settlement_type,
        &format!("{request_path}.settlementType"),
        0,
    )?;
    for (name, expression) in [
        ("input", input),
        ("authorityScope", authority_scope),
        ("basis", basis),
        ("budget.maxSettlementBytes", &budget.max_settlement_bytes),
        ("budget.maxAttempts", &budget.max_attempts),
    ] {
        validate_core_expression_types(
            module,
            state,
            expression,
            &format!("{request_path}.{name}"),
            depth,
        )?;
    }
    Ok(())
}

fn validate_core_obstruction_reason_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    reason: &CoreObstructionReason,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    for (field, value) in &reason.payload {
        validate_core_expression_types(
            module,
            state,
            value,
            &format!("{path}.payload.{field}"),
            depth,
        )?;
    }
    Ok(())
}

fn validate_core_match_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    scrutinee: &CoreExpr,
    arms: &[CoreMatchArm],
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_expression_types(module, state, scrutinee, path, depth + 1)?;
    let mut cases = BTreeSet::new();
    if arms.is_empty() {
        return Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidDefinition,
            path,
        ));
    }
    for arm in arms {
        if !is_core_field_name(&arm.case) || !cases.insert(&arm.case) {
            return Err(CoreTypeIntegrityFailure::new(
                CoreTypeIntegrityFailureKind::InvalidDefinition,
                path,
            ));
        }
        if let Some(binder) = &arm.binder {
            validate_core_local_type(module, state, binder, path)?;
        }
        validate_core_expression_types(module, state, &arm.body, path, depth + 1)?;
    }
    Ok(())
}

fn validate_core_expression_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    expression: &CoreExpr,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_graph_depth(depth, path)?;
    match expression {
        CoreExpr::Local { reference } => {
            validate_core_local_type(module, state, reference, &format!("{path}.local.type"))
        }
        CoreExpr::Const(_) => Ok(()),
        CoreExpr::Variant { ty, case, payload } => {
            variant_expressions::validate(module, state, ty, case, payload.as_deref(), path, depth)
        }
        CoreExpr::Match { scrutinee, arms } => {
            validate_core_match_types(module, state, scrutinee, arms, path, depth)
        }
        CoreExpr::Record { fields } => {
            for (field, value) in fields {
                validate_core_expression_types(
                    module,
                    state,
                    value,
                    &format!("{path}.record.{field}"),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        CoreExpr::List { values } => {
            for (index, value) in values.iter().enumerate() {
                validate_core_expression_types(
                    module,
                    state,
                    value,
                    &format!("{path}.list[{index}]"),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        CoreExpr::Field { base, .. } => validate_core_expression_types(
            module,
            state,
            base,
            &format!("{path}.field.base"),
            depth + 1,
        ),
        CoreExpr::Call {
            type_args, args, ..
        } => {
            for (index, reference) in type_args.iter().enumerate() {
                validate_core_type_reference(
                    module,
                    state,
                    reference,
                    &format!("{path}.call.typeArgs[{index}]"),
                    0,
                )?;
            }
            for (index, argument) in args.iter().enumerate() {
                validate_core_expression_types(
                    module,
                    state,
                    argument,
                    &format!("{path}.call.args[{index}]"),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            validate_core_predicate_types(
                module,
                state,
                predicate,
                &format!("{path}.if.predicate"),
                depth + 1,
            )?;
            validate_core_expression_types(
                module,
                state,
                then_value,
                &format!("{path}.if.then"),
                depth + 1,
            )?;
            validate_core_expression_types(
                module,
                state,
                else_value,
                &format!("{path}.if.else"),
                depth + 1,
            )
        }
    }
}

fn validate_core_predicate_types(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    predicate: &CorePredicate,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_graph_depth(depth, path)?;
    match predicate {
        CorePredicate::True | CorePredicate::False => Ok(()),
        CorePredicate::Not(value) => {
            validate_core_predicate_types(module, state, value, &format!("{path}.not"), depth + 1)
        }
        CorePredicate::All(values) | CorePredicate::Any(values) => {
            for (index, value) in values.iter().enumerate() {
                validate_core_predicate_types(
                    module,
                    state,
                    value,
                    &format!("{path}.values[{index}]"),
                    depth + 1,
                )?;
            }
            Ok(())
        }
        CorePredicate::Compare { left, right, .. } => {
            validate_core_expression_types(
                module,
                state,
                left,
                &format!("{path}.compare.left"),
                depth + 1,
            )?;
            validate_core_expression_types(
                module,
                state,
                right,
                &format!("{path}.compare.right"),
                depth + 1,
            )
        }
    }
}

fn scalar_types_fit(left: &CoreType, right: &CoreType) -> Option<bool> {
    match (left, right) {
        (CoreType::Bool, CoreType::Bool) | (CoreType::Unit, CoreType::Unit) => Some(true),
        (CoreType::Int { width: left }, CoreType::Int { width: right }) => Some(left == right),
        (
            CoreType::String {
                max: left_max,
                canonical: left_canonical,
            },
            CoreType::String {
                max: right_max,
                canonical: right_canonical,
            },
        ) => Some(left_max <= right_max && left_canonical == right_canonical),
        (
            CoreType::Bytes {
                min: left_min,
                max: left_max,
            },
            CoreType::Bytes {
                min: right_min,
                max: right_max,
            },
        ) => Some(left_min.unwrap_or(0) >= right_min.unwrap_or(0) && left_max <= right_max),
        _ => None,
    }
}

fn nominal_types_fit(left: &CoreType, right: &CoreType) -> Option<bool> {
    match (left, right) {
        (
            CoreType::Nominal {
                contract: left_contract,
                representation: left_representation,
            },
            CoreType::Nominal {
                contract: right_contract,
                representation: right_representation,
            },
        ) => Some(left_contract == right_contract && left_representation == right_representation),
        (CoreType::Nominal { .. }, _) | (_, CoreType::Nominal { .. }) => Some(false),
        _ => None,
    }
}

/// Alpha-stable local reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRef {
    pub id: String,
    pub alpha_name: String,
    pub ty: String,
}

/// Literal Core value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreValue {
    Null,
    Bool(bool),
    Int { width: String, value: String },
    String(String),
    Bytes(Vec<u8>),
}

/// Core expression subset used by initial source-to-Core lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreExpr {
    Local {
        reference: LocalRef,
    },
    Const(CoreValue),
    /// A checked tagged value under the existing variant-expression schema.
    Variant {
        ty: String,
        case: String,
        payload: Option<Box<CoreExpr>>,
    },
    /// Exactly one scrutinee evaluation and one selected arm evaluation.
    Match {
        scrutinee: Box<CoreExpr>,
        arms: Vec<CoreMatchArm>,
    },
    Record {
        fields: BTreeMap<String, CoreExpr>,
    },
    /// Ordered bounded list; its type comes from the declared binding context.
    List {
        values: Vec<CoreExpr>,
    },
    Field {
        base: Box<CoreExpr>,
        field: String,
    },
    Call {
        callee: String,
        type_args: Vec<String>,
        args: Vec<CoreExpr>,
    },
    If {
        predicate: Box<CorePredicate>,
        then_value: Box<CoreExpr>,
        else_value: Box<CoreExpr>,
    },
}

/// An exhaustive case arm with an optional lexically scoped payload reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMatchArm {
    pub case: String,
    pub binder: Option<LocalRef>,
    pub body: CoreExpr,
}

/// Core predicate subset used by initial source-to-Core lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorePredicate {
    True,
    False,
    Not(Box<CorePredicate>),
    All(Vec<CorePredicate>),
    Any(Vec<CorePredicate>),
    Compare {
        op: CompareOp,
        left: CoreExpr,
        right: CoreExpr,
    },
}

#[cfg(test)]
mod structural_type_reference_tests {
    use super::{
        builtin_core_type, named_core_type_closure, render_self_describing_core_type,
        validate_core_module_type_integrity, CoreBlock, CoreBudget, CoreExpr, CoreIntent,
        CoreModule, CoreNode, CorePredicate, CoreType, CoreTypeIntegrityFailureKind, CoreValue,
        LocalRef, CORE_API_VERSION, MAX_CORE_TYPE_DEPTH,
    };
    use std::collections::BTreeMap;

    fn module_with_types(types: BTreeMap<String, CoreType>) -> CoreModule {
        CoreModule {
            functions: BTreeMap::new(),
            api_version: CORE_API_VERSION.to_owned(),
            coordinate: "integrity.test@1".to_owned(),
            imports: Vec::new(),
            types,
            intents: BTreeMap::new(),
            required_core_capabilities: Vec::new(),
        }
    }

    fn nested_options(inner: &str, count: usize) -> String {
        (0..count).fold(inner.to_owned(), |item, _| format!("Option<{item}>"))
    }

    fn shallow_and_deep_named_module(
        shallow: &str,
        deep: &str,
        nested_item_wrappers: usize,
    ) -> CoreModule {
        module_with_types(BTreeMap::from([
            (
                shallow.to_owned(),
                CoreType::Option {
                    item: "U64".to_owned(),
                },
            ),
            (
                deep.to_owned(),
                CoreType::Option {
                    item: nested_options(shallow, nested_item_wrappers),
                },
            ),
        ]))
    }

    fn intent_with_type_occurrences(
        input: String,
        output: String,
        nested_local: Option<String>,
    ) -> CoreIntent {
        let nodes = nested_local.into_iter().map(|ty| CoreNode::Branch {
            binding: None,
            predicate: CorePredicate::True,
            then_block: CoreBlock {
                locals: vec![LocalRef {
                    id: "nested.local".to_owned(),
                    alpha_name: "$nested".to_owned(),
                    ty,
                }],
                nodes: Vec::new(),
                result: CoreExpr::Const(CoreValue::Null),
            },
            else_block: CoreBlock {
                locals: Vec::new(),
                nodes: Vec::new(),
                result: CoreExpr::Const(CoreValue::Null),
            },
        });
        CoreIntent {
            input,
            output,
            required_operation_profile: "integrity.read-only".to_owned(),
            basis: None,
            input_constraints: Vec::new(),
            core_evaluation_budget: CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 1,
                max_output_bytes: 1,
            },
            body: CoreBlock {
                locals: Vec::new(),
                nodes: nodes.collect(),
                result: CoreExpr::Const(CoreValue::Null),
            },
        }
    }

    #[test]
    fn canonical_structural_type_references_round_trip() {
        let cases = [
            ("Unit", CoreType::Unit),
            (
                "String<max=7,canonical=unicode-scalar-nfc>",
                CoreType::String {
                    max: 7,
                    canonical: "unicode-scalar-nfc".to_owned(),
                },
            ),
            (
                "Bytes<min=1,max=3>",
                CoreType::Bytes {
                    min: Some(1),
                    max: 3,
                },
            ),
            (
                "Record<left:String<max=3,canonical=raw-utf8>,right:U64>",
                CoreType::Record {
                    fields: BTreeMap::from([
                        (
                            "left".to_owned(),
                            "String<max=3,canonical=raw-utf8>".to_owned(),
                        ),
                        ("right".to_owned(), "U64".to_owned()),
                    ]),
                },
            ),
            (
                "List<Record<inner:Record<value:U64>,tail:Bool>,max=3>",
                CoreType::List {
                    item: "Record<inner:Record<value:U64>,tail:Bool>".to_owned(),
                    max: 3,
                },
            ),
            (
                "Option<Record<value:U64>>",
                CoreType::Option {
                    item: "Record<value:U64>".to_owned(),
                },
            ),
            (
                "Map<String<max=2,canonical=raw-utf8>,CapabilityRef<U64>,max=4>",
                CoreType::Map {
                    key: "String<max=2,canonical=raw-utf8>".to_owned(),
                    value: "CapabilityRef<U64>".to_owned(),
                    max: 4,
                },
            ),
            (
                "edict.external-action.request/v1<Option<Record<value:U64>>>",
                CoreType::ExternalActionRequest {
                    settlement: "Option<Record<value:U64>>".to_owned(),
                },
            ),
        ];
        for (reference, expected) in cases {
            assert_eq!(
                builtin_core_type(reference),
                Some(expected.clone()),
                "{reference}"
            );
            assert_eq!(
                render_self_describing_core_type(&expected).as_deref(),
                Some(reference),
                "{reference}"
            );
        }

        for malformed in [
            "Record<right:U64,left:Bool>",
            "Record<value:U64,value:Bool>",
            "Record<value:>",
            "Record< value:U64>",
            "Record<value:U64",
            "Record<value:String<max=03,canonical=raw-utf8>>",
            "Bytes<min=3,max=3>",
        ] {
            assert_eq!(builtin_core_type(malformed), None, "{malformed}");
        }

        let at_depth = (0..MAX_CORE_TYPE_DEPTH).fold("U64".to_owned(), |inner, _| {
            format!("Record<value:{inner}>")
        });
        assert_eq!(
            builtin_core_type(&at_depth)
                .as_ref()
                .and_then(render_self_describing_core_type)
                .as_deref(),
            Some(at_depth.as_str())
        );

        let over_depth = format!("Record<value:{at_depth}>");
        assert_eq!(builtin_core_type(&over_depth), None);
    }

    #[test]
    fn whole_module_type_integrity_reports_stable_kinds_and_paths() {
        let over_depth =
            (0..=MAX_CORE_TYPE_DEPTH).fold("U64".to_owned(), |inner, _| format!("Option<{inner}>"));
        let mut cycle = BTreeMap::new();
        cycle.insert(
            "CycleA".to_owned(),
            CoreType::Option {
                item: "CycleB".to_owned(),
            },
        );
        cycle.insert(
            "CycleB".to_owned(),
            CoreType::Option {
                item: "CycleA".to_owned(),
            },
        );
        let cases = [
            (
                "table key",
                module_with_types(BTreeMap::from([("Bool".to_owned(), CoreType::Bool)])),
                CoreTypeIntegrityFailureKind::InvalidTableKey,
                "types.Bool",
            ),
            (
                "definition",
                module_with_types(BTreeMap::from([(
                    "Wide".to_owned(),
                    CoreType::Int {
                        width: "I128".to_owned(),
                    },
                )])),
                CoreTypeIntegrityFailureKind::InvalidDefinition,
                "types.Wide.width",
            ),
            (
                "reference",
                module_with_types(BTreeMap::from([(
                    "Bad".to_owned(),
                    CoreType::Option {
                        item: "List<U64,max=01>".to_owned(),
                    },
                )])),
                CoreTypeIntegrityFailureKind::InvalidReference,
                "types.Bad.item",
            ),
            (
                "unresolved",
                module_with_types(BTreeMap::from([(
                    "Bad".to_owned(),
                    CoreType::Option {
                        item: "Missing".to_owned(),
                    },
                )])),
                CoreTypeIntegrityFailureKind::UnresolvedNamedReference,
                "types.Bad.item",
            ),
            (
                "nominal",
                module_with_types(BTreeMap::from([(
                    "Handle".to_owned(),
                    CoreType::Nominal {
                        contract: "Other".to_owned(),
                        representation: "U64".to_owned(),
                    },
                )])),
                CoreTypeIntegrityFailureKind::NominalContractMismatch,
                "types.Handle.contract",
            ),
            (
                "cycle",
                module_with_types(cycle),
                CoreTypeIntegrityFailureKind::ReferenceCycle,
                "types.CycleA.item.item",
            ),
            (
                "depth",
                module_with_types(BTreeMap::from([(
                    "Deep".to_owned(),
                    CoreType::Option { item: over_depth },
                )])),
                CoreTypeIntegrityFailureKind::DepthExceeded,
                "types.Deep",
            ),
        ];

        for (case, module, expected_kind, expected_path) in cases {
            let failure = validate_core_module_type_integrity(&module)
                .expect_err("invalid module must not mint a witness");
            assert_eq!(failure.kind(), expected_kind, "{case}");
            assert_eq!(failure.path(), expected_path, "{case}");
        }
    }

    #[test]
    fn valid_unused_named_definition_mints_a_borrowed_witness() {
        let module = module_with_types(BTreeMap::from([(
            "Unused".to_owned(),
            CoreType::List {
                item: "U64".to_owned(),
                max: 4,
            },
        )]));

        let validated = validate_core_module_type_integrity(&module)
            .expect("valid unused authored definition remains legal");
        assert!(std::ptr::eq(validated.module(), &raw const module));
    }

    #[test]
    fn named_type_depth_is_independent_of_table_order() {
        let cases = [
            (
                "shallow definition sorts first",
                shallow_and_deep_named_module("A", "Z", 127),
                "types.Z",
            ),
            (
                "deep definition sorts first",
                shallow_and_deep_named_module("Z", "A", 127),
                "types.A",
            ),
        ];

        let kinds = cases.each_ref().map(|(_, module, _)| {
            validate_core_module_type_integrity(module)
                .err()
                .map(|failure| failure.kind())
        });
        assert_eq!(
            kinds,
            [Some(CoreTypeIntegrityFailureKind::DepthExceeded); 2]
        );

        for (case, module, expected_path) in cases {
            let failure = validate_core_module_type_integrity(&module)
                .expect_err("depth 129 must reject independently of lexical key order");
            assert_eq!(
                failure.kind(),
                CoreTypeIntegrityFailureKind::DepthExceeded,
                "{case}"
            );
            assert_eq!(failure.path(), expected_path, "{case}");
        }
    }

    #[test]
    fn named_type_depth_accepts_exact_boundary() {
        let exactly_at_limit = shallow_and_deep_named_module("A", "Z", 126);
        validate_core_module_type_integrity(&exactly_at_limit)
            .expect("127 wrappers around a height-one name place U64 at depth 128");

        let one_beyond_limit = shallow_and_deep_named_module("A", "Z", 127);
        let failure = validate_core_module_type_integrity(&one_beyond_limit)
            .expect_err("128 wrappers around a height-one name place U64 at depth 129");
        assert_eq!(failure.kind(), CoreTypeIntegrityFailureKind::DepthExceeded);
        assert_eq!(failure.path(), "types.Z");
    }

    #[test]
    fn named_type_depth_checks_every_occurrence() {
        let mut graph_carried = module_with_types(BTreeMap::from([(
            "A".to_owned(),
            CoreType::Option {
                item: "U64".to_owned(),
            },
        )]));
        graph_carried.intents.insert(
            "depth".to_owned(),
            intent_with_type_occurrences(
                "A".to_owned(),
                nested_options("A", 127),
                Some(nested_options("A", 128)),
            ),
        );
        let failure = validate_core_module_type_integrity(&graph_carried)
            .expect_err("a prior shallow and exact use must not bless a deeper occurrence");
        assert_eq!(failure.kind(), CoreTypeIntegrityFailureKind::DepthExceeded);
        assert_eq!(
            failure.path(),
            "intents.depth.body.nodes[0].branch.then.locals[0].type"
        );
    }

    #[test]
    fn named_type_depth_uses_resolved_table_identity() {
        let mut exactly_at_limit = module_with_types(BTreeMap::from([(
            "A".to_owned(),
            CoreType::Option {
                item: "U64".to_owned(),
            },
        )]));
        exactly_at_limit.intents.insert(
            "depth".to_owned(),
            intent_with_type_occurrences(
                nested_options("A", 127),
                nested_options("integrity.test@1.A", 127),
                None,
            ),
        );
        validate_core_module_type_integrity(&exactly_at_limit)
            .expect("both spellings share one exactly-at-limit expansion summary");

        for spelling in ["A", "integrity.test@1.A"] {
            let mut one_beyond_limit = exactly_at_limit.clone();
            one_beyond_limit
                .intents
                .get_mut("depth")
                .expect("depth intent")
                .body = intent_with_type_occurrences(
                "U64".to_owned(),
                "U64".to_owned(),
                Some(nested_options(spelling, 128)),
            )
            .body;
            let failure = validate_core_module_type_integrity(&one_beyond_limit)
                .expect_err("both spellings share the same over-depth expansion summary");
            assert_eq!(
                failure.kind(),
                CoreTypeIntegrityFailureKind::DepthExceeded,
                "{spelling}"
            );
            assert_eq!(
                failure.path(),
                "intents.depth.body.nodes[0].branch.then.locals[0].type",
                "{spelling}"
            );
        }
    }

    #[test]
    fn named_type_cycles_remain_distinct_from_depth_exhaustion() {
        let module = module_with_types(BTreeMap::from([
            (
                "CycleA".to_owned(),
                CoreType::Option {
                    item: "CycleB".to_owned(),
                },
            ),
            (
                "CycleB".to_owned(),
                CoreType::Option {
                    item: "CycleA".to_owned(),
                },
            ),
        ]));

        let failure = validate_core_module_type_integrity(&module)
            .expect_err("a shallow cycle must not become a cached success or depth error");
        assert_eq!(failure.kind(), CoreTypeIntegrityFailureKind::ReferenceCycle);
        assert_eq!(failure.path(), "types.CycleA.item.item");
    }

    #[test]
    fn named_type_closure_is_independent_of_root_order() {
        let module = shallow_and_deep_named_module("A", "Z", 126);
        let shallow_first = named_core_type_closure(&module, ["A", "Z"], |_| true)
            .expect("the exactly-at-limit closure is valid");
        let deep_first = named_core_type_closure(&module, ["Z", "A"], |_| true)
            .expect("root order cannot change an exactly-at-limit verdict");

        assert_eq!(shallow_first, deep_first);
        assert_eq!(
            shallow_first.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["A", "Z"]
        );

        let over_limit = shallow_and_deep_named_module("A", "Z", 127);
        for roots in [["A", "Z"], ["Z", "A"]] {
            assert_eq!(named_core_type_closure(&over_limit, roots, |_| true), None);
        }

        // Qualification changes spelling, not the authenticated table identity.
        assert_eq!(
            named_core_type_closure(&module, ["integrity.test@1.A", "Z"], |_| true),
            Some(shallow_first)
        );
        assert_eq!(
            named_core_type_closure(&module, ["integrity.test@1.A"], |key| key != "A"),
            None
        );
    }

    #[test]
    fn named_type_expansion_bounds_recursive_descent() {
        for count in [128, 129, 4096] {
            let module = module_with_types(
                (0..count)
                    .map(|index| {
                        (
                            format!("N{index:04}"),
                            CoreType::Option {
                                item: if index + 1 == count {
                                    "U64".to_owned()
                                } else {
                                    format!("N{:04}", index + 1)
                                },
                            },
                        )
                    })
                    .collect(),
            );
            let failure = validate_core_module_type_integrity(&module).err();
            if count == 128 {
                assert_eq!(failure, None);
            } else {
                let failure = failure.expect("over-depth chains reject before stack exhaustion");
                assert_eq!(failure.kind(), CoreTypeIntegrityFailureKind::DepthExceeded);
                assert_eq!(failure.path(), "types.N0000");
            }
        }
    }

    fn height_two_structural_definitions() -> [(&'static str, CoreType); 7] {
        [
            (
                "option",
                CoreType::Option {
                    item: "A".to_owned(),
                },
            ),
            (
                "list",
                CoreType::List {
                    item: "A".to_owned(),
                    max: 1,
                },
            ),
            (
                "capability",
                CoreType::CapabilityRef {
                    item: "A".to_owned(),
                },
            ),
            (
                "external request",
                CoreType::ExternalActionRequest {
                    settlement: "A".to_owned(),
                },
            ),
            (
                "map",
                CoreType::Map {
                    key: "U64".to_owned(),
                    value: "A".to_owned(),
                    max: 1,
                },
            ),
            (
                "record",
                CoreType::Record {
                    fields: BTreeMap::from([("value".to_owned(), "A".to_owned())]),
                },
            ),
            (
                "variant payload",
                CoreType::Variant {
                    cases: BTreeMap::from([("some".to_owned(), Some("A".to_owned()))]),
                },
            ),
        ]
    }

    fn assert_height_two_definition_boundary(case: &str, definition: &CoreType) {
        for (wrappers, should_succeed) in [(125, true), (126, false)] {
            let module = module_with_types(BTreeMap::from([
                (
                    "A".to_owned(),
                    CoreType::Option {
                        item: "U64".to_owned(),
                    },
                ),
                ("B".to_owned(), definition.clone()),
                (
                    "Z".to_owned(),
                    CoreType::Option {
                        item: nested_options("B", wrappers),
                    },
                ),
            ]));
            let result = validate_core_module_type_integrity(&module);
            if should_succeed {
                result.expect("height-two child at depth 126 ends at depth 128");
            } else {
                assert_eq!(
                    result
                        .expect_err("height-two child at depth 127 ends at depth 129")
                        .kind(),
                    CoreTypeIntegrityFailureKind::DepthExceeded,
                    "{case}"
                );
            }
        }
    }

    fn assert_nominal_height_boundary() {
        let module = module_with_types(BTreeMap::from([
            (
                "A".to_owned(),
                CoreType::Option {
                    item: "U64".to_owned(),
                },
            ),
            (
                "B".to_owned(),
                CoreType::Nominal {
                    contract: "B".to_owned(),
                    representation: "A".to_owned(),
                },
            ),
            (
                "Z".to_owned(),
                CoreType::Option {
                    item: nested_options("B", 126),
                },
            ),
        ]));
        assert_eq!(
            validate_core_module_type_integrity(&module)
                .expect_err("nominal representation consumes one structural edge")
                .kind(),
            CoreTypeIntegrityFailureKind::DepthExceeded
        );
    }

    fn assert_zero_child_height(definition: CoreType) {
        let module = module_with_types(BTreeMap::from([
            ("A".to_owned(), definition),
            (
                "Z".to_owned(),
                CoreType::Option {
                    item: nested_options("A", 127),
                },
            ),
        ]));
        validate_core_module_type_integrity(&module)
            .expect("empty record and payloadless variant have zero child height");
    }

    #[test]
    fn named_type_expansion_height_covers_every_structural_constructor() {
        for (case, definition) in height_two_structural_definitions() {
            assert_height_two_definition_boundary(case, &definition);
        }

        assert_nominal_height_boundary();

        for terminal in [
            CoreType::Record {
                fields: BTreeMap::new(),
            },
            CoreType::Variant {
                cases: BTreeMap::from([("none".to_owned(), None)]),
            },
        ] {
            assert_zero_child_height(terminal);
        }
    }
}

/// Comparison operators in Core predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Source-origin input constraint lowered into Core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputConstraint {
    pub coordinate: String,
    pub source: InputConstraintSource,
    pub predicate: CorePredicate,
}

/// Origin of a Core input constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputConstraintSource {
    Where,
    Compiler,
}

/// Core evaluation budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreBudget {
    pub max_steps: u64,
    pub max_allocated_bytes: u64,
    pub max_output_bytes: u64,
}

/// Core intent shape for the initial lowerer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreIntent {
    pub input: String,
    pub output: String,
    pub required_operation_profile: String,
    /// Authored basis expression. Runtime basis resolution and
    /// admission remain target/runtime responsibilities.
    pub basis: Option<CoreExpr>,
    pub input_constraints: Vec<InputConstraint>,
    pub core_evaluation_budget: CoreBudget,
    pub body: CoreBlock,
}

/// Core block with alpha-stable locals and ordered nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreBlock {
    pub locals: Vec<LocalRef>,
    pub nodes: Vec<CoreNode>,
    pub result: CoreExpr,
}

/// Static maximum carried by bounded Core control flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreBound {
    Literal(u64),
    Coordinate(String),
}

/// Core node subset used by the first source-to-Core slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreNode {
    Let {
        binding: LocalRef,
        value: CoreExpr,
    },
    Require {
        predicate: CorePredicate,
        arm: CoreRequireFailureArm,
    },
    Effect {
        binding: LocalRef,
        effect: String,
        input: CoreExpr,
        obstruction_map: BTreeMap<String, CoreObstructionArm>,
    },
    ExternalActionRequest {
        binding: LocalRef,
        operation: ResourceRef,
        input_type: String,
        settlement_type: String,
        input_schema: ResourceRef,
        settlement_schema: ResourceRef,
        input: CoreExpr,
        authority_scope: Box<CoreExpr>,
        basis: Box<CoreExpr>,
        budget: Box<CoreExternalActionBudget>,
        reconciliation_law: ResourceRef,
    },
    For {
        binder: LocalRef,
        iter: CoreExpr,
        bound: CoreBound,
        body: CoreBlock,
    },
    Branch {
        /// When present, the selected block result becomes this local.
        /// Statement-only branches leave the binding absent.
        binding: Option<LocalRef>,
        predicate: CorePredicate,
        then_block: CoreBlock,
        else_block: CoreBlock,
    },
}

/// Runtime-valued bounds carried by a typed external-action request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreExternalActionBudget {
    pub max_settlement_bytes: CoreExpr,
    pub max_attempts: CoreExpr,
}

/// Core disposition for a failed `require` predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreRequireFailureArm {
    /// Terminal typed obstruction: the success path cannot continue.
    Terminal { reason: CoreObstructionReason },
    /// Preserved obstruction strand: the attempt continues as obstructed
    /// causal support rather than collapsing into terminal obstruction.
    ContinueObstructed { reason: CoreObstructionReason },
}

/// Closed Core reason envelope with opaque canonical payload fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreObstructionReason {
    pub kind: String,
    pub payload: BTreeMap<String, CoreExpr>,
}

/// Core obstruction arm for a semantic effect failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreObstructionArm {
    pub binder: LocalRef,
    pub value: CoreExpr,
}
