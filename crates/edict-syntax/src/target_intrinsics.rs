//! Admission of pinned target-catalog pure intrinsics.
//!
//! A target profile's intrinsic catalog (`edict.target-profile.intrinsics/v2`)
//! declares `intrinsicClass: "pure"` constructors the target owns. A consumer
//! admits exactly the contracts it requires, against a digest it already
//! trusts, and passes the resulting facts to the source compiler and to Target
//! IR lowering. A catalog never authenticates itself: the expected digest comes
//! from the consumer, and unknown, unpinned or mismatched contracts fail closed.

use std::collections::{BTreeMap, BTreeSet};

use crate::canonical::{
    decode_canonical_cbor, digest_canonical_value, sha256_review_string, CanonicalValue,
};
use crate::core_ir::{classify_core_type_reference, CoreType, CoreTypeReference, ResourceRef};

/// Intrinsic catalog document version this crate admits.
pub const TARGET_INTRINSICS_API_VERSION: &str = "edict.target-profile.intrinsics/v2";

const PURE_FIELDS: [&str; 10] = [
    "argumentTypes",
    "contractVersion",
    "costRule",
    "costTemplate",
    "footprintTemplate",
    "guardSupport",
    "intrinsicClass",
    "returnType",
    "typeParameters",
    "writeClass",
];

/// One pure intrinsic a consumer requires, at an exact contract version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredPureIntrinsic {
    pub coordinate: String,
    pub contract_version: String,
}

/// Why a catalog or one of its contracts was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetPureIntrinsicFailureKind {
    /// The target or catalog reference carries no digest to compare against.
    UnpinnedResource,
    /// The catalog bytes are not canonical CBOR.
    NonCanonicalCatalog,
    /// The catalog bytes do not match the trusted pinned digest.
    DigestMismatch,
    /// The catalog document is not a v1 intrinsic catalog.
    MalformedCatalog,
    /// A required coordinate is absent from the catalog.
    MissingIntrinsic,
    /// A required coordinate names an effect intrinsic.
    NotPure,
    /// The contract's version differs from the required version.
    VersionMismatch,
    /// A pure contract has missing, extra or ill-typed fields.
    MalformedContract,
    /// A coordinate is not owned by the target's namespace.
    OutsideNamespace,
    /// A signature names a type the catalog does not define.
    UnresolvedType,
    /// A provider-owned type is not a record of structural field types.
    InvalidProviderType,
    /// The cost rule has the wrong arity, a negative or oversized coefficient,
    /// or charges an argument with no size bound.
    InvalidCostRule,
}

/// One refused obligation, naming the coordinate it concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetPureIntrinsicFailure {
    pub kind: TargetPureIntrinsicFailureKind,
    pub coordinate: String,
}

/// A complete, pinned pure-intrinsic contract.
///
/// Its fields are private: only [`admit_target_pure_intrinsics`] builds one,
/// so a fact always names a contract that was checked against a trusted pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetPureIntrinsicFact {
    target: ResourceRef,
    catalog: ResourceRef,
    coordinate: String,
    contract_version: String,
    parameter_types: Vec<String>,
    return_type: String,
    cost_base: u64,
    cost_per_argument_max: Vec<u64>,
    provider_types: BTreeMap<String, CoreType>,
    provider_definitions: BTreeMap<String, String>,
}

impl TargetPureIntrinsicFact {
    /// Target profile import this intrinsic belongs to.
    #[must_use]
    pub fn target(&self) -> &ResourceRef {
        &self.target
    }

    /// Pinned intrinsic catalog the contract was admitted from.
    #[must_use]
    pub fn catalog(&self) -> &ResourceRef {
        &self.catalog
    }

    /// Canonical intrinsic coordinate, below the target coordinate.
    #[must_use]
    pub fn coordinate(&self) -> &str {
        &self.coordinate
    }

    /// Semantic contract version.
    #[must_use]
    pub fn contract_version(&self) -> &str {
        &self.contract_version
    }

    /// Ceiling type of each argument. Narrower operands are accepted.
    #[must_use]
    pub fn parameter_types(&self) -> &[String] {
        &self.parameter_types
    }

    /// Return type coordinate.
    #[must_use]
    pub fn return_type(&self) -> &str {
        &self.return_type
    }

    /// Fixed steps charged by every call.
    #[must_use]
    pub fn cost_base(&self) -> u64 {
        self.cost_base
    }

    /// Steps charged per unit of each argument's established size bound: the
    /// byte maximum of a `Bytes` argument or the element maximum of a `List`.
    #[must_use]
    pub fn cost_per_argument_max(&self) -> &[u64] {
        &self.cost_per_argument_max
    }

    /// Provider-owned named types reachable from this signature.
    #[must_use]
    pub fn provider_types(&self) -> &BTreeMap<String, CoreType> {
        &self.provider_types
    }

    /// Structural definitions of [`Self::provider_types`], as published.
    #[must_use]
    pub fn provider_definitions(&self) -> &BTreeMap<String, String> {
        &self.provider_definitions
    }

    /// Steps for one call, given each argument's established size bound.
    /// Returns `None` on arity mismatch or overflow.
    #[must_use]
    pub fn steps_for(&self, argument_maxima: &[u64]) -> Option<u64> {
        if argument_maxima.len() != self.cost_per_argument_max.len() {
            return None;
        }
        self.cost_per_argument_max
            .iter()
            .zip(argument_maxima)
            .try_fold(self.cost_base, |steps, (coefficient, maximum)| {
                steps.checked_add(coefficient.checked_mul(*maximum)?)
            })
    }
}

/// Digest of a canonical intrinsic catalog under its catalog coordinate.
///
/// Consumers use this to compute the pin they record; admission compares the
/// supplied bytes against a pin recorded earlier, never against itself.
///
/// # Errors
///
/// Returns `NonCanonicalCatalog` when the bytes are not canonical CBOR.
pub fn digest_target_intrinsics_catalog(
    catalog_coordinate: &str,
    catalog_bytes: &[u8],
) -> Result<String, TargetPureIntrinsicFailure> {
    let value = decode_canonical_cbor(catalog_bytes).map_err(|_| {
        failure(
            TargetPureIntrinsicFailureKind::NonCanonicalCatalog,
            catalog_coordinate,
        )
    })?;
    digest_canonical_value(catalog_coordinate, &value)
        .map(|digest| sha256_review_string(&digest))
        .map_err(|_| {
            failure(
                TargetPureIntrinsicFailureKind::NonCanonicalCatalog,
                catalog_coordinate,
            )
        })
}

/// Admit exactly the required pure intrinsics from a pinned catalog.
///
/// `target` is the target profile import the intrinsics belong to; its
/// coordinate is the namespace every intrinsic and provider type must sit
/// below. `catalog` carries the trusted expected digest of `catalog_bytes`.
/// Admission is all-or-nothing and returns facts in `required` order.
///
/// # Errors
///
/// Returns every refused obligation when a pin is missing or mismatched, the
/// catalog is malformed, or any required contract is absent, effectful,
/// versioned differently, incomplete or ill-typed.
pub fn admit_target_pure_intrinsics(
    target: &ResourceRef,
    catalog: &ResourceRef,
    catalog_bytes: &[u8],
    required: &[RequiredPureIntrinsic],
) -> Result<Vec<TargetPureIntrinsicFact>, Vec<TargetPureIntrinsicFailure>> {
    use TargetPureIntrinsicFailureKind as K;
    for reference in [target, catalog] {
        // The full pin form Target itself requires, not merely its prefix.
        if !reference.is_digest_locked() {
            return Err(vec![failure(K::UnpinnedResource, &reference.coordinate)]);
        }
    }
    let actual = digest_target_intrinsics_catalog(&catalog.coordinate, catalog_bytes)
        .map_err(|refused| vec![refused])?;
    if catalog.digest.as_deref() != Some(actual.as_str()) {
        return Err(vec![failure(K::DigestMismatch, &catalog.coordinate)]);
    }
    let document = decode_canonical_cbor(catalog_bytes)
        .map_err(|_| vec![failure(K::NonCanonicalCatalog, &catalog.coordinate)])?;
    let malformed = || vec![failure(K::MalformedCatalog, &catalog.coordinate)];
    let fields = text_map(&document).ok_or_else(malformed)?;
    let allowed = ["apiVersion", "intrinsics", "types"];
    if fields.keys().any(|key| !allowed.contains(key))
        || fields.get("apiVersion").and_then(|value| text(value))
            != Some(TARGET_INTRINSICS_API_VERSION)
    {
        return Err(malformed());
    }
    let intrinsics = fields
        .get("intrinsics")
        .and_then(|value| text_map(value))
        .ok_or_else(malformed)?;
    let mut failures = Vec::new();
    let provider = provider_types(fields.get("types").copied(), target, &mut failures);
    let mut admitted = Vec::with_capacity(required.len());
    for requirement in required {
        match admit_one(target, catalog, &intrinsics, &provider, requirement) {
            Ok(fact) => admitted.push(fact),
            Err(kind) => failures.push(failure(kind, &requirement.coordinate)),
        }
    }
    if failures.is_empty() {
        Ok(admitted)
    } else {
        failures.sort_by(|left, right| {
            (left.kind, &left.coordinate).cmp(&(right.kind, &right.coordinate))
        });
        failures.dedup();
        Err(failures)
    }
}

/// Provider-owned named types: records whose fields are structural types.
fn provider_types(
    value: Option<&CanonicalValue>,
    target: &ResourceRef,
    failures: &mut Vec<TargetPureIntrinsicFailure>,
) -> BTreeMap<String, (String, CoreType)> {
    use TargetPureIntrinsicFailureKind as K;
    let mut types = BTreeMap::new();
    let Some(value) = value else {
        return types;
    };
    let Some(entries) = text_map(value) else {
        failures.push(failure(K::MalformedCatalog, &target.coordinate));
        return types;
    };
    for (name, definition) in entries {
        if !below(name, &target.coordinate) {
            failures.push(failure(K::OutsideNamespace, name));
            continue;
        }
        let Some(definition) = text(definition) else {
            failures.push(failure(K::InvalidProviderType, name));
            continue;
        };
        match classify_core_type_reference(definition) {
            Some(CoreTypeReference::Structural(record @ CoreType::Record { .. }))
                if record_fields_are_structural(&record) =>
            {
                types.insert(name.to_owned(), (definition.to_owned(), record));
            }
            _ => failures.push(failure(K::InvalidProviderType, name)),
        }
    }
    types
}

fn record_fields_are_structural(record: &CoreType) -> bool {
    let CoreType::Record { fields } = record else {
        return false;
    };
    !fields.is_empty() && fields.values().all(|field| structurally_closed(field))
}

/// A type reference that names no type at any depth: intrinsic, or structural
/// with every child structurally closed. Classification already bounds the
/// nesting depth.
fn structurally_closed(reference: &str) -> bool {
    match classify_core_type_reference(reference) {
        Some(CoreTypeReference::Intrinsic(_)) => true,
        Some(CoreTypeReference::Structural(core_type)) => child_references(&core_type)
            .iter()
            .all(|child| structurally_closed(child)),
        _ => false,
    }
}

/// Every type reference a structural type contains.
fn child_references(core_type: &CoreType) -> Vec<String> {
    match core_type {
        CoreType::Bool
        | CoreType::Unit
        | CoreType::Int { .. }
        | CoreType::String { .. }
        | CoreType::Bytes { .. } => Vec::new(),
        CoreType::Nominal { representation, .. } => vec![representation.clone()],
        CoreType::Record { fields } => fields.values().cloned().collect(),
        CoreType::Variant { cases } => cases.values().flatten().cloned().collect(),
        CoreType::Option { item }
        | CoreType::List { item, .. }
        | CoreType::CapabilityRef { item } => vec![item.clone()],
        CoreType::Map { key, value, .. } => vec![key.clone(), value.clone()],
        CoreType::ExternalActionRequest { settlement } => vec![settlement.clone()],
    }
}

fn admit_one(
    target: &ResourceRef,
    catalog: &ResourceRef,
    intrinsics: &BTreeMap<&str, &CanonicalValue>,
    provider: &BTreeMap<String, (String, CoreType)>,
    requirement: &RequiredPureIntrinsic,
) -> Result<TargetPureIntrinsicFact, TargetPureIntrinsicFailureKind> {
    use TargetPureIntrinsicFailureKind as K;
    if !below(&requirement.coordinate, &target.coordinate) {
        return Err(K::OutsideNamespace);
    }
    let entry = intrinsics
        .get(requirement.coordinate.as_str())
        .ok_or(K::MissingIntrinsic)?;
    let fields = text_map(entry).ok_or(K::MalformedContract)?;
    if fields.get("intrinsicClass").and_then(|value| text(value)) != Some("pure") {
        return Err(K::NotPure);
    }
    if fields.len() != PURE_FIELDS.len() || PURE_FIELDS.iter().any(|key| !fields.contains_key(key))
    {
        return Err(K::MalformedContract);
    }
    let version = fields
        .get("contractVersion")
        .and_then(|value| text(value))
        .filter(|version| !version.is_empty())
        .ok_or(K::MalformedContract)?;
    if version != requirement.contract_version {
        return Err(K::VersionMismatch);
    }
    let texts = |key: &str| -> Option<Vec<String>> {
        let CanonicalValue::Array(values) = fields.get(key)? else {
            return None;
        };
        values
            .iter()
            .map(|value| text(value).map(str::to_owned))
            .collect()
    };
    let parameter_types = texts("argumentTypes").ok_or(K::MalformedContract)?;
    let return_type = fields
        .get("returnType")
        .and_then(|value| text(value))
        .ok_or(K::MalformedContract)?
        .to_owned();
    let nonempty_text = |key: &str| {
        fields
            .get(key)
            .and_then(|value| text(value))
            .is_some_and(|value| !value.is_empty())
    };
    if !texts("typeParameters").is_some_and(|parameters| parameters.is_empty())
        || fields.get("guardSupport") != Some(&&CanonicalValue::Bool(false))
        || fields.get("writeClass").and_then(|value| text(value)) != Some("none")
        || !nonempty_text("footprintTemplate")
        || !nonempty_text("costTemplate")
    {
        return Err(K::MalformedContract);
    }
    let mut closure = BTreeMap::new();
    let mut definitions = BTreeMap::new();
    let mut sizes = Vec::with_capacity(parameter_types.len());
    for reference in &parameter_types {
        sizes.push(resolve_signature_type(
            reference,
            target,
            provider,
            &mut closure,
            &mut definitions,
        )?);
    }
    resolve_signature_type(
        &return_type,
        target,
        provider,
        &mut closure,
        &mut definitions,
    )?;
    let (cost_base, cost_per_argument_max) =
        cost_rule(fields.get("costRule").copied(), &sizes).ok_or(K::InvalidCostRule)?;
    Ok(TargetPureIntrinsicFact {
        target: target.clone(),
        catalog: catalog.clone(),
        coordinate: requirement.coordinate.clone(),
        contract_version: version.to_owned(),
        parameter_types,
        return_type,
        cost_base,
        cost_per_argument_max,
        provider_types: closure,
        provider_definitions: definitions,
    })
}

/// Resolve one signature type, collecting provider types it reaches. Returns
/// whether the type has a size bound a cost coefficient may charge.
fn resolve_signature_type(
    reference: &str,
    target: &ResourceRef,
    provider: &BTreeMap<String, (String, CoreType)>,
    closure: &mut BTreeMap<String, CoreType>,
    definitions: &mut BTreeMap<String, String>,
) -> Result<bool, TargetPureIntrinsicFailureKind> {
    use TargetPureIntrinsicFailureKind as K;
    match classify_core_type_reference(reference).ok_or(K::UnresolvedType)? {
        CoreTypeReference::Intrinsic(_) => Ok(false),
        CoreTypeReference::Named => {
            if !below(reference, &target.coordinate) {
                return Err(K::OutsideNamespace);
            }
            let (definition, core_type) = provider.get(reference).ok_or(K::UnresolvedType)?;
            closure.insert(reference.to_owned(), core_type.clone());
            definitions.insert(reference.to_owned(), definition.clone());
            // Provider records are structurally closed; walking them anyway
            // keeps admission complete if that rule ever widens.
            for child in child_references(core_type) {
                resolve_signature_type(&child, target, provider, closure, definitions)?;
            }
            Ok(false)
        }
        CoreTypeReference::Structural(core_type) => {
            let sized = matches!(core_type, CoreType::Bytes { .. } | CoreType::List { .. });
            for child in child_references(&core_type) {
                resolve_signature_type(&child, target, provider, closure, definitions)?;
            }
            Ok(sized)
        }
    }
}

fn cost_rule(value: Option<&CanonicalValue>, sized: &[bool]) -> Option<(u64, Vec<u64>)> {
    let fields = text_map(value?)?;
    if fields.len() != 2 {
        return None;
    }
    let base = u64_value(fields.get("base")?)?;
    let CanonicalValue::Array(coefficients) = fields.get("perArgumentMax")? else {
        return None;
    };
    if coefficients.len() != sized.len() {
        return None;
    }
    let coefficients = coefficients
        .iter()
        .zip(sized)
        .map(|(value, sized)| u64_value(value).filter(|coefficient| *sized || *coefficient == 0))
        .collect::<Option<Vec<_>>>()?;
    Some((base, coefficients))
}

fn u64_value(value: &CanonicalValue) -> Option<u64> {
    let CanonicalValue::Integer(value) = value else {
        return None;
    };
    u64::try_from(*value).ok()
}

fn text(value: &CanonicalValue) -> Option<&str> {
    match value {
        CanonicalValue::Text(text) => Some(text),
        _ => None,
    }
}

/// A map whose keys are all distinct text strings.
fn text_map(value: &CanonicalValue) -> Option<BTreeMap<&str, &CanonicalValue>> {
    let CanonicalValue::Map(entries) = value else {
        return None;
    };
    let mut seen = BTreeSet::new();
    let mut map = BTreeMap::new();
    for (key, value) in entries {
        let key = text(key)?;
        if !seen.insert(key) {
            return None;
        }
        map.insert(key, value);
    }
    Some(map)
}

fn below(coordinate: &str, namespace: &str) -> bool {
    coordinate
        .strip_prefix(namespace)
        .and_then(|suffix| suffix.strip_prefix('.'))
        .is_some_and(|suffix| !suffix.is_empty())
}

fn failure(kind: TargetPureIntrinsicFailureKind, coordinate: &str) -> TargetPureIntrinsicFailure {
    TargetPureIntrinsicFailure {
        kind,
        coordinate: coordinate.to_owned(),
    }
}
