//! Canonical CBOR encoding for Edict Core and Target IR artifacts.
//!
//! This module implements the `edict.canonical-cbor/v1` subset needed by the
//! current in-memory Core and Target IR models. It emits deterministic bytes
//! and can validate those bytes by decoding to a canonical value and
//! re-encoding. It also computes reviewed SHA-256 digest frames.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str;

use sha2::{Digest, Sha256};

use crate::core_ir::{
    is_lowercase_sha256_review_digest, parse_core_integer, validate_core_module_type_integrity,
    CompareOp, CoreBlock, CoreBudget, CoreExpr, CoreExternalActionBudget, CoreImport,
    CoreImportKind, CoreIntent, CoreModule, CoreNode, CoreObstructionArm, CorePredicate, CoreType,
    CoreValue, InputConstraint, InputConstraintSource, LocalRef, ResourceRef,
};
use crate::target_ir::{
    TargetIrArtifact, TargetIrExternalActionRequest, TargetIrIntent, TargetIrPureBinding,
    TargetIrRequireFailure, TargetIrRequirement, TargetIrSemanticClosure, TargetIrStep,
};

/// Canonical encoding profile for Core artifacts.
pub const CORE_CANONICAL_ENCODING: &str = "edict.canonical-cbor/v1";

/// Hash frame prefix for Edict and Continuum artifact digests.
pub const CORE_DIGEST_FRAME: &str = "edict.digest/v1";

/// Maximum child nesting depth accepted by the canonical encoder and decoder.
///
/// The root value starts at depth zero, so a scalar wrapped in exactly this many
/// containers is accepted. The next nested value is rejected before recursive
/// traversal can consume additional stack.
pub const MAX_CANONICAL_NESTING_DEPTH: usize = 128;

/// Artifact domain label for Core module digests.
pub const CORE_MODULE_DIGEST_DOMAIN: &str = "edict.core.module/v1";

/// Artifact domain label for Target IR artifact digests.
pub const TARGET_IR_ARTIFACT_DIGEST_DOMAIN: &str = "edict.target-ir.artifact/v1";

/// Canonical envelope version for typed external-action resources.
pub const EXTERNAL_ACTION_RESOURCE_API_VERSION: &str = "edict.external-action-resource/v1";

/// Artifact domain label for typed external-action resource digests.
pub const EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN: &str = "edict.external-action-resource/v1";

/// Stable canonical encoding error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalErrorKind {
    /// A Core value cannot be represented in the supported canonical CBOR subset.
    UnsupportedValue,
    /// A numeric Core value is outside the supported integer range or malformed.
    InvalidInteger,
    /// A digest-required Core reference has not been resolved.
    UnresolvedDigest,
    /// A digest string is not the expected review rendering.
    InvalidDigest,
    /// The byte stream ended before a complete value was decoded.
    UnexpectedEof,
    /// The byte stream contained extra data after one complete value.
    TrailingData,
    /// The byte stream uses a CBOR major type or additional-info form we reject.
    UnsupportedCbor,
    /// The byte stream decodes but is not in canonical form.
    NonCanonical,
    /// A value exceeds the deterministic canonical container nesting limit.
    NestingLimitExceeded,
    /// A decoded CBOR map contains duplicate keys.
    DuplicateMapKey,
    /// A decoded text string is not valid UTF-8.
    InvalidUtf8,
}

/// Canonical encoding failure with stable kind plus detail for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalError {
    kind: CanonicalErrorKind,
    message: String,
}

impl CanonicalError {
    fn new(kind: CanonicalErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Return the stable error category.
    #[must_use]
    pub const fn kind(&self) -> CanonicalErrorKind {
        self.kind
    }
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CanonicalError {}

/// Decoded canonical CBOR value tree.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalValue {
    /// CBOR null.
    Null,
    /// CBOR boolean.
    Bool(bool),
    /// CBOR integer, represented across positive and negative major types.
    Integer(i128),
    /// CBOR byte string.
    Bytes(Vec<u8>),
    /// CBOR UTF-8 text string.
    Text(String),
    /// CBOR array.
    Array(Vec<CanonicalValue>),
    /// CBOR map. Encoding sorts keys by their canonical encoded bytes.
    Map(Vec<(CanonicalValue, CanonicalValue)>),
}

/// SHA-256 digest for a Core artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CoreDigest {
    bytes: [u8; 32],
}

impl CoreDigest {
    const fn sha256(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Return the digest algorithm name.
    #[must_use]
    pub const fn algorithm(&self) -> &'static str {
        "sha256"
    }

    /// Return the raw SHA-256 digest bytes.
    #[must_use]
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Return the human review rendering `sha256:<64 lowercase hex>`.
    #[must_use]
    pub fn to_review_string(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for CoreDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.algorithm())?;
        f.write_str(":")?;
        for byte in self.bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Compute a domain-framed digest from a decoded canonical value.
///
/// This helper does not prove owning-schema validity. Provider invocation code
/// calls it only after its explicit schema capability accepts the value. Crate
/// privacy keeps the raw framing helper out of the public API.
pub(crate) fn digest_canonical_value(
    domain: &str,
    value: &CanonicalValue,
) -> Result<[u8; 32], CanonicalError> {
    if domain.is_empty() {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "canonical artifact digest domain is empty",
        ));
    }
    // Encode each tuple member at its own schema root. Digest framing is not
    // part of the artifact value and must not consume its nesting budget.
    let mut preimage = vec![0x83]; // Canonical fixed-length array of three values.
    preimage.extend(encode_canonical_cbor(&text(CORE_DIGEST_FRAME))?);
    preimage.extend(encode_canonical_cbor(&text(domain))?);
    preimage.extend(encode_canonical_cbor(value)?);
    let hash = Sha256::digest(preimage);
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&hash);
    Ok(bytes)
}

/// Decode canonical CBOR and compute its domain-framed SHA-256 identity.
///
/// This establishes canonical encoding and one shared digest frame. It does
/// not establish the artifact's owning schema; callers must validate that
/// separately.
///
/// # Errors
///
/// Returns an error for non-canonical CBOR, an empty domain, an unsupported
/// value, or a value beyond the canonical nesting boundary.
pub fn digest_canonical_artifact(domain: &str, bytes: &[u8]) -> Result<CoreDigest, CanonicalError> {
    let value = decode_canonical_cbor(bytes)?;
    Ok(CoreDigest::sha256(digest_canonical_value(domain, &value)?))
}

pub(crate) fn sha256_review_string(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut review = String::with_capacity(7 + bytes.len() * 2);
    review.push_str("sha256:");
    for byte in bytes {
        review.push(char::from(HEX[usize::from(byte >> 4)]));
        review.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    review
}

/// Encode a Core module as `edict.canonical-cbor/v1`.
///
/// # Errors
///
/// Returns an error if a Core integer or digest cannot be represented in the
/// supported canonical form.
pub fn encode_core_module(module: &CoreModule) -> Result<Vec<u8>, CanonicalError> {
    encode_canonical_cbor(&core_module_value(module)?)
}

/// Compute the reviewed digest for a Core module.
///
/// The digest is SHA-256 over the canonical CBOR encoding of:
///
/// ```text
/// ["edict.digest/v1", "edict.core.module/v1", <canonical Core module value>]
/// ```
///
/// # Errors
///
/// Returns an error if the Core module cannot be represented in the supported
/// canonical form.
pub fn digest_core_module(module: &CoreModule) -> Result<CoreDigest, CanonicalError> {
    Ok(CoreDigest::sha256(digest_canonical_value(
        CORE_MODULE_DIGEST_DOMAIN,
        &core_module_value(module)?,
    )?))
}

/// Encode a Target IR artifact as `edict.canonical-cbor/v1`.
///
/// # Errors
///
/// Returns an error if a Target IR resource digest, Core expression, Core
/// predicate, or integer cannot be represented in the supported canonical form.
pub fn encode_target_ir_artifact(artifact: &TargetIrArtifact) -> Result<Vec<u8>, CanonicalError> {
    encode_canonical_cbor(&target_ir_artifact_value(artifact)?)
}

/// Compute the reviewed digest for a Target IR artifact.
///
/// The digest is SHA-256 over the canonical CBOR encoding of:
///
/// ```text
/// ["edict.digest/v1", "edict.target-ir.artifact/v1", <canonical Target IR value>]
/// ```
///
/// # Errors
///
/// Returns an error if the Target IR artifact cannot be represented in the
/// supported canonical form.
pub fn digest_target_ir_artifact(
    artifact: &TargetIrArtifact,
) -> Result<CoreDigest, CanonicalError> {
    Ok(CoreDigest::sha256(digest_canonical_value(
        TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
        &target_ir_artifact_value(artifact)?,
    )?))
}

/// Digest domain for a contract bundle's `semanticBundleDigest`.
pub const BUNDLE_SEMANTIC_DIGEST_DOMAIN: &str = "edict.bundle.semantic/v1";

/// Digest domain for a contract bundle's `releaseBundleDigest`.
pub const BUNDLE_RELEASE_DIGEST_DOMAIN: &str = "edict.bundle.release/v1";

/// The two contract-bundle digest layers. Using a typed domain instead of an
/// arbitrary string keeps the preimage frame honest: only the two defined
/// bundle layers can be requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleDigestDomain {
    /// `edict.bundle.semantic/v1` — executable semantics.
    Semantic,
    /// `edict.bundle.release/v1` — semantics plus source provenance and toolchain.
    Release,
}

impl BundleDigestDomain {
    /// Return the spec domain label this layer frames its preimage under.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            BundleDigestDomain::Semantic => BUNDLE_SEMANTIC_DIGEST_DOMAIN,
            BundleDigestDomain::Release => BUNDLE_RELEASE_DIGEST_DOMAIN,
        }
    }
}

/// A source artifact descriptor inside a release bundle preimage: a logical,
/// package-relative path bound together with its digest-locked artifact.
#[derive(Debug, Clone, Copy)]
pub struct BundleSourceDescriptor<'a> {
    /// The logical, package-relative source path (bundle provenance).
    pub logical_path: &'a str,
    /// The digest-locked source artifact reference.
    pub artifact: &'a ResourceRef,
}

/// One ordered component of a contract-bundle digest preimage.
///
/// Structure is part of the byte-level contract. A bare digest binds only its
/// hash; a [`Resource`](BundlePreimageComponent::Resource) binds its
/// `coordinate` (identity) *and* digest; a
/// [`SourceArtifacts`](BundlePreimageComponent::SourceArtifacts) component binds
/// each logical source path. This is what lets the release layer detect a
/// logical-path or toolchain-coordinate change even when the digest bytes are
/// unchanged.
#[derive(Debug, Clone, Copy)]
pub enum BundlePreimageComponent<'a> {
    /// A single `sha256:<hex>` review digest.
    Digest(&'a str),
    /// An ordered list of `sha256:<hex>` review digests.
    DigestList(&'a [String]),
    /// A digest-locked resource reference binding coordinate and digest.
    Resource(&'a ResourceRef),
    /// An ordered list of digest-locked resource references.
    ResourceList(&'a [ResourceRef]),
    /// An ordered list of source artifact descriptors (logical path + artifact).
    SourceArtifacts(&'a [BundleSourceDescriptor<'a>]),
}

/// Encode one preimage component into its canonical value. Resource references
/// reuse the established `{id, digest}` encoding (which requires a digest-locked
/// resource), so a coordinate change moves the digest even when the digest bytes
/// are unchanged, and a missing digest is rejected.
fn bundle_component_value(
    component: &BundlePreimageComponent,
) -> Result<CanonicalValue, CanonicalError> {
    Ok(match component {
        BundlePreimageComponent::Digest(digest) => bundle_digest_value(digest)?,
        BundlePreimageComponent::DigestList(digests) => CanonicalValue::Array(
            digests
                .iter()
                .map(|digest| bundle_digest_value(digest))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        BundlePreimageComponent::Resource(resource) => bundle_resource_ref_value(resource)?,
        BundlePreimageComponent::ResourceList(resources) => CanonicalValue::Array(
            resources
                .iter()
                .map(bundle_resource_ref_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        BundlePreimageComponent::SourceArtifacts(descriptors) => CanonicalValue::Array(
            descriptors
                .iter()
                .map(|descriptor| {
                    if !is_logical_package_relative_path(descriptor.logical_path) {
                        return Err(CanonicalError::new(
                            CanonicalErrorKind::UnsupportedValue,
                            "bundle source artifact path must be logical and package-relative",
                        ));
                    }
                    Ok(CanonicalValue::Array(vec![
                        text(descriptor.logical_path),
                        bundle_resource_ref_value(descriptor.artifact)?,
                    ]))
                })
                .collect::<Result<Vec<_>, CanonicalError>>()?,
        ),
    })
}

fn bundle_resource_ref_value(resource: &ResourceRef) -> Result<CanonicalValue, CanonicalError> {
    if resource.coordinate.is_empty() {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "bundle resource coordinate is empty",
        ));
    }
    let Some(digest) = &resource.digest else {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnresolvedDigest,
            "bundle resource digest is unresolved",
        ));
    };
    Ok(map([
        ("id", text(&resource.coordinate)),
        ("digest", bundle_digest_value(digest)?),
    ]))
}

fn bundle_digest_value(digest: &str) -> Result<CanonicalValue, CanonicalError> {
    if !is_lowercase_sha256_review_digest(digest) {
        return Err(CanonicalError::new(
            CanonicalErrorKind::InvalidDigest,
            "bundle digest must use sha256:<64 lowercase hex> review rendering",
        ));
    }
    digest_value(digest)
}

pub(crate) fn is_logical_package_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Compute a contract-bundle layer digest over its ordered preimage components.
///
/// The digest is SHA-256 over the canonical CBOR encoding of:
///
/// ```text
/// ["edict.digest/v1", "<domain>", [<typed component values>]]
/// ```
///
/// `<domain>` is [`BundleDigestDomain::label`]. Each `sha256:<hex>` review
/// digest is parsed into the authoritative typed value `["sha256", <32 raw
/// bytes>]`; resource references encode as `{id, digest}` maps and require a
/// present digest; source descriptors encode as `[logical_path, {id, digest}]`
/// after logical package-relative path validation. Review strings are never
/// hashed directly.
///
/// # Errors
///
/// Returns an error if any component digest is not a strict
/// `sha256:<64 lowercase hex>` review rendering.
pub fn digest_bundle_layer(
    domain: BundleDigestDomain,
    components: &[BundlePreimageComponent],
) -> Result<CoreDigest, CanonicalError> {
    let mut payload = Vec::with_capacity(components.len());
    for component in components {
        payload.push(bundle_component_value(component)?);
    }
    Ok(CoreDigest::sha256(digest_canonical_value(
        domain.label(),
        &CanonicalValue::Array(payload),
    )?))
}

/// Encode a decoded canonical value to canonical CBOR bytes.
///
/// # Errors
///
/// Returns an error if an integer is outside the supported CBOR range.
pub fn encode_canonical_cbor(value: &CanonicalValue) -> Result<Vec<u8>, CanonicalError> {
    let mut out = Vec::new();
    encode_value(value, &mut out, 0)?;
    Ok(out)
}

/// Decode and validate canonical CBOR bytes.
///
/// The decoder accepts only the definite-length subset used by Edict Core. After
/// decoding, it re-encodes the value and requires an exact byte match, rejecting
/// non-minimal integers, unsorted maps, and other non-canonical encodings.
///
/// # Errors
///
/// Returns a stable [`CanonicalErrorKind`] for malformed, unsupported, or
/// non-canonical bytes.
pub fn decode_canonical_cbor(bytes: &[u8]) -> Result<CanonicalValue, CanonicalError> {
    let mut decoder = Decoder::new(bytes);
    let value = decoder.value(0)?;
    if decoder.remaining() != 0 {
        return Err(CanonicalError::new(
            CanonicalErrorKind::TrailingData,
            "canonical CBOR stream has trailing bytes",
        ));
    }
    let reencoded = encode_canonical_cbor(&value)?;
    if reencoded != bytes {
        return Err(CanonicalError::new(
            CanonicalErrorKind::NonCanonical,
            "decoded value does not re-encode to identical canonical bytes",
        ));
    }
    Ok(value)
}

fn target_ir_artifact_value(artifact: &TargetIrArtifact) -> Result<CanonicalValue, CanonicalError> {
    if !crate::target_ir::execution_order::artifact_has_valid_execution_order(artifact) {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "Target IR execution order is invalid for its selected domain",
        ));
    }
    if artifact.source_core_coordinate.is_empty() {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "Target IR source Core coordinate is empty",
        ));
    }
    let has_external_action_requests = artifact
        .intents
        .values()
        .any(|intent| !intent.external_action_requests.is_empty());
    let has_pure_bindings = artifact
        .intents
        .values()
        .any(|intent| !intent.pure_bindings.is_empty());
    if artifact.semantic_closure.is_none()
        && (artifact
            .intents
            .values()
            .any(|intent| intent.basis.is_some())
            || has_pure_bindings
            || has_external_action_requests)
    {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "basis-, pure-binding-, or external-action-bearing Target IR requires a semantic closure",
        ));
    }
    if let Some(closure) = &artifact.semantic_closure {
        for request in artifact
            .intents
            .values()
            .flat_map(|intent| &intent.external_action_requests)
        {
            if !closure
                .capabilities
                .iter()
                .any(|capability| capability == &request.operation)
            {
                return Err(CanonicalError::new(
                    CanonicalErrorKind::UnsupportedValue,
                    format!(
                        "Target IR request operation `{}` is absent from the capability closure",
                        request.operation.coordinate
                    ),
                ));
            }
        }
    }
    let mut entries = vec![
        (
            "kind",
            text(
                if artifact.domain == crate::target_ir::ECHO_ORDERED_SPAN_IR_DOMAIN {
                    "orderedTargetIrArtifact"
                } else {
                    "targetIrArtifact"
                },
            ),
        ),
        ("domain", text(&artifact.domain)),
        (
            "targetProfile",
            target_ir_resource_ref_value(&artifact.target_profile)?,
        ),
        (
            "sourceCoreCoordinate",
            text(&artifact.source_core_coordinate),
        ),
        (
            "intents",
            string_map_results(
                artifact
                    .intents
                    .iter()
                    .map(|(name, intent)| Ok((name.as_str(), target_ir_intent_value(intent)?))),
            )?,
        ),
    ];
    if let Some(closure) = &artifact.semantic_closure {
        if closure.source_core.coordinate != artifact.source_core_coordinate {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnsupportedValue,
                "Target IR semantic closure source Core coordinate does not match the artifact source Core coordinate",
            ));
        }
        entries.push((
            "semanticClosure",
            target_ir_semantic_closure_value(closure)?,
        ));
    }
    Ok(map(entries))
}

fn target_ir_semantic_closure_value(
    closure: &TargetIrSemanticClosure,
) -> Result<CanonicalValue, CanonicalError> {
    let lawpacks = target_ir_resource_set(&closure.lawpacks, "lawpack")?;
    let capabilities = target_ir_resource_set(&closure.capabilities, "capability")?;
    let mut entries = vec![
        (
            "sourceCore",
            target_ir_resource_ref_value(&closure.source_core)?,
        ),
        (
            "lawpacks",
            sorted_array_results(lawpacks.into_values().map(target_ir_resource_ref_value))?,
        ),
    ];
    if !capabilities.is_empty() {
        entries.push((
            "capabilities",
            sorted_array_results(capabilities.into_values().map(target_ir_resource_ref_value))?,
        ));
    }
    Ok(map(entries))
}

fn target_ir_resource_set<'a>(
    resources: &'a [ResourceRef],
    field_name: &str,
) -> Result<BTreeMap<&'a str, &'a ResourceRef>, CanonicalError> {
    let mut indexed = BTreeMap::new();
    for resource in resources {
        if let Some(prior) = indexed.get(resource.coordinate.as_str()) {
            if *prior != resource {
                return Err(CanonicalError::new(
                    CanonicalErrorKind::UnsupportedValue,
                    format!(
                        "Target IR {field_name} coordinate `{}` is bound to conflicting resources",
                        resource.coordinate
                    ),
                ));
            }
        } else {
            indexed.insert(resource.coordinate.as_str(), resource);
        }
    }
    Ok(indexed)
}

fn target_ir_resource_ref_value(resource: &ResourceRef) -> Result<CanonicalValue, CanonicalError> {
    if resource.coordinate.is_empty() {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "Target IR resource coordinate is empty",
        ));
    }
    let Some(digest) = &resource.digest else {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnresolvedDigest,
            "Target IR resource digest is unresolved",
        ));
    };
    if !is_lowercase_sha256_review_digest(digest) {
        return Err(CanonicalError::new(
            CanonicalErrorKind::InvalidDigest,
            "Target IR resource digest must use sha256:<64 lowercase hex> review rendering",
        ));
    }
    Ok(map([
        ("id", text(&resource.coordinate)),
        ("digest", digest_value(digest)?),
    ]))
}

fn validate_target_ir_intent_identities(intent: &TargetIrIntent) -> Result<(), CanonicalError> {
    let mut binding_ids = BTreeSet::new();
    for binding in &intent.pure_bindings {
        if binding.id.is_empty() || !binding_ids.insert(binding.id.as_str()) {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnsupportedValue,
                format!(
                    "Target IR pure binding id `{}` is empty or duplicated",
                    binding.id
                ),
            ));
        }
    }
    let mut local_ids = BTreeSet::from(["arg.0"]);
    for (producer, binding) in intent
        .pure_bindings
        .iter()
        .map(|binding| ("pure binding", &binding.binding))
        .chain(
            intent
                .steps
                .iter()
                .map(|step| ("target step", &step.binding)),
        )
        .chain(
            intent
                .external_action_requests
                .iter()
                .map(|request| ("external-action request", &request.binding)),
        )
    {
        if binding.id.is_empty() || !local_ids.insert(binding.id.as_str()) {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnsupportedValue,
                format!(
                    "Target IR {producer} local id `{}` is empty or duplicated across the application input and producers",
                    binding.id
                ),
            ));
        }
    }
    let mut request_ids = BTreeSet::new();
    for request in &intent.external_action_requests {
        if !request_ids.insert(request.id.as_str()) {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnsupportedValue,
                format!(
                    "Target IR external-action request id `{}` is duplicated",
                    request.id
                ),
            ));
        }
    }
    Ok(())
}

fn target_ir_intent_value(intent: &TargetIrIntent) -> Result<CanonicalValue, CanonicalError> {
    validate_target_ir_intent_identities(intent)?;
    let mut entries = vec![
        ("operationProfile", text(&intent.operation_profile)),
        (
            "inputConstraints",
            sorted_array_results(intent.input_constraints.iter().map(input_constraint_value))?,
        ),
        (
            "coreEvaluationBudget",
            core_budget_value(&intent.core_evaluation_budget),
        ),
        (
            "requirements",
            array_results(intent.requirements.iter().map(target_ir_requirement_value))?,
        ),
        (
            "steps",
            array_results(intent.steps.iter().map(target_ir_step_value))?,
        ),
        ("result", core_expr_value(&intent.result)?),
    ];
    if let Some(order) = &intent.execution_order {
        entries.push((
            "executionOrder",
            CanonicalValue::Array(order.iter().map(|id| text(id)).collect()),
        ));
    }
    if let Some(basis) = &intent.basis {
        entries.push(("basis", core_expr_value(basis)?));
    }
    if !intent.pure_bindings.is_empty() {
        entries.push((
            "pureBindings",
            array_results(
                intent
                    .pure_bindings
                    .iter()
                    .map(target_ir_pure_binding_value),
            )?,
        ));
    }
    if !intent.external_action_requests.is_empty() {
        entries.push((
            "externalActionRequests",
            array_results(
                intent
                    .external_action_requests
                    .iter()
                    .map(target_ir_external_action_request_value),
            )?,
        ));
    }
    Ok(map(entries))
}

fn target_ir_pure_binding_value(
    binding: &TargetIrPureBinding,
) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("id", text(&binding.id)),
        ("binding", local_ref_value(&binding.binding)),
        ("value", core_expr_value(&binding.value)?),
    ]))
}

fn target_ir_external_action_request_value(
    request: &TargetIrExternalActionRequest,
) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("id", text(&request.id)),
        ("binding", local_ref_value(&request.binding)),
        (
            "operation",
            target_ir_resource_ref_value(&request.operation)?,
        ),
        ("inputType", text(&request.input_type)),
        ("settlementType", text(&request.settlement_type)),
        (
            "inputSchema",
            target_ir_resource_ref_value(&request.input_schema)?,
        ),
        (
            "settlementSchema",
            target_ir_resource_ref_value(&request.settlement_schema)?,
        ),
        ("input", core_expr_value(&request.input)?),
        ("authorityScope", core_expr_value(&request.authority_scope)?),
        ("basis", core_expr_value(&request.basis)?),
        ("budget", external_action_budget_value(&request.budget)?),
        (
            "reconciliationLaw",
            target_ir_resource_ref_value(&request.reconciliation_law)?,
        ),
        ("state", text("awaitingSettlement")),
        ("settlementAdmission", text("schemaRequired")),
    ]))
}

fn target_ir_requirement_value(
    requirement: &TargetIrRequirement,
) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("id", text(&requirement.id)),
        ("predicate", core_predicate_value(&requirement.predicate)?),
        (
            "onFailure",
            target_ir_require_failure_value(&requirement.on_failure)?,
        ),
    ]))
}

fn target_ir_require_failure_value(
    failure: &TargetIrRequireFailure,
) -> Result<CanonicalValue, CanonicalError> {
    match failure {
        TargetIrRequireFailure::Terminal { reason } => Ok(map([
            ("kind", text("terminal")),
            ("reason", core_obstruction_reason_value(reason)?),
        ])),
        TargetIrRequireFailure::ContinueObstructed { reason } => Ok(map([
            ("kind", text("continueObstructed")),
            ("reason", core_obstruction_reason_value(reason)?),
        ])),
    }
}

fn target_ir_step_value(step: &TargetIrStep) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("id", text(&step.id)),
        ("binding", local_ref_value(&step.binding)),
        ("effect", text(&step.effect)),
        ("targetIntrinsic", text(&step.target_intrinsic)),
        ("input", core_expr_value(&step.input)?),
        (
            "obstructionFailures",
            sorted_text_set(step.obstruction_failures.iter().map(String::as_str)),
        ),
        (
            "obstructionArms",
            string_map_results(
                step.obstruction_arms
                    .iter()
                    .map(|(failure, arm)| Ok((failure.as_str(), core_obstruction_arm_value(arm)?))),
            )?,
        ),
    ]))
}

fn core_module_value(module: &CoreModule) -> Result<CanonicalValue, CanonicalError> {
    let validated = validate_core_module_type_integrity(module).map_err(|failure| {
        CanonicalError::new(CanonicalErrorKind::UnsupportedValue, failure.to_string())
    })?;
    let module = validated.module();
    let capability_imports = module
        .imports
        .iter()
        .filter(|import| import.kind == CoreImportKind::Capability)
        .map(|import| &import.resource)
        .collect::<Vec<_>>();
    let mut requests = Vec::new();
    for intent in module.intents.values() {
        collect_request_operations(&intent.body, &mut requests, 0)?;
    }
    for request in requests {
        if !capability_imports.contains(&request) {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnsupportedValue,
                format!(
                    "Core request operation `{}` is absent from the capability imports",
                    request.coordinate
                ),
            ));
        }
    }
    let mut fields = vec![
        ("apiVersion", text(&module.api_version)),
        ("coordinate", text(&module.coordinate)),
        (
            "imports",
            sorted_array_results(module.imports.iter().map(core_import_value))?,
        ),
        (
            "types",
            string_map_results(
                module
                    .types
                    .iter()
                    .map(|(name, ty)| core_type_value(ty).map(|value| (name.as_str(), value))),
            )?,
        ),
        (
            "intents",
            string_map_results(
                module
                    .intents
                    .iter()
                    .map(|(name, intent)| Ok((name.as_str(), core_intent_value(intent)?))),
            )?,
        ),
        (
            "requiredCoreCapabilities",
            sorted_text_set(module.required_core_capabilities.iter().map(String::as_str)),
        ),
    ];
    if !module.functions.is_empty() {
        fields.push((
            "functions",
            string_map_results(
                module
                    .functions
                    .iter()
                    .map(|(name, function)| Ok((name.as_str(), core_function_value(function)?))),
            )?,
        ));
    }
    Ok(map(fields))
}

fn core_function_value(
    function: &crate::core_ir::CoreFunction,
) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("params", array(function.params.iter().map(local_ref_value))),
        ("returnType", text(&function.return_type)),
        (
            "body",
            map([
                (
                    "locals",
                    array(function.body.locals.iter().map(local_ref_value)),
                ),
                (
                    "bindings",
                    array_results(function.body.bindings.iter().map(|binding| {
                        Ok(map([
                            ("kind", text("let")),
                            ("binding", local_ref_value(&binding.binding)),
                            ("value", core_expr_value(&binding.value)?),
                        ]))
                    }))?,
                ),
                ("result", core_expr_value(&function.body.result)?),
            ]),
        ),
    ]))
}

fn collect_request_operations<'a>(
    block: &'a CoreBlock,
    out: &mut Vec<&'a ResourceRef>,
    depth: usize,
) -> Result<(), CanonicalError> {
    if depth > MAX_CANONICAL_NESTING_DEPTH {
        return Err(CanonicalError::new(
            CanonicalErrorKind::NestingLimitExceeded,
            format!(
                "Core request traversal nesting exceeds maximum depth {MAX_CANONICAL_NESTING_DEPTH}"
            ),
        ));
    }
    for node in &block.nodes {
        match node {
            CoreNode::ExternalActionRequest { operation, .. } => out.push(operation),
            CoreNode::For { body, .. } => collect_request_operations(body, out, depth + 1)?,
            CoreNode::Branch {
                then_block,
                else_block,
                ..
            } => {
                collect_request_operations(then_block, out, depth + 1)?;
                collect_request_operations(else_block, out, depth + 1)?;
            }
            CoreNode::Let { .. } | CoreNode::Require { .. } | CoreNode::Effect { .. } => {}
        }
    }
    Ok(())
}

fn core_import_value(import: &CoreImport) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("kind", text(import.kind.as_str())),
        ("ref", resource_ref_value(&import.resource)?),
    ]))
}

fn resource_ref_value(resource: &ResourceRef) -> Result<CanonicalValue, CanonicalError> {
    if resource.coordinate.is_empty() {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "Core resource coordinate is empty",
        ));
    }
    let mut entries = vec![("id", text(&resource.coordinate))];
    let Some(digest) = &resource.digest else {
        return Err(CanonicalError::new(
            CanonicalErrorKind::UnresolvedDigest,
            "Core import resource digest is unresolved",
        ));
    };
    entries.push(("digest", digest_value(digest)?));
    Ok(map(entries))
}

fn digest_value(digest: &str) -> Result<CanonicalValue, CanonicalError> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(CanonicalError::new(
            CanonicalErrorKind::InvalidDigest,
            "digest must use sha256 review rendering",
        ));
    };
    if hex.len() != 64 {
        return Err(CanonicalError::new(
            CanonicalErrorKind::InvalidDigest,
            "sha256 digest must contain 64 hex characters",
        ));
    }
    let mut bytes = Vec::with_capacity(32);
    for chunk in hex.as_bytes().as_chunks::<2>().0 {
        let hi = hex_value(chunk[0])?;
        let lo = hex_value(chunk[1])?;
        bytes.push((hi << 4) | lo);
    }
    Ok(CanonicalValue::Array(vec![
        text("sha256"),
        CanonicalValue::Bytes(bytes),
    ]))
}

fn hex_value(byte: u8) -> Result<u8, CanonicalError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(CanonicalError::new(
            CanonicalErrorKind::InvalidDigest,
            "sha256 digest must contain hex characters",
        )),
    }
}

fn core_type_value(ty: &CoreType) -> Result<CanonicalValue, CanonicalError> {
    let value = match ty {
        CoreType::Bool => map([("kind", text("Bool"))]),
        CoreType::Unit => map([("kind", text("Unit"))]),
        CoreType::Int { width } => map([("kind", text(width))]),
        CoreType::String { max, canonical } => map([
            ("kind", text("String")),
            ("max", uint(*max)),
            ("canonical", text(canonical)),
        ]),
        CoreType::Bytes { min, max } => {
            if min.is_some_and(|min| min > *max) {
                return Err(CanonicalError::new(
                    CanonicalErrorKind::UnsupportedValue,
                    "Core byte type minimum exceeds its maximum",
                ));
            }
            let mut fields = vec![("kind", text("Bytes"))];
            if let Some(min) = min {
                fields.push(("min", uint(*min)));
            }
            fields.push(("max", uint(*max)));
            map(fields)
        }
        CoreType::Nominal {
            contract,
            representation,
        } => map([
            ("kind", text("Nominal")),
            ("contract", text(contract)),
            ("representation", text(representation)),
        ]),
        CoreType::Record { fields } => map([
            ("kind", text("Record")),
            (
                "fields",
                string_map(fields.iter().map(|(name, ty)| (name.as_str(), text(ty)))),
            ),
        ]),
        CoreType::Variant { cases } => map([
            ("kind", text("Variant")),
            (
                "cases",
                string_map(cases.iter().map(|(name, payload)| {
                    let body = if let Some(payload) = payload {
                        map([("payload", text(payload))])
                    } else {
                        CanonicalValue::Map(Vec::new())
                    };
                    (name.as_str(), body)
                })),
            ),
        ]),
        CoreType::Option { item } => map([("kind", text("Option")), ("item", text(item))]),
        CoreType::List { item, max } => map([
            ("kind", text("List")),
            ("item", text(item)),
            ("max", uint(*max)),
        ]),
        CoreType::Map { key, value, max } => map([
            ("kind", text("Map")),
            ("key", text(key)),
            ("value", text(value)),
            ("max", uint(*max)),
        ]),
        CoreType::CapabilityRef { item } => {
            map([("kind", text("CapabilityRef")), ("item", text(item))])
        }
        CoreType::ExternalActionRequest { settlement } => map([
            ("kind", text("ExternalActionRequest")),
            ("settlement", text(settlement)),
        ]),
    };
    Ok(value)
}

fn core_intent_value(intent: &CoreIntent) -> Result<CanonicalValue, CanonicalError> {
    let mut entries = vec![
        ("input", text(&intent.input)),
        ("output", text(&intent.output)),
        (
            "requiredOperationProfile",
            text(&intent.required_operation_profile),
        ),
        (
            "inputConstraints",
            sorted_array_results(intent.input_constraints.iter().map(input_constraint_value))?,
        ),
        (
            "coreEvaluationBudget",
            core_budget_value(&intent.core_evaluation_budget),
        ),
        ("body", core_block_value(&intent.body)?),
    ];
    if let Some(basis) = &intent.basis {
        entries.push(("basis", core_expr_value(basis)?));
    }
    Ok(map(entries))
}

fn input_constraint_value(constraint: &InputConstraint) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("coordinate", text(&constraint.coordinate)),
        (
            "source",
            text(input_constraint_source_str(constraint.source)),
        ),
        ("predicate", core_predicate_value(&constraint.predicate)?),
    ]))
}

const fn input_constraint_source_str(source: InputConstraintSource) -> &'static str {
    match source {
        InputConstraintSource::Where => "where",
        InputConstraintSource::Compiler => "compiler",
    }
}

fn core_budget_value(budget: &CoreBudget) -> CanonicalValue {
    map([
        ("maxSteps", uint(budget.max_steps)),
        ("maxAllocatedBytes", uint(budget.max_allocated_bytes)),
        ("maxOutputBytes", uint(budget.max_output_bytes)),
    ])
}

fn core_block_value(block: &CoreBlock) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("locals", array(block.locals.iter().map(local_ref_value))),
        (
            "nodes",
            array_results(block.nodes.iter().map(core_node_value))?,
        ),
        ("result", core_expr_value(&block.result)?),
    ]))
}

fn core_node_value(node: &CoreNode) -> Result<CanonicalValue, CanonicalError> {
    match node {
        CoreNode::Let { binding, value } => Ok(map([
            ("kind", text("let")),
            ("binding", local_ref_value(binding)),
            ("value", core_expr_value(value)?),
        ])),
        CoreNode::Require { predicate, arm } => Ok(map([
            ("kind", text("require")),
            ("predicate", core_predicate_value(predicate)?),
            ("onFailure", core_require_failure_arm_value(arm)?),
        ])),
        CoreNode::Effect {
            binding,
            effect,
            input,
            obstruction_map,
        } => Ok(map([
            ("kind", text("effect")),
            ("binding", local_ref_value(binding)),
            ("effect", text(effect)),
            ("input", core_expr_value(input)?),
            (
                "obstructionMap",
                string_map_results(obstruction_map.iter().map(|(failure, arm)| {
                    Ok((failure.as_str(), core_obstruction_arm_value(arm)?))
                }))?,
            ),
        ])),
        CoreNode::ExternalActionRequest {
            binding,
            operation,
            input_type,
            settlement_type,
            input_schema,
            settlement_schema,
            input,
            authority_scope,
            basis,
            budget,
            reconciliation_law,
        } => Ok(map([
            ("kind", text("externalActionRequest")),
            ("binding", local_ref_value(binding)),
            ("operation", resource_ref_value(operation)?),
            ("inputType", text(input_type)),
            ("settlementType", text(settlement_type)),
            ("inputSchema", resource_ref_value(input_schema)?),
            ("settlementSchema", resource_ref_value(settlement_schema)?),
            ("input", core_expr_value(input)?),
            ("authorityScope", core_expr_value(authority_scope)?),
            ("basis", core_expr_value(basis)?),
            ("budget", external_action_budget_value(budget)?),
            ("reconciliationLaw", resource_ref_value(reconciliation_law)?),
            ("state", text("awaitingSettlement")),
            ("settlementAdmission", text("schemaRequired")),
        ])),
        CoreNode::For {
            binder,
            iter,
            bound,
            body,
        } => Ok(map([
            ("kind", text("for")),
            ("binder", local_ref_value(binder)),
            ("iter", core_expr_value(iter)?),
            ("bound", core_bound_value(bound)),
            ("body", core_block_value(body)?),
        ])),
        CoreNode::Branch {
            binding,
            predicate,
            then_block,
            else_block,
        } => {
            let mut fields = vec![
                ("kind", text("branch")),
                ("predicate", core_predicate_value(predicate)?),
                ("then", core_block_value(then_block)?),
                ("else", core_block_value(else_block)?),
            ];
            if let Some(binding) = binding {
                fields.push(("binding", local_ref_value(binding)));
            }
            Ok(map(fields))
        }
    }
}

fn core_bound_value(bound: &crate::core_ir::CoreBound) -> CanonicalValue {
    match bound {
        crate::core_ir::CoreBound::Literal(value) => {
            map([("kind", text("literal")), ("value", uint(*value))])
        }
        crate::core_ir::CoreBound::Coordinate(reference) => {
            map([("kind", text("coordinate")), ("ref", text(reference))])
        }
    }
}

fn external_action_budget_value(
    budget: &CoreExternalActionBudget,
) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        (
            "maxSettlementBytes",
            core_expr_value(&budget.max_settlement_bytes)?,
        ),
        ("maxAttempts", core_expr_value(&budget.max_attempts)?),
    ]))
}

fn core_require_failure_arm_value(
    arm: &crate::core_ir::CoreRequireFailureArm,
) -> Result<CanonicalValue, CanonicalError> {
    match arm {
        crate::core_ir::CoreRequireFailureArm::Terminal { reason } => Ok(map([
            ("kind", text("terminal")),
            ("reason", core_obstruction_reason_value(reason)?),
        ])),
        crate::core_ir::CoreRequireFailureArm::ContinueObstructed { reason } => Ok(map([
            ("kind", text("continueObstructed")),
            ("reason", core_obstruction_reason_value(reason)?),
        ])),
    }
}

fn core_obstruction_reason_value(
    reason: &crate::core_ir::CoreObstructionReason,
) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("reasonKind", text(&reason.kind)),
        (
            "payload",
            string_map_results(
                reason
                    .payload
                    .iter()
                    .map(|(name, expr)| Ok((name.as_str(), core_expr_value(expr)?))),
            )?,
        ),
    ]))
}

fn core_obstruction_arm_value(arm: &CoreObstructionArm) -> Result<CanonicalValue, CanonicalError> {
    Ok(map([
        ("binder", local_ref_value(&arm.binder)),
        ("value", core_expr_value(&arm.value)?),
    ]))
}

fn core_expr_value(expr: &CoreExpr) -> Result<CanonicalValue, CanonicalError> {
    Ok(match expr {
        CoreExpr::Local { reference } => {
            map([("kind", text("local")), ("ref", local_ref_value(reference))])
        }
        CoreExpr::Const(value) => map([("kind", text("const")), ("value", core_value(value)?)]),
        CoreExpr::Record { fields } => map([
            ("kind", text("record")),
            (
                "fields",
                string_map_results(
                    fields
                        .iter()
                        .map(|(name, expr)| Ok((name.as_str(), core_expr_value(expr)?))),
                )?,
            ),
        ]),
        CoreExpr::Field { base, field } => map([
            ("kind", text("field")),
            ("base", core_expr_value(base)?),
            ("field", text(field)),
        ]),
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } => map([
            ("kind", text("call")),
            ("callee", text(callee)),
            (
                "typeArgs",
                array(type_args.iter().map(String::as_str).map(text)),
            ),
            ("args", array_results(args.iter().map(core_expr_value))?),
        ]),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => map([
            ("kind", text("if")),
            ("predicate", core_predicate_value(predicate)?),
            ("then", core_expr_value(then_value)?),
            ("else", core_expr_value(else_value)?),
        ]),
    })
}

fn core_value(value: &CoreValue) -> Result<CanonicalValue, CanonicalError> {
    Ok(match value {
        CoreValue::Null => map([("kind", text("null"))]),
        CoreValue::Bool(value) => map([("kind", text("bool")), ("value", bool_value(*value))]),
        CoreValue::Int { width, value } => map([
            ("kind", text("int")),
            ("width", text(width)),
            ("value", int_text_value(width, value)?),
        ]),
        CoreValue::String(value) => map([("kind", text("string")), ("value", text(value))]),
        CoreValue::Bytes(value) => map([
            ("kind", text("bytes")),
            ("value", CanonicalValue::Bytes(value.clone())),
        ]),
    })
}

fn int_text_value(width: &str, value: &str) -> Result<CanonicalValue, CanonicalError> {
    let value = parse_core_integer(width, value).ok_or_else(|| {
        CanonicalError::new(
            CanonicalErrorKind::InvalidInteger,
            format!("Core integer value is outside the declared `{width}` domain"),
        )
    })?;
    Ok(CanonicalValue::Integer(value))
}

fn core_predicate_value(predicate: &CorePredicate) -> Result<CanonicalValue, CanonicalError> {
    Ok(match predicate {
        CorePredicate::True => map([("kind", text("true"))]),
        CorePredicate::False => map([("kind", text("false"))]),
        CorePredicate::Not(value) => map([
            ("kind", text("not")),
            ("value", core_predicate_value(value)?),
        ]),
        CorePredicate::All(values) => map([
            ("kind", text("all")),
            (
                "values",
                array_results(values.iter().map(core_predicate_value))?,
            ),
        ]),
        CorePredicate::Any(values) => map([
            ("kind", text("any")),
            (
                "values",
                array_results(values.iter().map(core_predicate_value))?,
            ),
        ]),
        CorePredicate::Compare { op, left, right } => map([
            ("kind", text("compare")),
            ("op", text(compare_op_str(*op))),
            ("left", core_expr_value(left)?),
            ("right", core_expr_value(right)?),
        ]),
    })
}

const fn compare_op_str(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Eq => "==",
        CompareOp::Ne => "!=",
        CompareOp::Lt => "<",
        CompareOp::Le => "<=",
        CompareOp::Gt => ">",
        CompareOp::Ge => ">=",
    }
}

fn local_ref_value(local: &LocalRef) -> CanonicalValue {
    map([
        ("id", text(&local.id)),
        ("alphaName", text(&local.alpha_name)),
        ("type", text(&local.ty)),
    ])
}

fn map<'a>(entries: impl IntoIterator<Item = (&'a str, CanonicalValue)>) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn string_map<'a>(entries: impl IntoIterator<Item = (&'a str, CanonicalValue)>) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn string_map_results<'a>(
    entries: impl IntoIterator<Item = Result<(&'a str, CanonicalValue), CanonicalError>>,
) -> Result<CanonicalValue, CanonicalError> {
    entries
        .into_iter()
        .map(|entry| entry.map(|(key, value)| (text(key), value)))
        .collect::<Result<Vec<_>, _>>()
        .map(CanonicalValue::Map)
}

fn array(entries: impl IntoIterator<Item = CanonicalValue>) -> CanonicalValue {
    CanonicalValue::Array(entries.into_iter().collect())
}

fn sorted_text_set<'a>(entries: impl IntoIterator<Item = &'a str>) -> CanonicalValue {
    array(
        entries
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(text),
    )
}

fn array_results(
    entries: impl IntoIterator<Item = Result<CanonicalValue, CanonicalError>>,
) -> Result<CanonicalValue, CanonicalError> {
    entries
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map(CanonicalValue::Array)
}

fn sorted_array_results(
    entries: impl IntoIterator<Item = Result<CanonicalValue, CanonicalError>>,
) -> Result<CanonicalValue, CanonicalError> {
    let mut encoded = entries
        .into_iter()
        .map(|entry| {
            let value = entry?;
            let bytes = encode_canonical_cbor(&value)?;
            Ok((bytes, value))
        })
        .collect::<Result<Vec<_>, CanonicalError>>()?;
    encoded.sort_by(|(left, _), (right, _)| left.cmp(right));
    Ok(CanonicalValue::Array(
        encoded.into_iter().map(|(_, value)| value).collect(),
    ))
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

const fn uint(value: u64) -> CanonicalValue {
    CanonicalValue::Integer(value as i128)
}

const fn bool_value(value: bool) -> CanonicalValue {
    CanonicalValue::Bool(value)
}

fn encode_value(
    value: &CanonicalValue,
    out: &mut Vec<u8>,
    depth: usize,
) -> Result<(), CanonicalError> {
    check_canonical_nesting_depth(depth)?;
    match value {
        CanonicalValue::Null => out.push(0xf6),
        CanonicalValue::Bool(false) => out.push(0xf4),
        CanonicalValue::Bool(true) => out.push(0xf5),
        CanonicalValue::Integer(value) => encode_integer(*value, out)?,
        CanonicalValue::Bytes(value) => {
            encode_type_value(2, usize_to_u64(value.len())?, out);
            out.extend(value);
        }
        CanonicalValue::Text(value) => {
            encode_type_value(3, usize_to_u64(value.len())?, out);
            out.extend(value.as_bytes());
        }
        CanonicalValue::Array(values) => {
            check_canonical_container_depth(depth)?;
            encode_type_value(4, usize_to_u64(values.len())?, out);
            for value in values {
                encode_value(value, out, depth + 1)?;
            }
        }
        CanonicalValue::Map(entries) => {
            check_canonical_container_depth(depth)?;
            let mut encoded = Vec::with_capacity(entries.len());
            let mut keys = BTreeSet::new();
            for (key, value) in entries {
                let mut key_bytes = Vec::new();
                encode_value(key, &mut key_bytes, depth + 1)?;
                if !keys.insert(key_bytes.clone()) {
                    return Err(CanonicalError::new(
                        CanonicalErrorKind::DuplicateMapKey,
                        "CBOR map contains duplicate keys",
                    ));
                }
                encoded.push((key_bytes, value));
            }
            encoded.sort_by(|(left, _), (right, _)| left.cmp(right));
            encode_type_value(5, usize_to_u64(encoded.len())?, out);
            for (key_bytes, value) in encoded {
                out.extend(key_bytes);
                encode_value(value, out, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn check_canonical_nesting_depth(depth: usize) -> Result<(), CanonicalError> {
    if depth > MAX_CANONICAL_NESTING_DEPTH {
        return Err(CanonicalError::new(
            CanonicalErrorKind::NestingLimitExceeded,
            format!("canonical value nesting exceeds maximum depth {MAX_CANONICAL_NESTING_DEPTH}"),
        ));
    }
    Ok(())
}

fn check_canonical_container_depth(depth: usize) -> Result<(), CanonicalError> {
    if depth >= MAX_CANONICAL_NESTING_DEPTH {
        return Err(CanonicalError::new(
            CanonicalErrorKind::NestingLimitExceeded,
            format!(
                "canonical container nesting exceeds maximum depth {MAX_CANONICAL_NESTING_DEPTH}"
            ),
        ));
    }
    Ok(())
}

fn encode_integer(value: i128, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    if value >= 0 {
        let value = u64::try_from(value).map_err(|_| {
            CanonicalError::new(
                CanonicalErrorKind::InvalidInteger,
                "positive integer exceeds CBOR uint range",
            )
        })?;
        encode_type_value(0, value, out);
    } else {
        let magnitude = (-1i128).checked_sub(value).ok_or_else(|| {
            CanonicalError::new(
                CanonicalErrorKind::InvalidInteger,
                "negative integer cannot be converted to CBOR range",
            )
        })?;
        let magnitude = u64::try_from(magnitude).map_err(|_| {
            CanonicalError::new(
                CanonicalErrorKind::InvalidInteger,
                "negative integer exceeds CBOR negative range",
            )
        })?;
        encode_type_value(1, magnitude, out);
    }
    Ok(())
}

fn encode_type_value(major: u8, value: u64, out: &mut Vec<u8>) {
    let prefix = major << 5;
    match value {
        0..=23 => out.push(prefix | u8::try_from(value).expect("value <= 23")),
        24..=0xff => {
            out.push(prefix | 0x18);
            out.push(u8::try_from(value).expect("value <= u8::MAX"));
        }
        0x100..=0xffff => {
            out.push(prefix | 0x19);
            out.extend(
                u16::try_from(value)
                    .expect("value <= u16::MAX")
                    .to_be_bytes(),
            );
        }
        0x1_0000..=0xffff_ffff => {
            out.push(prefix | 0x1a);
            out.extend(
                u32::try_from(value)
                    .expect("value <= u32::MAX")
                    .to_be_bytes(),
            );
        }
        _ => {
            out.push(prefix | 0x1b);
            out.extend(value.to_be_bytes());
        }
    }
}

fn usize_to_u64(value: usize) -> Result<u64, CanonicalError> {
    u64::try_from(value).map_err(|_| {
        CanonicalError::new(
            CanonicalErrorKind::UnsupportedValue,
            "collection length cannot fit in CBOR uint range",
        )
    })
}

struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn value(&mut self, depth: usize) -> Result<CanonicalValue, CanonicalError> {
        check_canonical_nesting_depth(depth)?;
        let initial = self.byte()?;
        let major = initial >> 5;
        let additional = initial & 0x1f;
        match major {
            0 => Ok(CanonicalValue::Integer(i128::from(
                self.argument(additional)?,
            ))),
            1 => Ok(CanonicalValue::Integer(
                -1 - i128::from(self.argument(additional)?),
            )),
            2 => {
                let len = self.length(additional)?;
                Ok(CanonicalValue::Bytes(self.take(len)?.to_vec()))
            }
            3 => {
                let len = self.length(additional)?;
                let bytes = self.take(len)?;
                let value = str::from_utf8(bytes).map_err(|_| {
                    CanonicalError::new(
                        CanonicalErrorKind::InvalidUtf8,
                        "CBOR text string is not valid UTF-8",
                    )
                })?;
                Ok(CanonicalValue::Text(value.to_owned()))
            }
            4 => {
                check_canonical_container_depth(depth)?;
                let len = self.length(additional)?;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    values.push(self.value(depth + 1)?);
                }
                Ok(CanonicalValue::Array(values))
            }
            5 => {
                check_canonical_container_depth(depth)?;
                let len = self.length(additional)?;
                let mut entries = Vec::with_capacity(len);
                let mut keys = BTreeSet::new();
                for _ in 0..len {
                    let key = self.value(depth + 1)?;
                    let mut encoded_key = Vec::new();
                    encode_value(&key, &mut encoded_key, depth + 1)?;
                    if !keys.insert(encoded_key) {
                        return Err(CanonicalError::new(
                            CanonicalErrorKind::DuplicateMapKey,
                            "CBOR map contains duplicate keys",
                        ));
                    }
                    let value = self.value(depth + 1)?;
                    entries.push((key, value));
                }
                Ok(CanonicalValue::Map(entries))
            }
            7 => match additional {
                20 => Ok(CanonicalValue::Bool(false)),
                21 => Ok(CanonicalValue::Bool(true)),
                22 => Ok(CanonicalValue::Null),
                _ => Err(CanonicalError::new(
                    CanonicalErrorKind::UnsupportedCbor,
                    "unsupported CBOR simple value",
                )),
            },
            _ => Err(CanonicalError::new(
                CanonicalErrorKind::UnsupportedCbor,
                "unsupported CBOR major type",
            )),
        }
    }

    fn argument(&mut self, additional: u8) -> Result<u64, CanonicalError> {
        match additional {
            0..=23 => Ok(u64::from(additional)),
            24 => Ok(u64::from(self.byte()?)),
            25 => {
                let bytes = self.take_array::<2>()?;
                Ok(u64::from(u16::from_be_bytes(bytes)))
            }
            26 => {
                let bytes = self.take_array::<4>()?;
                Ok(u64::from(u32::from_be_bytes(bytes)))
            }
            27 => Ok(u64::from_be_bytes(self.take_array::<8>()?)),
            _ => Err(CanonicalError::new(
                CanonicalErrorKind::UnsupportedCbor,
                "indefinite or reserved CBOR length is unsupported",
            )),
        }
    }

    fn length(&mut self, additional: u8) -> Result<usize, CanonicalError> {
        let len = usize::try_from(self.argument(additional)?).map_err(|_| {
            CanonicalError::new(
                CanonicalErrorKind::UnsupportedCbor,
                "CBOR collection length does not fit usize",
            )
        })?;
        if len > self.remaining() {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnexpectedEof,
                "CBOR declared length exceeds remaining input",
            ));
        }
        Ok(len)
    }

    fn byte(&mut self) -> Result<u8, CanonicalError> {
        let Some(value) = self.bytes.get(self.pos).copied() else {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnexpectedEof,
                "expected another CBOR byte",
            ));
        };
        self.pos += 1;
        Ok(value)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], CanonicalError> {
        let end = self.pos.checked_add(len).ok_or_else(|| {
            CanonicalError::new(
                CanonicalErrorKind::UnexpectedEof,
                "CBOR length overflowed input position",
            )
        })?;
        let Some(slice) = self.bytes.get(self.pos..end) else {
            return Err(CanonicalError::new(
                CanonicalErrorKind::UnexpectedEof,
                "CBOR value extends past end of input",
            ));
        };
        self.pos = end;
        Ok(slice)
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], CanonicalError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
}

#[cfg(test)]
mod canonical_artifact_tests {
    use sha2::{Digest, Sha256};

    use super::{
        decode_canonical_cbor, digest_canonical_value, encode_canonical_cbor,
        is_logical_package_relative_path, text, CanonicalErrorKind, CanonicalValue,
        CORE_DIGEST_FRAME, MAX_CANONICAL_NESTING_DEPTH,
    };

    fn nested_array(depth: usize) -> CanonicalValue {
        (0..depth).fold(CanonicalValue::Null, |value, _| {
            CanonicalValue::Array(vec![value])
        })
    }

    fn nested_array_bytes(depth: usize) -> Vec<u8> {
        let mut bytes = vec![0x81; depth];
        bytes.push(0xf6);
        bytes
    }

    fn nested_empty_array(container_count: usize) -> CanonicalValue {
        assert!(container_count > 0);
        (1..container_count).fold(CanonicalValue::Array(Vec::new()), |value, _| {
            CanonicalValue::Array(vec![value])
        })
    }

    fn nested_empty_array_bytes(container_count: usize) -> Vec<u8> {
        assert!(container_count > 0);
        let mut bytes = vec![0x81; container_count - 1];
        bytes.push(0x80);
        bytes
    }

    fn nested_empty_map(container_count: usize) -> CanonicalValue {
        assert!(container_count > 0);
        (1..container_count).fold(CanonicalValue::Map(Vec::new()), |value, _| {
            CanonicalValue::Map(vec![(text("k"), value)])
        })
    }

    fn nested_empty_map_bytes(container_count: usize) -> Vec<u8> {
        assert!(container_count > 0);
        let mut bytes = Vec::with_capacity((container_count - 1) * 3 + 1);
        for _ in 1..container_count {
            bytes.extend([0xa1, 0x61, b'k']);
        }
        bytes.push(0xa0);
        bytes
    }

    #[test]
    fn canonical_nesting_limit_is_enforced_on_encode_and_decode() {
        let at_limit = nested_array(MAX_CANONICAL_NESTING_DEPTH);
        let at_limit_bytes = nested_array_bytes(MAX_CANONICAL_NESTING_DEPTH);

        assert_eq!(
            encode_canonical_cbor(&at_limit).expect("maximum-depth value encodes"),
            at_limit_bytes
        );
        assert_eq!(
            decode_canonical_cbor(&at_limit_bytes).expect("maximum-depth bytes decode"),
            at_limit
        );

        let encode_err = encode_canonical_cbor(&nested_array(MAX_CANONICAL_NESTING_DEPTH + 1))
            .expect_err("over-depth value rejects");
        let decode_err =
            decode_canonical_cbor(&nested_array_bytes(MAX_CANONICAL_NESTING_DEPTH + 1))
                .expect_err("over-depth bytes reject");

        assert_eq!(encode_err.kind(), CanonicalErrorKind::NestingLimitExceeded);
        assert_eq!(decode_err.kind(), CanonicalErrorKind::NestingLimitExceeded);
    }

    #[test]
    fn canonical_nesting_limit_counts_empty_terminal_containers() {
        let cases = [
            (
                nested_empty_array(MAX_CANONICAL_NESTING_DEPTH),
                nested_empty_array(MAX_CANONICAL_NESTING_DEPTH + 1),
                nested_empty_array_bytes(MAX_CANONICAL_NESTING_DEPTH),
                nested_empty_array_bytes(MAX_CANONICAL_NESTING_DEPTH + 1),
            ),
            (
                nested_empty_map(MAX_CANONICAL_NESTING_DEPTH),
                nested_empty_map(MAX_CANONICAL_NESTING_DEPTH + 1),
                nested_empty_map_bytes(MAX_CANONICAL_NESTING_DEPTH),
                nested_empty_map_bytes(MAX_CANONICAL_NESTING_DEPTH + 1),
            ),
        ];

        for (at_limit, over_limit, at_limit_bytes, over_limit_bytes) in cases {
            assert_eq!(
                encode_canonical_cbor(&at_limit).expect("128 empty containers encode"),
                at_limit_bytes
            );
            assert_eq!(
                decode_canonical_cbor(&at_limit_bytes).expect("128 empty containers decode"),
                at_limit
            );

            let encode_err = encode_canonical_cbor(&over_limit)
                .expect_err("the 129th empty container must reject on encode");
            let decode_err = decode_canonical_cbor(&over_limit_bytes)
                .expect_err("the 129th empty container must reject on decode");
            assert_eq!(encode_err.kind(), CanonicalErrorKind::NestingLimitExceeded);
            assert_eq!(decode_err.kind(), CanonicalErrorKind::NestingLimitExceeded);
        }
    }

    #[test]
    fn canonical_artifact_digest_decodes_and_domain_frames_bytes() {
        let value = CanonicalValue::Map(vec![(text("answer"), CanonicalValue::Integer(42))]);
        let bytes = encode_canonical_cbor(&value).expect("artifact value encodes");
        let domain = "test.artifact/v1";
        let framed =
            CanonicalValue::Array(vec![text(CORE_DIGEST_FRAME), text(domain), value.clone()]);
        let expected =
            Sha256::digest(encode_canonical_cbor(&framed).expect("artifact digest frame encodes"));
        let decoded = decode_canonical_cbor(&bytes).expect("artifact bytes decode");

        assert_eq!(
            digest_canonical_value(domain, &decoded).expect("artifact value digest"),
            expected.as_slice()
        );
        assert_ne!(
            digest_canonical_value(domain, &decoded).expect("first domain digest"),
            digest_canonical_value("test.other/v1", &decoded).expect("second domain digest")
        );
    }

    #[test]
    fn canonical_artifact_digest_accepts_the_public_nesting_boundary() {
        let bytes = nested_array_bytes(MAX_CANONICAL_NESTING_DEPTH);
        let value = decode_canonical_cbor(&bytes).expect("maximum-depth artifact decodes");

        digest_canonical_value("test.maximum-depth/v1", &value)
            .expect("digest framing must not consume the artifact nesting budget");
    }

    #[test]
    fn canonical_artifact_decode_rejects_noncanonical_bytes() {
        let err = decode_canonical_cbor(&[0x18, 0x00])
            .expect_err("non-minimal canonical bytes reject before hashing");

        assert_eq!(err.kind(), CanonicalErrorKind::NonCanonical);
    }

    #[test]
    fn logical_package_relative_path_contract_is_shared() {
        for path in ["contracts/main.edict", "generated/review/report.cbor"] {
            assert!(is_logical_package_relative_path(path), "rejected {path:?}");
        }
        for path in [
            "",
            "/contracts/main.edict",
            "contracts//main.edict",
            "contracts/./main.edict",
            "contracts/../main.edict",
            "C:/contracts/main.edict",
            r"contracts\main.edict",
        ] {
            assert!(!is_logical_package_relative_path(path), "accepted {path:?}");
        }
    }
}

#[cfg(test)]
mod bundle_layer_digest_tests {
    use super::{
        digest_bundle_layer, BundleDigestDomain, BundlePreimageComponent, BundleSourceDescriptor,
        CanonicalErrorKind,
    };
    use crate::core_ir::ResourceRef;

    const A: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    const B: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
    const SEM: BundleDigestDomain = BundleDigestDomain::Semantic;

    fn resource(coordinate: &str, digest: &str) -> ResourceRef {
        ResourceRef {
            coordinate: coordinate.to_owned(),
            digest: Some(digest.to_owned()),
        }
    }

    fn bundle_error_kind(components: &[BundlePreimageComponent]) -> CanonicalErrorKind {
        digest_bundle_layer(SEM, components)
            .expect_err("bundle digest layer rejects malformed preimage")
            .kind()
    }

    #[test]
    fn invalid_digest_review_string_is_rejected() {
        assert_eq!(
            bundle_error_kind(&[BundlePreimageComponent::Digest("not-a-digest")]),
            CanonicalErrorKind::InvalidDigest
        );
    }

    #[test]
    fn uppercase_bundle_digest_review_string_is_rejected() {
        let uppercase = "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let uppercase_resource = resource("compiler.a@1", uppercase);

        assert_eq!(
            bundle_error_kind(&[BundlePreimageComponent::Digest(uppercase)]),
            CanonicalErrorKind::InvalidDigest
        );
        assert_eq!(
            bundle_error_kind(&[BundlePreimageComponent::Resource(&uppercase_resource)]),
            CanonicalErrorKind::InvalidDigest
        );
    }

    #[test]
    fn digest_is_deterministic() {
        let components = [
            BundlePreimageComponent::Digest(A),
            BundlePreimageComponent::Digest(B),
        ];
        assert_eq!(
            digest_bundle_layer(SEM, &components).expect("digest"),
            digest_bundle_layer(SEM, &components).expect("digest"),
        );
    }

    #[test]
    fn component_order_changes_the_digest() {
        let forward = digest_bundle_layer(
            SEM,
            &[
                BundlePreimageComponent::Digest(A),
                BundlePreimageComponent::Digest(B),
            ],
        )
        .expect("digest");
        let swapped = digest_bundle_layer(
            SEM,
            &[
                BundlePreimageComponent::Digest(B),
                BundlePreimageComponent::Digest(A),
            ],
        )
        .expect("digest");
        assert_ne!(forward, swapped);
    }

    #[test]
    fn domain_separates_semantic_from_release() {
        let component = [BundlePreimageComponent::Digest(A)];
        assert_ne!(
            digest_bundle_layer(BundleDigestDomain::Semantic, &component).expect("digest"),
            digest_bundle_layer(BundleDigestDomain::Release, &component).expect("digest"),
        );
    }

    #[test]
    fn list_structure_is_distinct_from_flattened_singles() {
        // [DigestList([A, B])] must not collide with [Digest(A), Digest(B)]: the
        // nesting is part of the byte-level contract.
        let nested = digest_bundle_layer(
            SEM,
            &[BundlePreimageComponent::DigestList(&[
                A.to_owned(),
                B.to_owned(),
            ])],
        )
        .expect("digest");
        let flat = digest_bundle_layer(
            SEM,
            &[
                BundlePreimageComponent::Digest(A),
                BundlePreimageComponent::Digest(B),
            ],
        )
        .expect("digest");
        assert_ne!(nested, flat);
    }

    #[test]
    fn resource_coordinate_change_moves_digest_with_same_digest() {
        // The whole point of binding the coordinate: a toolchain identity change
        // (compiler/lowerer/verifier coordinate) with the same artifact digest
        // must still move the bundle digest.
        let one = resource("compiler.a@1", A);
        let two = resource("compiler.b@1", A);
        assert_ne!(
            digest_bundle_layer(SEM, &[BundlePreimageComponent::Resource(&one)]).expect("digest"),
            digest_bundle_layer(SEM, &[BundlePreimageComponent::Resource(&two)]).expect("digest"),
        );
    }

    #[test]
    fn resource_preimage_shape_matches_checked_digest() {
        let compiler = resource("compiler.a@1", A);
        let digest = digest_bundle_layer(SEM, &[BundlePreimageComponent::Resource(&compiler)])
            .expect("digest");

        assert_eq!(
            digest.to_review_string(),
            "sha256:affd17f8c86b66b0109d6ae373cb35888be8138f56df109ae21445284267bd1b"
        );
    }

    #[test]
    fn resource_without_digest_is_rejected() {
        // Hash-significant bundle references must be digest-locked; a missing
        // digest is an error, not a silently-distinct preimage.
        let without = ResourceRef {
            coordinate: "compiler.a@1".to_owned(),
            digest: None,
        };
        assert_eq!(
            bundle_error_kind(&[BundlePreimageComponent::Resource(&without)]),
            CanonicalErrorKind::UnresolvedDigest
        );
    }

    #[test]
    fn source_logical_path_change_moves_digest_with_same_artifact() {
        // Provenance: a logical source path change must move the (release) bundle
        // digest even when the source artifact digest is unchanged.
        let artifact = resource("src", A);
        let first = [BundleSourceDescriptor {
            logical_path: "a/main.edict",
            artifact: &artifact,
        }];
        let second = [BundleSourceDescriptor {
            logical_path: "b/main.edict",
            artifact: &artifact,
        }];
        assert_ne!(
            digest_bundle_layer(SEM, &[BundlePreimageComponent::SourceArtifacts(&first)])
                .expect("digest"),
            digest_bundle_layer(SEM, &[BundlePreimageComponent::SourceArtifacts(&second)])
                .expect("digest"),
        );
    }

    #[test]
    fn source_artifact_logical_paths_are_rejected() {
        let artifact = resource("src", A);
        for logical_path in [
            "",
            "/contracts/main.edict",
            "contracts/../main.edict",
            "contracts/./main.edict",
            "C:/contracts/main.edict",
            r"contracts\main.edict",
        ] {
            let descriptors = [BundleSourceDescriptor {
                logical_path,
                artifact: &artifact,
            }];

            assert_eq!(
                bundle_error_kind(&[BundlePreimageComponent::SourceArtifacts(&descriptors)]),
                CanonicalErrorKind::UnsupportedValue,
                "accepted invalid source artifact path {logical_path:?}"
            );
        }
    }
}
