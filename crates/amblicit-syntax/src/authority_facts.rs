//! File-backed authority-fact loading for compiler context facts.
//!
//! This module deliberately loads only the first facts already modeled by
//! `CompilerContext`: operation profiles, profile write-class allowances,
//! effect write classes, and budgets. It does not validate full lawpack or
//! target-profile manifests, and it does not implement participant trust
//! policy.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::canonical::{
    decode_canonical_cbor, digest_canonical_value, encode_canonical_cbor, sha256_review_string,
    CanonicalErrorKind, CanonicalValue,
};
use crate::compiler::CompilerContext;
use crate::core_ir::{is_sha256_review_digest, CoreBudget};
use crate::lowerability::WriteClass;

/// Authority-facts document ABI supported by this crate.
pub const AUTHORITY_FACTS_API_VERSION: &str = "edict.authority-facts/v1";

/// Root rule in the Edict-owned authority-facts CDDL contract.
pub const AUTHORITY_FACTS_CDDL_ROOT: &str = "authority-facts";

const AUTHORITY_FACTS_CBOR_PATH: &str = "<authority-facts-cbor>";

/// Kind of digest-bound source that supplied authority facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityFactSourceKind {
    Lawpack,
    TargetProfile,
}

/// Digest-bound authority-fact source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityFactSource {
    pub kind: AuthorityFactSourceKind,
    pub coordinate: String,
    pub digest: String,
}

/// Operation profile fact available to the compiler resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationProfileFact {
    pub source: String,
    pub core: String,
    pub allowed_write_classes: Vec<WriteClass>,
}

/// Effect write-class fact available to compiler profile/effect checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectWriteClassFact {
    pub effect: String,
    pub write_class: WriteClass,
}

/// Budget fact available to the compiler resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetFact {
    pub source: String,
    pub budget: CoreBudget,
}

/// One validated authority-facts document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityFactsDocument {
    pub api_version: String,
    pub source: AuthorityFactSource,
    pub operation_profiles: Vec<OperationProfileFact>,
    pub effect_write_classes: Vec<EffectWriteClassFact>,
    pub budgets: Vec<BudgetFact>,
}

/// Stable authority-facts load failure categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityFactsLoadFailureKind {
    Io,
    InvalidJson,
    InvalidCanonicalCbor,
    InvalidCborShape,
    DuplicateFact,
    InvalidApiVersion,
    InvalidSourceKind,
    MissingCoordinate,
    InvalidCoordinate,
    NonDigestLockedSource,
    InvalidWriteClass,
    ConflictingFact,
}

/// One failed authority-facts loading obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityFactsLoadFailure {
    pub kind: AuthorityFactsLoadFailureKind,
    pub path: String,
    pub field: String,
    pub coordinate: String,
}

/// Load and validate one authority-facts JSON document.
///
/// # Errors
///
/// Returns stable load failures when the file cannot be read, cannot be parsed
/// as the supported JSON shape, lacks digest-bound source identity, or carries
/// duplicate or invalid fact values.
pub fn load_authority_facts_file(
    path: impl AsRef<Path>,
) -> Result<AuthorityFactsDocument, Vec<AuthorityFactsLoadFailure>> {
    let path = path.as_ref();
    let path_display = path.display().to_string();
    let text = fs::read_to_string(path).map_err(|_err| {
        vec![failure(
            AuthorityFactsLoadFailureKind::Io,
            &path_display,
            "file",
            "",
        )]
    })?;
    let raw = serde_json::from_str::<RawAuthorityFactsDocument>(&text).map_err(|_err| {
        vec![failure(
            AuthorityFactsLoadFailureKind::InvalidJson,
            &path_display,
            "json",
            "",
        )]
    })?;
    validate_raw_document(raw, &path_display)
}

/// Encode one authority-facts document as `edict.canonical-cbor/v1`.
///
/// Fact collections are canonical maps keyed by their authority coordinate.
/// Allowed write classes are normalized as a canonical map-set. Source digests use
/// the shared typed `['sha256', 32-byte value]` wire representation rather than
/// their JSON review string.
///
/// # Errors
///
/// Returns stable failures when the document violates the existing semantic
/// contract, repeats a fact coordinate, or cannot be represented by the
/// canonical authority-facts shape.
pub fn encode_authority_facts_cbor(
    document: &AuthorityFactsDocument,
) -> Result<Vec<u8>, Vec<AuthorityFactsLoadFailure>> {
    let path = authority_document_path(document);
    let mut failures = Vec::new();
    validate_document(document, &path, &mut failures);
    validate_unique_document_facts(document, &path, &mut failures);
    if !failures.is_empty() {
        return Err(failures);
    }

    let value = authority_facts_canonical_value(document, &path)?;
    encode_canonical_cbor(&value).map_err(|_err| {
        vec![failure(
            AuthorityFactsLoadFailureKind::InvalidCanonicalCbor,
            &path,
            "canonicalCbor",
            &document.source.coordinate,
        )]
    })
}

/// Compute the domain-framed digest for an authority-facts document.
///
/// The digest is SHA-256 over canonical CBOR for
/// `['edict.digest/v1', 'edict.authority-facts/v1', <document>]`. The returned
/// string is the lowercase review rendering; the digest inside the document
/// remains a typed byte value on the canonical wire.
///
/// # Errors
///
/// Returns the same stable semantic, duplicate-coordinate, and canonical-shape
/// failures as [`encode_authority_facts_cbor`].
pub fn digest_authority_facts_document(
    document: &AuthorityFactsDocument,
) -> Result<String, Vec<AuthorityFactsLoadFailure>> {
    let path = authority_document_path(document);
    let mut failures = Vec::new();
    validate_document(document, &path, &mut failures);
    validate_unique_document_facts(document, &path, &mut failures);
    if !failures.is_empty() {
        return Err(failures);
    }

    let value = authority_facts_canonical_value(document, &path)?;
    let digest = digest_canonical_value(AUTHORITY_FACTS_API_VERSION, &value).map_err(|_err| {
        vec![failure(
            AuthorityFactsLoadFailureKind::InvalidCanonicalCbor,
            &path,
            "canonicalCbor",
            &document.source.coordinate,
        )]
    })?;
    Ok(sha256_review_string(&digest))
}

/// Decode canonical authority-facts bytes into the existing document model.
///
/// Canonical CBOR validation runs before structural decoding. The reconstructed
/// document then passes through the same semantic validation used by the JSON
/// file loader and [`compiler_context_from_authority_facts`].
///
/// # Errors
///
/// Returns stable failures for non-canonical bytes, a value outside the frozen
/// CDDL-compatible shape, a non-canonical write-class set, or an invalid
/// authority-facts semantic value.
pub fn decode_authority_facts_cbor(
    bytes: &[u8],
) -> Result<AuthorityFactsDocument, Vec<AuthorityFactsLoadFailure>> {
    let value = decode_canonical_cbor(bytes).map_err(|err| {
        let kind = if err.kind() == CanonicalErrorKind::DuplicateMapKey {
            AuthorityFactsLoadFailureKind::DuplicateFact
        } else {
            AuthorityFactsLoadFailureKind::InvalidCanonicalCbor
        };
        vec![failure(
            kind,
            AUTHORITY_FACTS_CBOR_PATH,
            "canonicalCbor",
            "",
        )]
    })?;
    let document = authority_facts_document_from_value(&value).map_err(|err| vec![err])?;
    let path = authority_document_path(&document);
    let mut failures = Vec::new();
    validate_document(&document, &path, &mut failures);
    if failures.is_empty() {
        Ok(document)
    } else {
        Err(failures)
    }
}

/// Load authority-facts files and merge them into a compiler context.
///
/// # Errors
///
/// Returns stable load failures when any input file is invalid or when the
/// loaded facts contain conflicting values for the same source coordinate.
pub fn load_compiler_context_from_authority_fact_files<I, P>(
    paths: I,
) -> Result<CompilerContext, Vec<AuthorityFactsLoadFailure>>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut documents = Vec::new();
    let mut failures = Vec::new();

    for path in paths {
        match load_authority_facts_file(path) {
            Ok(document) => documents.push(document),
            Err(mut errs) => failures.append(&mut errs),
        }
    }
    if !failures.is_empty() {
        return Err(failures);
    }

    compiler_context_from_authority_facts(&documents)
}

/// Merge validated authority-facts documents into a compiler context.
///
/// # Errors
///
/// Returns stable load failures when one document repeats a fact coordinate or
/// separate documents bind the same coordinate to different values. Identical
/// facts from separate digest-consistent documents remain harmless.
pub fn compiler_context_from_authority_facts(
    documents: &[AuthorityFactsDocument],
) -> Result<CompilerContext, Vec<AuthorityFactsLoadFailure>> {
    let mut source_digests = BTreeMap::<(AuthorityFactSourceKind, String), String>::new();
    let mut profiles = BTreeMap::<String, (String, BTreeSet<WriteClass>)>::new();
    let mut effects = BTreeMap::<String, WriteClass>::new();
    let mut budgets = BTreeMap::<String, CoreBudget>::new();
    let mut failures = Vec::new();

    for document in documents {
        let path = authority_document_path(document);
        validate_document(document, &path, &mut failures);
        validate_unique_document_facts(document, &path, &mut failures);
    }
    if !failures.is_empty() {
        return Err(failures);
    }

    for document in documents {
        let path = authority_document_path(document);
        insert_source_digest(document, &path, &mut source_digests, &mut failures);
        for profile in &document.operation_profiles {
            insert_profile_fact(&mut profiles, profile, &path, &mut failures);
        }
        for effect in &document.effect_write_classes {
            insert_fact(
                &mut effects,
                &effect.effect,
                effect.write_class.clone(),
                "effectWriteClasses",
                &path,
                &mut failures,
            );
        }
        for budget in &document.budgets {
            insert_fact(
                &mut budgets,
                &budget.source,
                budget.budget.clone(),
                "budgets",
                &path,
                &mut failures,
            );
        }
    }

    if !failures.is_empty() {
        return Err(failures);
    }

    let mut context = CompilerContext::new();
    for (source, (core, write_classes)) in profiles {
        context = context
            .with_operation_profile(source.clone(), core)
            .with_operation_profile_write_classes(source, write_classes);
    }
    for (effect, write_class) in effects {
        context = context.with_effect_write_class(effect, write_class);
    }
    for (source, budget) in budgets {
        context = context.with_budget(source, budget);
    }
    Ok(context)
}

fn authority_document_path(document: &AuthorityFactsDocument) -> String {
    format!(
        "{}@{}",
        source_kind_name(document.source.kind),
        document.source.coordinate
    )
}

fn validate_document(
    document: &AuthorityFactsDocument,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    if document.api_version != AUTHORITY_FACTS_API_VERSION {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidApiVersion,
            path,
            "apiVersion",
            &document.api_version,
        ));
    }
    validate_source(&document.source, path, failures);
    for profile in &document.operation_profiles {
        validate_profile_fact(profile, path, failures);
    }
    for effect in &document.effect_write_classes {
        validate_effect_fact(effect, path, failures);
    }
    for budget in &document.budgets {
        validate_budget_fact(budget, path, failures);
    }
}

fn validate_unique_document_facts(
    document: &AuthorityFactsDocument,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    validate_unique_fact_coordinates(
        document
            .operation_profiles
            .iter()
            .map(|profile| profile.source.as_str()),
        path,
        "operationProfiles",
        failures,
    );
    validate_unique_fact_coordinates(
        document
            .effect_write_classes
            .iter()
            .map(|effect| effect.effect.as_str()),
        path,
        "effectWriteClasses",
        failures,
    );
    validate_unique_fact_coordinates(
        document.budgets.iter().map(|budget| budget.source.as_str()),
        path,
        "budgets",
        failures,
    );
}

fn validate_unique_fact_coordinates<'a>(
    coordinates: impl IntoIterator<Item = &'a str>,
    path: &str,
    field: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    let mut seen = BTreeSet::new();
    for coordinate in coordinates {
        if !seen.insert(coordinate) {
            failures.push(failure(
                AuthorityFactsLoadFailureKind::DuplicateFact,
                path,
                field,
                coordinate,
            ));
        }
    }
}

fn authority_facts_canonical_value(
    document: &AuthorityFactsDocument,
    path: &str,
) -> Result<CanonicalValue, Vec<AuthorityFactsLoadFailure>> {
    let operation_profiles = document
        .operation_profiles
        .iter()
        .map(|profile| {
            let allowed_write_classes = profile
                .allowed_write_classes
                .iter()
                .filter_map(abi_write_class_name)
                .collect::<BTreeSet<_>>();
            (
                CanonicalValue::Text(profile.source.clone()),
                cbor_map([
                    ("core", CanonicalValue::Text(profile.core.clone())),
                    (
                        "allowedWriteClasses",
                        CanonicalValue::Map(
                            allowed_write_classes
                                .into_iter()
                                .map(|name| {
                                    (CanonicalValue::Text(name.to_owned()), CanonicalValue::Null)
                                })
                                .collect(),
                        ),
                    ),
                ]),
            )
        })
        .collect();
    let effect_write_classes = document
        .effect_write_classes
        .iter()
        .filter_map(|effect| {
            abi_write_class_name(&effect.write_class).map(|write_class| {
                (
                    CanonicalValue::Text(effect.effect.clone()),
                    CanonicalValue::Text(write_class.to_owned()),
                )
            })
        })
        .collect();
    let budgets = document
        .budgets
        .iter()
        .map(|budget| {
            (
                CanonicalValue::Text(budget.source.clone()),
                cbor_map([
                    (
                        "maxSteps",
                        CanonicalValue::Integer(i128::from(budget.budget.max_steps)),
                    ),
                    (
                        "maxAllocatedBytes",
                        CanonicalValue::Integer(i128::from(budget.budget.max_allocated_bytes)),
                    ),
                    (
                        "maxOutputBytes",
                        CanonicalValue::Integer(i128::from(budget.budget.max_output_bytes)),
                    ),
                ]),
            )
        })
        .collect();

    Ok(cbor_map([
        (
            "apiVersion",
            CanonicalValue::Text(document.api_version.clone()),
        ),
        (
            "source",
            cbor_map([
                (
                    "kind",
                    CanonicalValue::Text(source_kind_name(document.source.kind).to_owned()),
                ),
                (
                    "coordinate",
                    CanonicalValue::Text(document.source.coordinate.clone()),
                ),
                (
                    "digest",
                    canonical_digest_value(&document.source.digest, path)?,
                ),
            ]),
        ),
        ("operationProfiles", CanonicalValue::Map(operation_profiles)),
        (
            "effectWriteClasses",
            CanonicalValue::Map(effect_write_classes),
        ),
        ("budgets", CanonicalValue::Map(budgets)),
    ]))
}

fn canonical_digest_value(
    digest: &str,
    path: &str,
) -> Result<CanonicalValue, Vec<AuthorityFactsLoadFailure>> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(vec![failure(
            AuthorityFactsLoadFailureKind::NonDigestLockedSource,
            path,
            "source.digest",
            "",
        )]);
    };
    if hex.len() != 64 {
        return Err(vec![failure(
            AuthorityFactsLoadFailureKind::NonDigestLockedSource,
            path,
            "source.digest",
            "",
        )]);
    }
    let mut bytes = Vec::with_capacity(32);
    for chunk in hex.as_bytes().as_chunks::<2>().0 {
        let Some(high) = hex_nibble(chunk[0]) else {
            return Err(vec![failure(
                AuthorityFactsLoadFailureKind::NonDigestLockedSource,
                path,
                "source.digest",
                "",
            )]);
        };
        let Some(low) = hex_nibble(chunk[1]) else {
            return Err(vec![failure(
                AuthorityFactsLoadFailureKind::NonDigestLockedSource,
                path,
                "source.digest",
                "",
            )]);
        };
        bytes.push((high << 4) | low);
    }
    Ok(CanonicalValue::Array(vec![
        CanonicalValue::Text("sha256".to_owned()),
        CanonicalValue::Bytes(bytes),
    ]))
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn authority_facts_document_from_value(
    value: &CanonicalValue,
) -> Result<AuthorityFactsDocument, AuthorityFactsLoadFailure> {
    let fields = exact_text_map(
        value,
        "document",
        &[
            "apiVersion",
            "source",
            "operationProfiles",
            "effectWriteClasses",
            "budgets",
        ],
    )?;
    let api_version = required_text(&fields, "apiVersion")?.to_owned();
    let source = parse_canonical_source(required_field(&fields, "source")?)?;
    let operation_profiles =
        parse_canonical_operation_profiles(required_field(&fields, "operationProfiles")?)?;
    let effect_write_classes =
        parse_canonical_effect_write_classes(required_field(&fields, "effectWriteClasses")?)?;
    let budgets = parse_canonical_budgets(required_field(&fields, "budgets")?)?;

    Ok(AuthorityFactsDocument {
        api_version,
        source,
        operation_profiles,
        effect_write_classes,
        budgets,
    })
}

fn parse_canonical_source(
    value: &CanonicalValue,
) -> Result<AuthorityFactSource, AuthorityFactsLoadFailure> {
    let fields = exact_text_map(value, "source", &["kind", "coordinate", "digest"])?;
    let kind_name = required_text(&fields, "kind")?;
    let kind = parse_source_kind(kind_name).ok_or_else(|| {
        failure(
            AuthorityFactsLoadFailureKind::InvalidSourceKind,
            AUTHORITY_FACTS_CBOR_PATH,
            "source.kind",
            kind_name,
        )
    })?;
    let coordinate = required_text(&fields, "coordinate")?.to_owned();
    let digest = parse_canonical_digest(required_field(&fields, "digest")?, &coordinate)?;
    Ok(AuthorityFactSource {
        kind,
        coordinate,
        digest,
    })
}

fn parse_canonical_digest(
    value: &CanonicalValue,
    coordinate: &str,
) -> Result<String, AuthorityFactsLoadFailure> {
    let CanonicalValue::Array(parts) = value else {
        return Err(non_digest_failure(coordinate));
    };
    let [CanonicalValue::Text(algorithm), CanonicalValue::Bytes(bytes)] = parts.as_slice() else {
        return Err(non_digest_failure(coordinate));
    };
    if algorithm != "sha256" || bytes.len() != 32 {
        return Err(non_digest_failure(coordinate));
    }

    Ok(sha256_review_string(bytes))
}

fn non_digest_failure(coordinate: &str) -> AuthorityFactsLoadFailure {
    failure(
        AuthorityFactsLoadFailureKind::NonDigestLockedSource,
        AUTHORITY_FACTS_CBOR_PATH,
        "source.digest",
        coordinate,
    )
}

fn parse_canonical_operation_profiles(
    value: &CanonicalValue,
) -> Result<Vec<OperationProfileFact>, AuthorityFactsLoadFailure> {
    let entries = text_map(value, "operationProfiles")?;
    entries
        .into_iter()
        .map(|(source, value)| {
            let fields =
                exact_text_map(value, "operationProfiles", &["core", "allowedWriteClasses"])?;
            let core = required_text(&fields, "core")?.to_owned();
            let allowed_write_classes = parse_canonical_write_class_set(
                required_field(&fields, "allowedWriteClasses")?,
                "operationProfiles.allowedWriteClasses",
                source,
            )?;
            Ok(OperationProfileFact {
                source: source.to_owned(),
                core,
                allowed_write_classes,
            })
        })
        .collect()
}

fn parse_canonical_effect_write_classes(
    value: &CanonicalValue,
) -> Result<Vec<EffectWriteClassFact>, AuthorityFactsLoadFailure> {
    let entries = text_map(value, "effectWriteClasses")?;
    entries
        .into_iter()
        .map(|(effect, value)| {
            let write_class_name = canonical_text(value, "effectWriteClasses")?;
            let write_class = parse_write_class(write_class_name).ok_or_else(|| {
                failure(
                    AuthorityFactsLoadFailureKind::InvalidWriteClass,
                    AUTHORITY_FACTS_CBOR_PATH,
                    "effectWriteClasses.writeClass",
                    effect,
                )
            })?;
            Ok(EffectWriteClassFact {
                effect: effect.to_owned(),
                write_class,
            })
        })
        .collect()
}

fn parse_canonical_budgets(
    value: &CanonicalValue,
) -> Result<Vec<BudgetFact>, AuthorityFactsLoadFailure> {
    let entries = text_map(value, "budgets")?;
    entries
        .into_iter()
        .map(|(source, value)| {
            let fields = exact_text_map(
                value,
                "budgets",
                &["maxSteps", "maxAllocatedBytes", "maxOutputBytes"],
            )?;
            Ok(BudgetFact {
                source: source.to_owned(),
                budget: CoreBudget {
                    max_steps: required_u64(&fields, "maxSteps")?,
                    max_allocated_bytes: required_u64(&fields, "maxAllocatedBytes")?,
                    max_output_bytes: required_u64(&fields, "maxOutputBytes")?,
                },
            })
        })
        .collect()
}

fn parse_canonical_write_class_set(
    value: &CanonicalValue,
    field: &str,
    coordinate: &str,
) -> Result<Vec<WriteClass>, AuthorityFactsLoadFailure> {
    let entries = text_map(value, field)?;
    let mut write_classes = Vec::with_capacity(entries.len());
    for (name, marker) in entries {
        if marker != &CanonicalValue::Null {
            return Err(shape_failure(field, coordinate));
        }
        let write_class = parse_write_class(name).ok_or_else(|| {
            failure(
                AuthorityFactsLoadFailureKind::InvalidWriteClass,
                AUTHORITY_FACTS_CBOR_PATH,
                field,
                coordinate,
            )
        })?;
        write_classes.push(write_class);
    }
    Ok(write_classes)
}

fn exact_text_map<'a>(
    value: &'a CanonicalValue,
    field: &str,
    expected: &[&str],
) -> Result<BTreeMap<&'a str, &'a CanonicalValue>, AuthorityFactsLoadFailure> {
    let fields = text_map(value, field)?;
    if fields.len() != expected.len() || expected.iter().any(|name| !fields.contains_key(name)) {
        return Err(shape_failure(field, ""));
    }
    Ok(fields)
}

fn text_map<'a>(
    value: &'a CanonicalValue,
    field: &str,
) -> Result<BTreeMap<&'a str, &'a CanonicalValue>, AuthorityFactsLoadFailure> {
    let CanonicalValue::Map(entries) = value else {
        return Err(shape_failure(field, ""));
    };
    let mut fields = BTreeMap::new();
    for (key, value) in entries {
        let CanonicalValue::Text(key) = key else {
            return Err(shape_failure(field, ""));
        };
        if fields.insert(key.as_str(), value).is_some() {
            return Err(failure(
                AuthorityFactsLoadFailureKind::DuplicateFact,
                AUTHORITY_FACTS_CBOR_PATH,
                field,
                key,
            ));
        }
    }
    Ok(fields)
}

fn required_field<'a>(
    fields: &BTreeMap<&str, &'a CanonicalValue>,
    field: &str,
) -> Result<&'a CanonicalValue, AuthorityFactsLoadFailure> {
    fields
        .get(field)
        .copied()
        .ok_or_else(|| shape_failure(field, ""))
}

fn required_text<'a>(
    fields: &BTreeMap<&str, &'a CanonicalValue>,
    field: &str,
) -> Result<&'a str, AuthorityFactsLoadFailure> {
    canonical_text(required_field(fields, field)?, field)
}

fn required_u64(
    fields: &BTreeMap<&str, &CanonicalValue>,
    field: &str,
) -> Result<u64, AuthorityFactsLoadFailure> {
    let CanonicalValue::Integer(value) = required_field(fields, field)? else {
        return Err(shape_failure(field, ""));
    };
    u64::try_from(*value).map_err(|_err| shape_failure(field, ""))
}

fn canonical_text<'a>(
    value: &'a CanonicalValue,
    field: &str,
) -> Result<&'a str, AuthorityFactsLoadFailure> {
    let CanonicalValue::Text(value) = value else {
        return Err(shape_failure(field, ""));
    };
    Ok(value)
}

fn shape_failure(field: &str, coordinate: &str) -> AuthorityFactsLoadFailure {
    failure(
        AuthorityFactsLoadFailureKind::InvalidCborShape,
        AUTHORITY_FACTS_CBOR_PATH,
        field,
        coordinate,
    )
}

fn cbor_map<const N: usize>(entries: [(&str, CanonicalValue); N]) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (CanonicalValue::Text(key.to_owned()), value))
            .collect(),
    )
}

fn validate_source(
    source: &AuthorityFactSource,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    if source.coordinate.is_empty() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::MissingCoordinate,
            path,
            "source.coordinate",
            "",
        ));
    } else if !is_authority_coordinate(&source.coordinate) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            path,
            "source.coordinate",
            &source.coordinate,
        ));
    }
    if !is_sha256_review_digest(&source.digest) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::NonDigestLockedSource,
            path,
            "source.digest",
            &source.coordinate,
        ));
    }
}

fn validate_profile_fact(
    profile: &OperationProfileFact,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    validate_fact_coordinate(&profile.source, path, "operationProfiles.source", failures);
    validate_fact_coordinate(&profile.core, path, "operationProfiles.core", failures);
    for write_class in &profile.allowed_write_classes {
        if !is_abi_write_class(write_class) {
            failures.push(failure(
                AuthorityFactsLoadFailureKind::InvalidWriteClass,
                path,
                "operationProfiles.allowedWriteClasses",
                &profile.source,
            ));
        }
    }
}

fn validate_effect_fact(
    effect: &EffectWriteClassFact,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    validate_fact_coordinate(&effect.effect, path, "effectWriteClasses.effect", failures);
    if !is_abi_write_class(&effect.write_class) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidWriteClass,
            path,
            "effectWriteClasses.writeClass",
            &effect.effect,
        ));
    }
}

fn validate_budget_fact(
    budget: &BudgetFact,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    validate_fact_coordinate(&budget.source, path, "budgets.source", failures);
}

fn validate_fact_coordinate(
    coordinate: &str,
    path: &str,
    field: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    if coordinate.is_empty() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::MissingCoordinate,
            path,
            field,
            "",
        ));
    } else if !is_authority_coordinate(coordinate) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            path,
            field,
            coordinate,
        ));
    }
}

fn insert_source_digest(
    document: &AuthorityFactsDocument,
    path: &str,
    source_digests: &mut BTreeMap<(AuthorityFactSourceKind, String), String>,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    let key = (document.source.kind, document.source.coordinate.clone());
    let digest = document.source.digest.to_ascii_lowercase();
    if let Some(existing_digest) = source_digests.get(&key) {
        if existing_digest != &digest {
            failures.push(failure(
                AuthorityFactsLoadFailureKind::ConflictingFact,
                path,
                "source.digest",
                &document.source.coordinate,
            ));
        }
    } else {
        source_digests.insert(key, digest);
    }
}

fn validate_raw_document(
    raw: RawAuthorityFactsDocument,
    path: &str,
) -> Result<AuthorityFactsDocument, Vec<AuthorityFactsLoadFailure>> {
    let mut failures = Vec::new();
    if raw.api_version != AUTHORITY_FACTS_API_VERSION {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidApiVersion,
            path,
            "apiVersion",
            &raw.api_version,
        ));
    }

    let source_kind = parse_source_kind(&raw.source.kind);
    if source_kind.is_none() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidSourceKind,
            path,
            "source.kind",
            &raw.source.kind,
        ));
    }
    let source_coordinate = raw.source.coordinate.unwrap_or_default();
    if source_coordinate.is_empty() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::MissingCoordinate,
            path,
            "source.coordinate",
            "",
        ));
    } else if !is_authority_coordinate(&source_coordinate) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            path,
            "source.coordinate",
            &source_coordinate,
        ));
    }
    let digest = raw.source.digest.unwrap_or_default();
    if !is_sha256_review_digest(&digest) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::NonDigestLockedSource,
            path,
            "source.digest",
            &source_coordinate,
        ));
    }

    let operation_profiles = raw
        .operation_profiles
        .into_iter()
        .filter_map(|profile| validate_operation_profile(profile, path, &mut failures))
        .collect();
    let effect_write_classes = raw
        .effect_write_classes
        .into_iter()
        .filter_map(|effect| validate_effect_write_class(effect, path, &mut failures))
        .collect();
    let budgets = raw
        .budgets
        .into_iter()
        .filter_map(|budget| validate_budget(budget, path, &mut failures))
        .collect();

    if !failures.is_empty() {
        return Err(failures);
    }

    let document = AuthorityFactsDocument {
        api_version: raw.api_version,
        source: AuthorityFactSource {
            kind: source_kind.expect("source kind already validated"),
            coordinate: source_coordinate,
            digest,
        },
        operation_profiles,
        effect_write_classes,
        budgets,
    };
    validate_unique_document_facts(&document, path, &mut failures);
    if failures.is_empty() {
        Ok(document)
    } else {
        Err(failures)
    }
}

fn validate_operation_profile(
    raw: RawOperationProfileFact,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) -> Option<OperationProfileFact> {
    let mut valid = true;
    if raw.source.is_empty() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::MissingCoordinate,
            path,
            "operationProfiles.source",
            "",
        ));
        valid = false;
    } else if !is_authority_coordinate(&raw.source) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            path,
            "operationProfiles.source",
            &raw.source,
        ));
        valid = false;
    }
    if raw.core.is_empty() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::MissingCoordinate,
            path,
            "operationProfiles.core",
            &raw.source,
        ));
        valid = false;
    } else if !is_authority_coordinate(&raw.core) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            path,
            "operationProfiles.core",
            &raw.core,
        ));
        valid = false;
    }

    let mut allowed_write_classes = Vec::new();
    for write_class in raw.allowed_write_classes {
        if let Some(value) = parse_write_class(&write_class) {
            allowed_write_classes.push(value);
        } else {
            failures.push(failure(
                AuthorityFactsLoadFailureKind::InvalidWriteClass,
                path,
                "operationProfiles.allowedWriteClasses",
                &raw.source,
            ));
            valid = false;
        }
    }

    valid.then_some(OperationProfileFact {
        source: raw.source,
        core: raw.core,
        allowed_write_classes,
    })
}

fn validate_effect_write_class(
    raw: RawEffectWriteClassFact,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) -> Option<EffectWriteClassFact> {
    let mut valid = true;
    if raw.effect.is_empty() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::MissingCoordinate,
            path,
            "effectWriteClasses.effect",
            "",
        ));
        valid = false;
    } else if !is_authority_coordinate(&raw.effect) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            path,
            "effectWriteClasses.effect",
            &raw.effect,
        ));
        valid = false;
    }
    let write_class = if let Some(value) = parse_write_class(&raw.write_class) {
        value
    } else {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidWriteClass,
            path,
            "effectWriteClasses.writeClass",
            &raw.effect,
        ));
        valid = false;
        WriteClass::None
    };

    valid.then_some(EffectWriteClassFact {
        effect: raw.effect,
        write_class,
    })
}

fn validate_budget(
    raw: RawBudgetFact,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) -> Option<BudgetFact> {
    if raw.source.is_empty() {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::MissingCoordinate,
            path,
            "budgets.source",
            "",
        ));
        return None;
    }
    if !is_authority_coordinate(&raw.source) {
        failures.push(failure(
            AuthorityFactsLoadFailureKind::InvalidCoordinate,
            path,
            "budgets.source",
            &raw.source,
        ));
        return None;
    }

    Some(BudgetFact {
        source: raw.source,
        budget: CoreBudget {
            max_steps: raw.max_steps,
            max_allocated_bytes: raw.max_allocated_bytes,
            max_output_bytes: raw.max_output_bytes,
        },
    })
}

fn insert_profile_fact(
    profiles: &mut BTreeMap<String, (String, BTreeSet<WriteClass>)>,
    profile: &OperationProfileFact,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) {
    let value = (
        profile.core.clone(),
        profile.allowed_write_classes.iter().cloned().collect(),
    );
    insert_fact(
        profiles,
        &profile.source,
        value,
        "operationProfiles",
        path,
        failures,
    );
}

fn insert_fact<T>(
    map: &mut BTreeMap<String, T>,
    coordinate: &str,
    value: T,
    field: &str,
    path: &str,
    failures: &mut Vec<AuthorityFactsLoadFailure>,
) where
    T: PartialEq,
{
    if let Some(existing) = map.get(coordinate) {
        if existing != &value {
            failures.push(failure(
                AuthorityFactsLoadFailureKind::ConflictingFact,
                path,
                field,
                coordinate,
            ));
        }
    } else {
        map.insert(coordinate.to_owned(), value);
    }
}

fn parse_source_kind(value: &str) -> Option<AuthorityFactSourceKind> {
    match value {
        "lawpack" => Some(AuthorityFactSourceKind::Lawpack),
        "targetProfile" => Some(AuthorityFactSourceKind::TargetProfile),
        _ => None,
    }
}

fn source_kind_name(kind: AuthorityFactSourceKind) -> &'static str {
    match kind {
        AuthorityFactSourceKind::Lawpack => "lawpack",
        AuthorityFactSourceKind::TargetProfile => "targetProfile",
    }
}

fn is_authority_coordinate(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'@' | b'/' | b':')
        })
}

fn parse_write_class(value: &str) -> Option<WriteClass> {
    match value {
        "none" => Some(WriteClass::None),
        "read" => Some(WriteClass::Read),
        "create" => Some(WriteClass::Create),
        "ensure" => Some(WriteClass::Ensure),
        "append" => Some(WriteClass::Append),
        "replace" => Some(WriteClass::Replace),
        "delete" => Some(WriteClass::Delete),
        "custom" => Some(WriteClass::Custom("custom".to_owned())),
        _ => None,
    }
}

fn abi_write_class_name(write_class: &WriteClass) -> Option<&str> {
    match write_class {
        WriteClass::None => Some("none"),
        WriteClass::Read => Some("read"),
        WriteClass::Create => Some("create"),
        WriteClass::Ensure => Some("ensure"),
        WriteClass::Append => Some("append"),
        WriteClass::Replace => Some("replace"),
        WriteClass::Delete => Some("delete"),
        WriteClass::Custom(value) if value == "custom" => Some("custom"),
        WriteClass::Custom(_) => None,
    }
}

fn is_abi_write_class(write_class: &WriteClass) -> bool {
    matches!(
        write_class,
        WriteClass::None
            | WriteClass::Read
            | WriteClass::Create
            | WriteClass::Ensure
            | WriteClass::Append
            | WriteClass::Replace
            | WriteClass::Delete
    ) || matches!(write_class, WriteClass::Custom(value) if value == "custom")
}

fn failure(
    kind: AuthorityFactsLoadFailureKind,
    path: &str,
    field: &str,
    coordinate: &str,
) -> AuthorityFactsLoadFailure {
    AuthorityFactsLoadFailure {
        kind,
        path: path.to_owned(),
        field: field.to_owned(),
        coordinate: coordinate.to_owned(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAuthorityFactsDocument {
    api_version: String,
    source: RawAuthorityFactSource,
    #[serde(default)]
    operation_profiles: Vec<RawOperationProfileFact>,
    #[serde(default)]
    effect_write_classes: Vec<RawEffectWriteClassFact>,
    #[serde(default)]
    budgets: Vec<RawBudgetFact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAuthorityFactSource {
    kind: String,
    coordinate: Option<String>,
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawOperationProfileFact {
    source: String,
    core: String,
    #[serde(default)]
    allowed_write_classes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawEffectWriteClassFact {
    effect: String,
    write_class: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawBudgetFact {
    source: String,
    max_steps: u64,
    max_allocated_bytes: u64,
    max_output_bytes: u64,
}
