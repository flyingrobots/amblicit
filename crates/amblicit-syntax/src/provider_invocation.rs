//! Pure validation for target-provider invocation values.
//!
//! This module mirrors the runtime-neutral provider WIT envelope with owned
//! Rust values. Validation is deliberately separated from component loading or
//! execution: a future host can only obtain a validated request wrapper after
//! the host-authored input contract, canonical bytes, domains, and digests all
//! agree. Provider responses are validated before any output identity is
//! exposed.

use std::any::Any;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::authority_facts::AUTHORITY_FACTS_API_VERSION;
use crate::canonical::{
    decode_canonical_cbor, digest_canonical_value, is_logical_package_relative_path,
    CanonicalValue, CORE_MODULE_DIGEST_DOMAIN, TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
};
use crate::target_profile::TARGET_PROFILE_API_VERSION;

/// Semantic protocol version accepted by the provider invocation validator.
pub const TARGET_PROVIDER_PROTOCOL_VERSION: ProviderProtocolVersion =
    ProviderProtocolVersion::V1_0_0;

/// Canonical lawpack artifact domain owned by the Edict lawpack ABI.
pub const PROVIDER_LAWPACK_ARTIFACT_DOMAIN: &str = "edict.lawpack/v1";

/// WIT-shaped provider protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderProtocolVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl ProviderProtocolVersion {
    pub const V1_0_0: Self = Self {
        major: 1,
        minor: 0,
        patch: 0,
    };
}

/// Digest algorithms admitted by the provider ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDigestAlgorithm {
    Sha256,
}

/// WIT-shaped typed digest using raw bytes rather than review rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDigest {
    pub algorithm: ProviderDigestAlgorithm,
    pub bytes: Vec<u8>,
}

/// Digest-bound resource identity carried across the provider ABI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResourceRef {
    pub coordinate: String,
    pub digest: ProviderDigest,
}

/// Opaque artifact bytes claimed under an explicit schema domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderArtifact {
    pub domain: String,
    pub bytes: Vec<u8>,
}

/// Stable failures returned by a host-owned artifact schema validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderArtifactSchemaValidationErrorKind {
    /// No canonical schema is registered for the declared domain.
    UnsupportedDomain,
    /// The canonical value is not an instance of the domain's owning schema.
    SchemaMismatch,
}

/// Explicit host-owned validation for versioned artifact-domain schemas.
///
/// Implementations are trusted host configuration, not provider callbacks.
/// Their contract requires total, deterministic behavior over the supplied
/// in-memory value and forbids discovery, I/O, clock, random, environment, and
/// component operations. Implementations must also be safe to share across
/// independent concurrent provider stores. Rust cannot enforce the effect
/// restrictions for an arbitrary implementation; the concrete host registry
/// must provide that evidence. The invocation validator establishes canonical
/// CBOR before this hook and uses the accepted value for host-side digest
/// computation.
pub trait ProviderArtifactSchemaValidator: Any + fmt::Debug + Send + Sync {
    /// Return whether an owning schema is available for `domain`.
    fn supports_domain(&self, domain: &str) -> bool;

    /// Validate one decoded canonical value against its owning domain schema.
    ///
    /// # Errors
    ///
    /// Returns a stable unsupported-domain or invalid-artifact classification.
    fn validate_canonical_value(
        &self,
        domain: &str,
        value: &CanonicalValue,
    ) -> Result<(), ProviderArtifactSchemaValidationErrorKind>;
}

/// Artifact bytes paired with the resource identity they must reproduce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderBoundArtifact {
    pub reference: ProviderResourceRef,
    pub artifact: ProviderArtifact,
}

/// Host-authored expected identity for one invocation input artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderArtifactBinding {
    pub reference: ProviderResourceRef,
    pub domain: String,
}

/// Runtime-neutral semantic input routing categories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderSemanticInputKind {
    Lawpack,
    AuthorityFacts,
    LowerabilityFacts,
    Auxiliary(String),
}

/// One semantic artifact sent to a provider component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSemanticInput {
    pub role: String,
    pub kind: ProviderSemanticInputKind,
    pub artifact: ProviderBoundArtifact,
}

/// Host-authored expected semantic input closure entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSemanticInputBinding {
    pub role: String,
    pub kind: ProviderSemanticInputKind,
    pub artifact: ProviderArtifactBinding,
}

/// Host-authored complete input contract for one lowering request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLoweringInvocationContract {
    pub core: ProviderArtifactBinding,
    pub target_profile: ProviderArtifactBinding,
    pub semantic_inputs: Vec<ProviderSemanticInputBinding>,
}

/// Host-authored complete input contract for one verification request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderVerificationInvocationContract {
    pub core: ProviderArtifactBinding,
    pub target_profile: ProviderArtifactBinding,
    pub target_ir: ProviderArtifactBinding,
    pub semantic_inputs: Vec<ProviderSemanticInputBinding>,
}

/// Output authority available to a lowerer component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLoweringOutputKind {
    TargetIr,
    GeneratedArtifact,
    ReviewPayload,
}

/// One output role requested from a lowerer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLoweringOutputRequest {
    pub role: String,
    pub kind: ProviderLoweringOutputKind,
    pub domain: String,
}

/// One output returned by a lowerer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLoweringOutputArtifact {
    pub role: String,
    pub kind: ProviderLoweringOutputKind,
    pub artifact: ProviderArtifact,
    pub logical_path: Option<String>,
}

/// Output authority available to a verifier component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderVerificationOutputKind {
    VerifierReport,
}

/// One output role requested from a verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderVerificationOutputRequest {
    pub role: String,
    pub kind: ProviderVerificationOutputKind,
    pub domain: String,
}

/// One output returned by a verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderVerificationOutputArtifact {
    pub role: String,
    pub kind: ProviderVerificationOutputKind,
    pub artifact: ProviderArtifact,
    pub logical_path: Option<String>,
}

/// Deterministic bounds applied to either provider result arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderResponseLimits {
    pub max_output_count: u32,
    pub max_diagnostic_count: u32,
    pub max_total_response_bytes: u64,
}

/// Provider diagnostic severity in WIT declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

impl ProviderDiagnosticSeverity {
    const fn declaration_index(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
}

/// Structured provider-authored diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDiagnostic {
    pub code: String,
    pub severity: ProviderDiagnosticSeverity,
    pub message: String,
    pub repair: Option<String>,
}

/// Target-owned semantic refusal categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRefusalKind {
    UnsupportedCoreAbi,
    UnsupportedTargetProfile,
    UnsupportedSemantics,
    UnsupportedOutputRole,
    InvalidSemanticArtifact,
}

/// A valid provider refusal remains distinct from host envelope failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRefusal {
    pub kind: ProviderRefusalKind,
    pub subject: Option<String>,
    pub diagnostics: Vec<ProviderDiagnostic>,
}

/// Complete lowerer invocation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLoweringRequest {
    pub protocol_version: ProviderProtocolVersion,
    pub core: ProviderBoundArtifact,
    pub target_profile: ProviderBoundArtifact,
    pub semantic_inputs: Vec<ProviderSemanticInput>,
    pub requested_outputs: Vec<ProviderLoweringOutputRequest>,
    pub limits: ProviderResponseLimits,
}

/// Complete verifier invocation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderVerificationRequest {
    pub protocol_version: ProviderProtocolVersion,
    pub core: ProviderBoundArtifact,
    pub target_profile: ProviderBoundArtifact,
    pub target_ir: ProviderBoundArtifact,
    pub semantic_inputs: Vec<ProviderSemanticInput>,
    pub requested_outputs: Vec<ProviderVerificationOutputRequest>,
    pub limits: ProviderResponseLimits,
}

/// Successful lowerer response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLoweringSuccess {
    pub outputs: Vec<ProviderLoweringOutputArtifact>,
    pub diagnostics: Vec<ProviderDiagnostic>,
}

/// Successful verifier response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderVerificationSuccess {
    pub outputs: Vec<ProviderVerificationOutputArtifact>,
    pub diagnostics: Vec<ProviderDiagnostic>,
}

pub type ProviderLoweringResult = Result<ProviderLoweringSuccess, ProviderRefusal>;
pub type ProviderVerificationResult = Result<ProviderVerificationSuccess, ProviderRefusal>;

/// Provider world used to produce one trusted output manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderInvocationKind {
    Lowering,
    Verification,
}

/// Inputs bound into a host-authored output manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInvocationInputManifest {
    core: ProviderArtifactBinding,
    target_profile: ProviderArtifactBinding,
    target_ir: Option<ProviderArtifactBinding>,
    semantic_inputs: Vec<ProviderSemanticInputBinding>,
}

impl ProviderInvocationInputManifest {
    #[must_use]
    pub const fn core(&self) -> &ProviderArtifactBinding {
        &self.core
    }

    #[must_use]
    pub const fn target_profile(&self) -> &ProviderArtifactBinding {
        &self.target_profile
    }

    #[must_use]
    pub const fn target_ir(&self) -> Option<&ProviderArtifactBinding> {
        self.target_ir.as_ref()
    }

    #[must_use]
    pub fn semantic_inputs(&self) -> &[ProviderSemanticInputBinding] {
        &self.semantic_inputs
    }
}

/// Trusted copy of one requested output role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOutputRequestBinding<K> {
    pub role: String,
    pub kind: K,
    pub domain: String,
}

/// Host-computed identity and routing metadata for one valid output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOutputManifestEntry<K> {
    pub role: String,
    pub kind: K,
    pub domain: String,
    pub digest: ProviderDigest,
    pub logical_path: Option<String>,
}

/// Host-authoritative manifest emitted only for a completely valid success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOutputManifest<K> {
    invocation: ProviderInvocationKind,
    protocol_version: ProviderProtocolVersion,
    inputs: ProviderInvocationInputManifest,
    requested_outputs: Vec<ProviderOutputRequestBinding<K>>,
    outputs: Vec<ProviderOutputManifestEntry<K>>,
}

impl<K> ProviderOutputManifest<K> {
    #[must_use]
    pub const fn invocation(&self) -> ProviderInvocationKind {
        self.invocation
    }

    #[must_use]
    pub const fn protocol_version(&self) -> ProviderProtocolVersion {
        self.protocol_version
    }

    #[must_use]
    pub const fn inputs(&self) -> &ProviderInvocationInputManifest {
        &self.inputs
    }

    #[must_use]
    pub fn requested_outputs(&self) -> &[ProviderOutputRequestBinding<K>] {
        &self.requested_outputs
    }

    #[must_use]
    pub fn outputs(&self) -> &[ProviderOutputManifestEntry<K>] {
        &self.outputs
    }
}

/// Opaque trusted provider result.
///
/// Callers can inspect but cannot construct or mutate this proof. A refusal is
/// valid target-owned evidence but never carries an output manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedProviderOutcome<S, K> {
    inner: ValidatedProviderOutcomeKind<S, K>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ValidatedProviderOutcomeKind<S, K> {
    Success {
        response: S,
        manifest: Box<ProviderOutputManifest<K>>,
    },
    Refusal(ProviderRefusal),
}

impl<S, K> ValidatedProviderOutcome<S, K> {
    fn success(response: S, manifest: ProviderOutputManifest<K>) -> Self {
        Self {
            inner: ValidatedProviderOutcomeKind::Success {
                response,
                manifest: Box::new(manifest),
            },
        }
    }

    fn from_refusal(refusal: ProviderRefusal) -> Self {
        Self {
            inner: ValidatedProviderOutcomeKind::Refusal(refusal),
        }
    }

    /// Return the validated success response, or `None` for provider refusal.
    #[must_use]
    pub const fn response(&self) -> Option<&S> {
        match &self.inner {
            ValidatedProviderOutcomeKind::Success { response, .. } => Some(response),
            ValidatedProviderOutcomeKind::Refusal(_) => None,
        }
    }

    /// Return the host-authored manifest, or `None` for provider refusal.
    #[must_use]
    pub fn manifest(&self) -> Option<&ProviderOutputManifest<K>> {
        match &self.inner {
            ValidatedProviderOutcomeKind::Success { manifest, .. } => Some(manifest),
            ValidatedProviderOutcomeKind::Refusal(_) => None,
        }
    }

    /// Return the validated provider refusal, or `None` for success.
    #[must_use]
    pub const fn refusal(&self) -> Option<&ProviderRefusal> {
        match &self.inner {
            ValidatedProviderOutcomeKind::Success { .. } => None,
            ValidatedProviderOutcomeKind::Refusal(refusal) => Some(refusal),
        }
    }
}

/// Overall invocation validation classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderInvocationValidationStatus {
    Valid,
    Invalid,
}

/// Stable host-owned provider invocation failure categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderInvocationValidationFailureKind {
    UnsupportedProtocolVersion,
    EmptyResourceCoordinate,
    MalformedDigest,
    ArtifactReferenceMismatch,
    ArtifactDigestMismatch,
    MissingArtifactDomain,
    ArtifactDomainMismatch,
    UnsupportedArtifactDomain,
    ArtifactSchemaMismatch,
    NonCanonicalArtifact,
    EmptyRole,
    DuplicateRole,
    OutOfOrderRole,
    MissingSemanticInput,
    UndeclaredSemanticInput,
    SemanticInputBindingMismatch,
    MissingRequestedOutput,
    UndeclaredOutput,
    OutputKindMismatch,
    InvalidLogicalPath,
    DuplicateLogicalPath,
    DuplicateDiagnostic,
    OutOfOrderDiagnostic,
    OutputCountLimitExceeded,
    DiagnosticCountLimitExceeded,
    ResponseByteCountOverflow,
    ResponseByteLimitExceeded,
    NonComparableRequests,
    LimitDependentResult,
}

/// One failed invocation validation obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInvocationValidationFailure {
    pub kind: ProviderInvocationValidationFailureKind,
    pub field: String,
    pub role: Option<String>,
    pub obligation: String,
}

/// Deterministic aggregate report for host-owned validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInvocationValidationReport {
    pub status: ProviderInvocationValidationStatus,
    pub failures: Vec<ProviderInvocationValidationFailure>,
}

impl ProviderInvocationValidationReport {
    fn from_failures(failures: Vec<ProviderInvocationValidationFailure>) -> Self {
        Self {
            status: if failures.is_empty() {
                ProviderInvocationValidationStatus::Valid
            } else {
                ProviderInvocationValidationStatus::Invalid
            },
            failures,
        }
    }
}

/// Opaque proof that a lowering request passed host-owned validation.
#[derive(Debug, Clone, Copy)]
pub struct ValidatedProviderLoweringRequest<'a> {
    schema_validator: &'a dyn ProviderArtifactSchemaValidator,
    contract: &'a ProviderLoweringInvocationContract,
    request: &'a ProviderLoweringRequest,
}

impl<'a> ValidatedProviderLoweringRequest<'a> {
    #[must_use]
    pub const fn schema_validator(&self) -> &'a dyn ProviderArtifactSchemaValidator {
        self.schema_validator
    }

    #[must_use]
    pub const fn contract(&self) -> &'a ProviderLoweringInvocationContract {
        self.contract
    }

    #[must_use]
    pub const fn request(&self) -> &'a ProviderLoweringRequest {
        self.request
    }
}

/// Opaque proof that a verification request passed host-owned validation.
#[derive(Debug, Clone, Copy)]
pub struct ValidatedProviderVerificationRequest<'a> {
    schema_validator: &'a dyn ProviderArtifactSchemaValidator,
    contract: &'a ProviderVerificationInvocationContract,
    request: &'a ProviderVerificationRequest,
}

impl<'a> ValidatedProviderVerificationRequest<'a> {
    #[must_use]
    pub const fn schema_validator(&self) -> &'a dyn ProviderArtifactSchemaValidator {
        self.schema_validator
    }

    #[must_use]
    pub const fn contract(&self) -> &'a ProviderVerificationInvocationContract {
        self.contract
    }

    #[must_use]
    pub const fn request(&self) -> &'a ProviderVerificationRequest {
        self.request
    }
}

/// Validate a host-authored lowering contract and its WIT-shaped request.
///
/// # Errors
///
/// Returns a structured report when the contract or request violates any
/// protocol, closure, ordering, domain, canonicality, or digest obligation.
pub fn validate_provider_lowering_request<'a>(
    schema_validator: &'a dyn ProviderArtifactSchemaValidator,
    contract: &'a ProviderLoweringInvocationContract,
    request: &'a ProviderLoweringRequest,
) -> Result<ValidatedProviderLoweringRequest<'a>, ProviderInvocationValidationReport> {
    let mut failures = Vec::new();
    check_protocol(request.protocol_version, &mut failures);
    check_contract_binding(
        &contract.core,
        "contract.core",
        None,
        Some(CORE_MODULE_DIGEST_DOMAIN),
        &mut failures,
    );
    check_contract_binding(
        &contract.target_profile,
        "contract.target_profile",
        None,
        Some(TARGET_PROFILE_API_VERSION),
        &mut failures,
    );
    check_semantic_contract(&contract.semantic_inputs, &mut failures);
    check_bound_artifact(
        schema_validator,
        &request.core,
        &contract.core,
        "core",
        None,
        &mut failures,
    );
    check_bound_artifact(
        schema_validator,
        &request.target_profile,
        &contract.target_profile,
        "target_profile",
        None,
        &mut failures,
    );
    check_semantic_inputs(
        schema_validator,
        &contract.semantic_inputs,
        &request.semantic_inputs,
        &mut failures,
    );
    check_lowering_output_requests(schema_validator, &request.requested_outputs, &mut failures);

    if failures.is_empty() {
        Ok(ValidatedProviderLoweringRequest {
            schema_validator,
            contract,
            request,
        })
    } else {
        Err(ProviderInvocationValidationReport::from_failures(failures))
    }
}

/// Validate a host-authored verification contract and its WIT-shaped request.
///
/// # Errors
///
/// Returns a structured report when the contract or request violates any
/// protocol, closure, ordering, domain, canonicality, or digest obligation.
pub fn validate_provider_verification_request<'a>(
    schema_validator: &'a dyn ProviderArtifactSchemaValidator,
    contract: &'a ProviderVerificationInvocationContract,
    request: &'a ProviderVerificationRequest,
) -> Result<ValidatedProviderVerificationRequest<'a>, ProviderInvocationValidationReport> {
    let mut failures = Vec::new();
    check_protocol(request.protocol_version, &mut failures);
    check_contract_binding(
        &contract.core,
        "contract.core",
        None,
        Some(CORE_MODULE_DIGEST_DOMAIN),
        &mut failures,
    );
    check_contract_binding(
        &contract.target_profile,
        "contract.target_profile",
        None,
        Some(TARGET_PROFILE_API_VERSION),
        &mut failures,
    );
    check_contract_binding(
        &contract.target_ir,
        "contract.target_ir",
        None,
        Some(TARGET_IR_ARTIFACT_DIGEST_DOMAIN),
        &mut failures,
    );
    check_semantic_contract(&contract.semantic_inputs, &mut failures);
    check_bound_artifact(
        schema_validator,
        &request.core,
        &contract.core,
        "core",
        None,
        &mut failures,
    );
    check_bound_artifact(
        schema_validator,
        &request.target_profile,
        &contract.target_profile,
        "target_profile",
        None,
        &mut failures,
    );
    check_bound_artifact(
        schema_validator,
        &request.target_ir,
        &contract.target_ir,
        "target_ir",
        None,
        &mut failures,
    );
    check_semantic_inputs(
        schema_validator,
        &contract.semantic_inputs,
        &request.semantic_inputs,
        &mut failures,
    );
    check_verification_output_requests(schema_validator, &request.requested_outputs, &mut failures);

    if failures.is_empty() {
        Ok(ValidatedProviderVerificationRequest {
            schema_validator,
            contract,
            request,
        })
    } else {
        Err(ProviderInvocationValidationReport::from_failures(failures))
    }
}

/// Validate a lowerer result and construct host-owned output identity.
///
/// # Errors
///
/// Returns a structured host failure report for malformed, unbound,
/// noncanonical, out-of-order, limit-exceeding, or otherwise invalid results.
pub fn validate_provider_lowering_result(
    validated: &ValidatedProviderLoweringRequest<'_>,
    result: &ProviderLoweringResult,
) -> Result<
    ValidatedProviderOutcome<ProviderLoweringSuccess, ProviderLoweringOutputKind>,
    ProviderInvocationValidationReport,
> {
    validate_lowering_result_for(
        validated.schema_validator,
        validated.contract,
        validated.request,
        result,
        ProviderInvocationKind::Lowering,
    )
}

/// Validate a verifier result and construct host-owned output identity.
///
/// # Errors
///
/// Returns a structured host failure report for malformed, unbound,
/// noncanonical, out-of-order, limit-exceeding, or otherwise invalid results.
pub fn validate_provider_verification_result(
    validated: &ValidatedProviderVerificationRequest<'_>,
    result: &ProviderVerificationResult,
) -> Result<
    ValidatedProviderOutcome<ProviderVerificationSuccess, ProviderVerificationOutputKind>,
    ProviderInvocationValidationReport,
> {
    validate_verification_result_for(
        validated.schema_validator,
        validated.contract,
        validated.request,
        result,
        ProviderInvocationKind::Verification,
    )
}

/// Compare two otherwise-identical lowering observations across limit changes.
#[must_use]
pub fn validate_provider_lowering_limit_independence(
    first_request: &ValidatedProviderLoweringRequest<'_>,
    first_result: &ProviderLoweringResult,
    second_request: &ValidatedProviderLoweringRequest<'_>,
    second_result: &ProviderLoweringResult,
) -> ProviderInvocationValidationReport {
    let mut failures = Vec::new();
    if first_request.contract != second_request.contract
        || !same_lowering_request_except_limits(first_request.request, second_request.request)
    {
        push_failure(
            &mut failures,
            ProviderInvocationValidationFailureKind::NonComparableRequests,
            "requests",
            None,
            "identical lowering contracts and requests except response limits",
        );
        return ProviderInvocationValidationReport::from_failures(failures);
    }

    let baseline_under_first = validate_lowering_result_for(
        first_request.schema_validator,
        first_request.contract,
        first_request.request,
        first_result,
        ProviderInvocationKind::Lowering,
    );
    let mut baseline_second_request = first_request.request.clone();
    baseline_second_request.limits = second_request.request.limits;
    let baseline_under_second = validate_lowering_result_for(
        second_request.schema_validator,
        second_request.contract,
        &baseline_second_request,
        first_result,
        ProviderInvocationKind::Lowering,
    );
    let candidate_under_second = validate_lowering_result_for(
        second_request.schema_validator,
        second_request.contract,
        second_request.request,
        second_result,
        ProviderInvocationKind::Lowering,
    );

    let baseline_fits_both = baseline_under_first.is_ok() && baseline_under_second.is_ok();
    extend_validation_failures(&mut failures, baseline_under_first);
    extend_validation_failures(&mut failures, baseline_under_second);
    extend_validation_failures(&mut failures, candidate_under_second);
    if baseline_fits_both && first_result != second_result {
        push_failure(
            &mut failures,
            ProviderInvocationValidationFailureKind::LimitDependentResult,
            "results",
            None,
            "byte-identical complete result when the baseline fits both limit sets",
        );
    }
    ProviderInvocationValidationReport::from_failures(failures)
}

/// Compare two otherwise-identical verification observations across limit changes.
#[must_use]
pub fn validate_provider_verification_limit_independence(
    first_request: &ValidatedProviderVerificationRequest<'_>,
    first_result: &ProviderVerificationResult,
    second_request: &ValidatedProviderVerificationRequest<'_>,
    second_result: &ProviderVerificationResult,
) -> ProviderInvocationValidationReport {
    let mut failures = Vec::new();
    if first_request.contract != second_request.contract
        || !same_verification_request_except_limits(first_request.request, second_request.request)
    {
        push_failure(
            &mut failures,
            ProviderInvocationValidationFailureKind::NonComparableRequests,
            "requests",
            None,
            "identical verification contracts and requests except response limits",
        );
        return ProviderInvocationValidationReport::from_failures(failures);
    }

    let baseline_under_first = validate_verification_result_for(
        first_request.schema_validator,
        first_request.contract,
        first_request.request,
        first_result,
        ProviderInvocationKind::Verification,
    );
    let mut baseline_second_request = first_request.request.clone();
    baseline_second_request.limits = second_request.request.limits;
    let baseline_under_second = validate_verification_result_for(
        second_request.schema_validator,
        second_request.contract,
        &baseline_second_request,
        first_result,
        ProviderInvocationKind::Verification,
    );
    let candidate_under_second = validate_verification_result_for(
        second_request.schema_validator,
        second_request.contract,
        second_request.request,
        second_result,
        ProviderInvocationKind::Verification,
    );

    let baseline_fits_both = baseline_under_first.is_ok() && baseline_under_second.is_ok();
    extend_validation_failures(&mut failures, baseline_under_first);
    extend_validation_failures(&mut failures, baseline_under_second);
    extend_validation_failures(&mut failures, candidate_under_second);
    if baseline_fits_both && first_result != second_result {
        push_failure(
            &mut failures,
            ProviderInvocationValidationFailureKind::LimitDependentResult,
            "results",
            None,
            "byte-identical complete result when the baseline fits both limit sets",
        );
    }
    ProviderInvocationValidationReport::from_failures(failures)
}

fn validate_lowering_result_for(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    contract: &ProviderLoweringInvocationContract,
    request: &ProviderLoweringRequest,
    result: &ProviderLoweringResult,
    invocation: ProviderInvocationKind,
) -> Result<
    ValidatedProviderOutcome<ProviderLoweringSuccess, ProviderLoweringOutputKind>,
    ProviderInvocationValidationReport,
> {
    match result {
        Ok(success) => {
            let mut failures = Vec::new();
            let requests = request
                .requested_outputs
                .iter()
                .map(|output| OutputRequestView {
                    role: &output.role,
                    kind: output.kind,
                    domain: &output.domain,
                })
                .collect::<Vec<_>>();
            let outputs = success
                .outputs
                .iter()
                .map(|output| OutputView {
                    role: &output.role,
                    kind: output.kind,
                    artifact: &output.artifact,
                    logical_path: output.logical_path.as_deref(),
                })
                .collect::<Vec<_>>();
            let manifest_outputs = check_success(
                schema_validator,
                &requests,
                &outputs,
                &success.diagnostics,
                request.limits,
                &mut failures,
            );
            if failures.is_empty() {
                Ok(ValidatedProviderOutcome::success(
                    success.clone(),
                    ProviderOutputManifest {
                        invocation,
                        protocol_version: request.protocol_version,
                        inputs: ProviderInvocationInputManifest {
                            core: contract.core.clone(),
                            target_profile: contract.target_profile.clone(),
                            target_ir: None,
                            semantic_inputs: contract.semantic_inputs.clone(),
                        },
                        requested_outputs: request
                            .requested_outputs
                            .iter()
                            .map(|output| ProviderOutputRequestBinding {
                                role: output.role.clone(),
                                kind: output.kind,
                                domain: output.domain.clone(),
                            })
                            .collect(),
                        outputs: manifest_outputs,
                    },
                ))
            } else {
                Err(ProviderInvocationValidationReport::from_failures(failures))
            }
        }
        Err(refusal) => validate_refusal(refusal, request.limits),
    }
}

fn validate_verification_result_for(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    contract: &ProviderVerificationInvocationContract,
    request: &ProviderVerificationRequest,
    result: &ProviderVerificationResult,
    invocation: ProviderInvocationKind,
) -> Result<
    ValidatedProviderOutcome<ProviderVerificationSuccess, ProviderVerificationOutputKind>,
    ProviderInvocationValidationReport,
> {
    match result {
        Ok(success) => {
            let mut failures = Vec::new();
            let requests = request
                .requested_outputs
                .iter()
                .map(|output| OutputRequestView {
                    role: &output.role,
                    kind: output.kind,
                    domain: &output.domain,
                })
                .collect::<Vec<_>>();
            let outputs = success
                .outputs
                .iter()
                .map(|output| OutputView {
                    role: &output.role,
                    kind: output.kind,
                    artifact: &output.artifact,
                    logical_path: output.logical_path.as_deref(),
                })
                .collect::<Vec<_>>();
            let manifest_outputs = check_success(
                schema_validator,
                &requests,
                &outputs,
                &success.diagnostics,
                request.limits,
                &mut failures,
            );
            if failures.is_empty() {
                Ok(ValidatedProviderOutcome::success(
                    success.clone(),
                    ProviderOutputManifest {
                        invocation,
                        protocol_version: request.protocol_version,
                        inputs: ProviderInvocationInputManifest {
                            core: contract.core.clone(),
                            target_profile: contract.target_profile.clone(),
                            target_ir: Some(contract.target_ir.clone()),
                            semantic_inputs: contract.semantic_inputs.clone(),
                        },
                        requested_outputs: request
                            .requested_outputs
                            .iter()
                            .map(|output| ProviderOutputRequestBinding {
                                role: output.role.clone(),
                                kind: output.kind,
                                domain: output.domain.clone(),
                            })
                            .collect(),
                        outputs: manifest_outputs,
                    },
                ))
            } else {
                Err(ProviderInvocationValidationReport::from_failures(failures))
            }
        }
        Err(refusal) => validate_refusal(refusal, request.limits),
    }
}

fn validate_refusal<S, K>(
    refusal: &ProviderRefusal,
    limits: ProviderResponseLimits,
) -> Result<ValidatedProviderOutcome<S, K>, ProviderInvocationValidationReport> {
    let mut failures = Vec::new();
    check_diagnostics(&refusal.diagnostics, &mut failures);
    let mut byte_lengths = Vec::new();
    if let Some(subject) = &refusal.subject {
        byte_lengths.push(subject.len());
    }
    diagnostic_byte_lengths(&refusal.diagnostics, &mut byte_lengths);
    check_response_limits(
        limits,
        0,
        refusal.diagnostics.len(),
        &byte_lengths,
        &mut failures,
    );
    if failures.is_empty() {
        Ok(ValidatedProviderOutcome::from_refusal(refusal.clone()))
    } else {
        Err(ProviderInvocationValidationReport::from_failures(failures))
    }
}

#[derive(Clone, Copy)]
struct OutputRequestView<'a, K> {
    role: &'a str,
    kind: K,
    domain: &'a str,
}

#[derive(Clone, Copy)]
struct OutputView<'a, K> {
    role: &'a str,
    kind: K,
    artifact: &'a ProviderArtifact,
    logical_path: Option<&'a str>,
}

fn check_success<K: Copy + Eq>(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    requests: &[OutputRequestView<'_, K>],
    outputs: &[OutputView<'_, K>],
    diagnostics: &[ProviderDiagnostic],
    limits: ProviderResponseLimits,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) -> Vec<ProviderOutputManifestEntry<K>> {
    check_role_order(
        requests.iter().map(|request| request.role),
        "requested_outputs.role",
        failures,
    );
    for request in requests {
        if request.domain.is_empty() {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::MissingArtifactDomain,
                "requested_outputs.domain",
                Some(request.role),
                "non-empty requested output artifact domain",
            );
        }
    }
    check_role_order(
        outputs.iter().map(|output| output.role),
        "outputs.role",
        failures,
    );

    let request_by_role = requests
        .iter()
        .map(|request| (request.role, request))
        .collect::<BTreeMap<_, _>>();
    let output_by_role = outputs
        .iter()
        .map(|output| (output.role, output))
        .collect::<BTreeMap<_, _>>();

    for request in requests {
        if !output_by_role.contains_key(request.role) {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::MissingRequestedOutput,
                "outputs.role",
                Some(request.role),
                "exactly one returned output for every requested role",
            );
        }
    }

    let mut paths = BTreeSet::new();
    for output in outputs {
        let Some(request) = request_by_role.get(output.role) else {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::UndeclaredOutput,
                "outputs.role",
                Some(output.role),
                "returned output role declared by the request",
            );
            check_output_metadata(output, &mut paths, failures);
            continue;
        };
        if output.kind != request.kind {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::OutputKindMismatch,
                "outputs.kind",
                Some(output.role),
                "returned output kind equal to its requested kind",
            );
        }
        if output.artifact.domain != request.domain {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::ArtifactDomainMismatch,
                "outputs.artifact.domain",
                Some(output.role),
                "returned output domain equal to its requested domain",
            );
        }
        check_output_metadata(output, &mut paths, failures);
    }

    check_diagnostics(diagnostics, failures);
    let mut byte_lengths = Vec::new();
    for output in outputs {
        byte_lengths.push(output.role.len());
        byte_lengths.push(output.artifact.domain.len());
        byte_lengths.push(output.artifact.bytes.len());
        if let Some(path) = output.logical_path {
            byte_lengths.push(path.len());
        }
    }
    diagnostic_byte_lengths(diagnostics, &mut byte_lengths);
    check_response_limits(
        limits,
        outputs.len(),
        diagnostics.len(),
        &byte_lengths,
        failures,
    );

    if !failures.is_empty() {
        return Vec::new();
    }
    let mut manifest = Vec::with_capacity(outputs.len());
    for output in outputs {
        digest_output_artifact(schema_validator, output, failures, &mut manifest);
    }
    manifest
}

fn check_output_metadata<'a, K>(
    output: &OutputView<'a, K>,
    paths: &mut BTreeSet<&'a str>,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    if output.artifact.domain.is_empty() {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::MissingArtifactDomain,
            "outputs.artifact.domain",
            Some(output.role),
            "non-empty returned artifact domain",
        );
    }
    if let Some(path) = output.logical_path {
        if !is_logical_package_relative_path(path) {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::InvalidLogicalPath,
                "outputs.logical_path",
                Some(output.role),
                "non-empty package-relative path using safe forward-slash segments",
            );
        } else if !paths.insert(path) {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::DuplicateLogicalPath,
                "outputs.logical_path",
                Some(output.role),
                "unique exact case-sensitive logical output path",
            );
        }
    }
}

fn digest_output_artifact<K: Copy>(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    output: &OutputView<'_, K>,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
    manifest: &mut Vec<ProviderOutputManifestEntry<K>>,
) {
    match validate_artifact_identity(schema_validator, output.artifact) {
        Ok(bytes) => manifest.push(ProviderOutputManifestEntry {
            role: output.role.to_owned(),
            kind: output.kind,
            domain: output.artifact.domain.clone(),
            digest: ProviderDigest {
                algorithm: ProviderDigestAlgorithm::Sha256,
                bytes: bytes.to_vec(),
            },
            logical_path: output.logical_path.map(str::to_owned),
        }),
        Err(error) => {
            push_artifact_validation_failure(
                failures,
                error,
                "outputs.artifact",
                Some(output.role),
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactValidationError {
    NonCanonical,
    UnsupportedDomain,
    SchemaMismatch,
}

fn validate_artifact_identity(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    artifact: &ProviderArtifact,
) -> Result<[u8; 32], ArtifactValidationError> {
    let value = decode_canonical_cbor(&artifact.bytes)
        .map_err(|_| ArtifactValidationError::NonCanonical)?;
    if !schema_validator.supports_domain(&artifact.domain) {
        return Err(ArtifactValidationError::UnsupportedDomain);
    }
    schema_validator
        .validate_canonical_value(&artifact.domain, &value)
        .map_err(|error| match error {
            ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain => {
                ArtifactValidationError::UnsupportedDomain
            }
            ProviderArtifactSchemaValidationErrorKind::SchemaMismatch => {
                ArtifactValidationError::SchemaMismatch
            }
        })?;
    digest_canonical_value(&artifact.domain, &value)
        .map_err(|_| ArtifactValidationError::NonCanonical)
}

fn push_artifact_validation_failure(
    failures: &mut Vec<ProviderInvocationValidationFailure>,
    error: ArtifactValidationError,
    field: &str,
    role: Option<&str>,
) {
    let (kind, obligation) = match error {
        ArtifactValidationError::NonCanonical => (
            ProviderInvocationValidationFailureKind::NonCanonicalArtifact,
            "canonical artifact bytes within the deterministic nesting bound",
        ),
        ArtifactValidationError::UnsupportedDomain => (
            ProviderInvocationValidationFailureKind::UnsupportedArtifactDomain,
            "host-owned canonical schema registered for the artifact domain",
        ),
        ArtifactValidationError::SchemaMismatch => (
            ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch,
            "canonical value valid under the artifact domain's owning schema",
        ),
    };
    push_failure(failures, kind, field, role, obligation);
}

fn check_protocol(
    version: ProviderProtocolVersion,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    if version != TARGET_PROVIDER_PROTOCOL_VERSION {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::UnsupportedProtocolVersion,
            "protocol_version",
            None,
            "target-provider semantic protocol version 1.0.0",
        );
    }
}

fn check_contract_binding(
    binding: &ProviderArtifactBinding,
    field: &str,
    role: Option<&str>,
    fixed_domain: Option<&str>,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    check_resource_ref(&binding.reference, field, role, failures);
    if binding.domain.is_empty() {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::MissingArtifactDomain,
            format!("{field}.domain"),
            role,
            "non-empty host-authored artifact domain",
        );
    } else if fixed_domain.is_some_and(|domain| domain != binding.domain) {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::ArtifactDomainMismatch,
            format!("{field}.domain"),
            role,
            fixed_domain.unwrap_or_default(),
        );
    }
}

fn check_resource_ref(
    reference: &ProviderResourceRef,
    field: &str,
    role: Option<&str>,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    if reference.coordinate.is_empty() {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::EmptyResourceCoordinate,
            format!("{field}.reference.coordinate"),
            role,
            "non-empty resource coordinate",
        );
    }
    if reference.digest.bytes.len() != 32 {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::MalformedDigest,
            format!("{field}.reference.digest"),
            role,
            "sha256 digest with exactly 32 raw bytes",
        );
    }
}

fn check_bound_artifact(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    actual: &ProviderBoundArtifact,
    expected: &ProviderArtifactBinding,
    field: &str,
    role: Option<&str>,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    check_resource_ref(&actual.reference, field, role, failures);
    if actual.reference != expected.reference {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::ArtifactReferenceMismatch,
            format!("{field}.reference"),
            role,
            "resource reference equal to the host-authored invocation binding",
        );
    }
    if actual.artifact.domain.is_empty() {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::MissingArtifactDomain,
            format!("{field}.artifact.domain"),
            role,
            "non-empty artifact domain",
        );
        return;
    }
    if actual.artifact.domain != expected.domain {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::ArtifactDomainMismatch,
            format!("{field}.artifact.domain"),
            role,
            "artifact domain equal to the host-authored invocation binding",
        );
    }
    match validate_artifact_identity(schema_validator, &actual.artifact) {
        Ok(computed) => {
            if actual.reference.digest.bytes.len() == 32
                && actual.reference.digest.bytes.as_slice() != computed
            {
                push_failure(
                    failures,
                    ProviderInvocationValidationFailureKind::ArtifactDigestMismatch,
                    format!("{field}.artifact.bytes"),
                    role,
                    "canonical bytes reproducing the bound sha256 digest",
                );
            }
        }
        Err(error) => {
            push_artifact_validation_failure(failures, error, &format!("{field}.artifact"), role);
        }
    }
}

fn check_semantic_contract(
    bindings: &[ProviderSemanticInputBinding],
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    check_role_order(
        bindings.iter().map(|binding| binding.role.as_str()),
        "contract.semantic_inputs.role",
        failures,
    );
    for binding in bindings {
        let fixed_domain = match binding.kind {
            ProviderSemanticInputKind::Lawpack => Some(PROVIDER_LAWPACK_ARTIFACT_DOMAIN),
            ProviderSemanticInputKind::AuthorityFacts => Some(AUTHORITY_FACTS_API_VERSION),
            ProviderSemanticInputKind::LowerabilityFacts
            | ProviderSemanticInputKind::Auxiliary(_) => None,
        };
        check_contract_binding(
            &binding.artifact,
            "contract.semantic_inputs.artifact",
            Some(&binding.role),
            fixed_domain,
            failures,
        );
    }
}

fn check_semantic_inputs(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    bindings: &[ProviderSemanticInputBinding],
    inputs: &[ProviderSemanticInput],
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    check_role_order(
        inputs.iter().map(|input| input.role.as_str()),
        "semantic_inputs.role",
        failures,
    );
    let bindings_by_role = bindings
        .iter()
        .map(|binding| (binding.role.as_str(), binding))
        .collect::<BTreeMap<_, _>>();
    let inputs_by_role = inputs
        .iter()
        .map(|input| (input.role.as_str(), input))
        .collect::<BTreeMap<_, _>>();

    for binding in bindings {
        if !inputs_by_role.contains_key(binding.role.as_str()) {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::MissingSemanticInput,
                "semantic_inputs.role",
                Some(&binding.role),
                "every host-declared semantic input present exactly once",
            );
        }
    }
    for input in inputs {
        let Some(binding) = bindings_by_role.get(input.role.as_str()) else {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::UndeclaredSemanticInput,
                "semantic_inputs.role",
                Some(&input.role),
                "semantic input role declared by the host invocation contract",
            );
            check_unbound_artifact(
                schema_validator,
                &input.artifact,
                "semantic_inputs.artifact",
                Some(&input.role),
                failures,
            );
            continue;
        };
        if input.kind != binding.kind {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::SemanticInputBindingMismatch,
                "semantic_inputs.kind",
                Some(&input.role),
                "semantic input kind equal to the host-authored binding",
            );
        }
        check_bound_artifact(
            schema_validator,
            &input.artifact,
            &binding.artifact,
            "semantic_inputs.artifact",
            Some(&input.role),
            failures,
        );
    }
}

fn check_unbound_artifact(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    artifact: &ProviderBoundArtifact,
    field: &str,
    role: Option<&str>,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    let binding = ProviderArtifactBinding {
        reference: artifact.reference.clone(),
        domain: artifact.artifact.domain.clone(),
    };
    check_bound_artifact(schema_validator, artifact, &binding, field, role, failures);
}

fn check_lowering_output_requests(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    outputs: &[ProviderLoweringOutputRequest],
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    check_role_order(
        outputs.iter().map(|output| output.role.as_str()),
        "requested_outputs.role",
        failures,
    );
    for output in outputs {
        if output.domain.is_empty() {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::MissingArtifactDomain,
                "requested_outputs.domain",
                Some(&output.role),
                "non-empty requested output domain",
            );
        } else {
            if !schema_validator.supports_domain(&output.domain) {
                push_failure(
                    failures,
                    ProviderInvocationValidationFailureKind::UnsupportedArtifactDomain,
                    "requested_outputs.domain",
                    Some(&output.role),
                    "host-owned canonical schema registered for the requested output domain",
                );
            }
            if output.kind == ProviderLoweringOutputKind::TargetIr
                && output.domain != TARGET_IR_ARTIFACT_DIGEST_DOMAIN
            {
                push_failure(
                    failures,
                    ProviderInvocationValidationFailureKind::ArtifactDomainMismatch,
                    "requested_outputs.domain",
                    Some(&output.role),
                    TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
                );
            }
        }
    }
}

fn check_verification_output_requests(
    schema_validator: &dyn ProviderArtifactSchemaValidator,
    outputs: &[ProviderVerificationOutputRequest],
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    check_role_order(
        outputs.iter().map(|output| output.role.as_str()),
        "requested_outputs.role",
        failures,
    );
    for output in outputs {
        if output.domain.is_empty() {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::MissingArtifactDomain,
                "requested_outputs.domain",
                Some(&output.role),
                "non-empty requested output domain",
            );
        } else if !schema_validator.supports_domain(&output.domain) {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::UnsupportedArtifactDomain,
                "requested_outputs.domain",
                Some(&output.role),
                "host-owned canonical schema registered for the requested output domain",
            );
        }
    }
}

fn check_role_order<'a>(
    roles: impl IntoIterator<Item = &'a str>,
    field: &str,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for role in roles {
        if role.is_empty() {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::EmptyRole,
                field,
                None,
                "non-empty role",
            );
        }
        let duplicate = !seen.insert(role);
        if duplicate {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::DuplicateRole,
                field,
                Some(role),
                "unique role within the role-keyed list",
            );
        } else if previous.is_some_and(|prior| prior.as_bytes() > role.as_bytes()) {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::OutOfOrderRole,
                field,
                Some(role),
                "strict ascending UTF-8 byte order",
            );
        }
        previous = Some(role);
    }
}

fn check_diagnostics(
    diagnostics: &[ProviderDiagnostic],
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    let mut seen = BTreeSet::new();
    let mut previous = None;
    for diagnostic in diagnostics {
        let duplicate = !seen.insert(diagnostic_key(diagnostic));
        if duplicate {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::DuplicateDiagnostic,
                "diagnostics",
                None,
                "unique diagnostics in WIT tuple order",
            );
        } else if previous
            .is_some_and(|prior| compare_diagnostic(prior, diagnostic) == Ordering::Greater)
        {
            push_failure(
                failures,
                ProviderInvocationValidationFailureKind::OutOfOrderDiagnostic,
                "diagnostics",
                None,
                "ascending WIT diagnostic tuple order",
            );
        }
        previous = Some(diagnostic);
    }
}

fn diagnostic_key(diagnostic: &ProviderDiagnostic) -> (&[u8], u8, &[u8], Option<&[u8]>) {
    (
        diagnostic.code.as_bytes(),
        diagnostic.severity.declaration_index(),
        diagnostic.message.as_bytes(),
        diagnostic.repair.as_deref().map(str::as_bytes),
    )
}

fn compare_diagnostic(left: &ProviderDiagnostic, right: &ProviderDiagnostic) -> Ordering {
    left.code
        .as_bytes()
        .cmp(right.code.as_bytes())
        .then_with(|| {
            left.severity
                .declaration_index()
                .cmp(&right.severity.declaration_index())
        })
        .then_with(|| left.message.as_bytes().cmp(right.message.as_bytes()))
        .then_with(|| compare_optional_text(left.repair.as_ref(), right.repair.as_ref()))
}

fn compare_optional_text(left: Option<&String>, right: Option<&String>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left.as_bytes().cmp(right.as_bytes()),
    }
}

fn diagnostic_byte_lengths(diagnostics: &[ProviderDiagnostic], lengths: &mut Vec<usize>) {
    for diagnostic in diagnostics {
        lengths.push(diagnostic.code.len());
        lengths.push(diagnostic.message.len());
        if let Some(repair) = &diagnostic.repair {
            lengths.push(repair.len());
        }
    }
}

fn check_response_limits(
    limits: ProviderResponseLimits,
    output_count: usize,
    diagnostic_count: usize,
    byte_lengths: &[usize],
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    if u64::try_from(output_count).map_or(true, |count| count > u64::from(limits.max_output_count))
    {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::OutputCountLimitExceeded,
            "limits.max_output_count",
            None,
            "returned output count within the request bound",
        );
    }
    if u64::try_from(diagnostic_count)
        .map_or(true, |count| count > u64::from(limits.max_diagnostic_count))
    {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::DiagnosticCountLimitExceeded,
            "limits.max_diagnostic_count",
            None,
            "returned diagnostic count within the request bound",
        );
    }
    let lengths = byte_lengths
        .iter()
        .map(|length| u64::try_from(*length))
        .collect::<Result<Vec<_>, _>>();
    let Ok(lengths) = lengths else {
        push_failure(
            failures,
            ProviderInvocationValidationFailureKind::ResponseByteCountOverflow,
            "limits.max_total_response_bytes",
            None,
            "checked u64 sum of every provider-authored byte and string length",
        );
        return;
    };
    check_response_byte_limit(limits.max_total_response_bytes, lengths, failures);
}

fn check_response_byte_limit(
    max_total_response_bytes: u64,
    lengths: impl IntoIterator<Item = u64>,
    failures: &mut Vec<ProviderInvocationValidationFailure>,
) {
    match checked_response_byte_total(lengths) {
        Some(total) if total > max_total_response_bytes => push_failure(
            failures,
            ProviderInvocationValidationFailureKind::ResponseByteLimitExceeded,
            "limits.max_total_response_bytes",
            None,
            "aggregate provider-authored response bytes within the request bound",
        ),
        Some(_) => {}
        None => push_failure(
            failures,
            ProviderInvocationValidationFailureKind::ResponseByteCountOverflow,
            "limits.max_total_response_bytes",
            None,
            "checked u64 sum of every provider-authored byte and string length",
        ),
    }
}

fn checked_response_byte_total(lengths: impl IntoIterator<Item = u64>) -> Option<u64> {
    lengths.into_iter().try_fold(0u64, u64::checked_add)
}

fn same_lowering_request_except_limits(
    left: &ProviderLoweringRequest,
    right: &ProviderLoweringRequest,
) -> bool {
    let mut normalized_right = right.clone();
    normalized_right.limits = left.limits;
    left == &normalized_right
}

fn same_verification_request_except_limits(
    left: &ProviderVerificationRequest,
    right: &ProviderVerificationRequest,
) -> bool {
    let mut normalized_right = right.clone();
    normalized_right.limits = left.limits;
    left == &normalized_right
}

fn extend_validation_failures<S, K>(
    failures: &mut Vec<ProviderInvocationValidationFailure>,
    result: Result<ValidatedProviderOutcome<S, K>, ProviderInvocationValidationReport>,
) {
    if let Err(report) = result {
        failures.extend(report.failures);
    }
}

fn push_failure(
    failures: &mut Vec<ProviderInvocationValidationFailure>,
    kind: ProviderInvocationValidationFailureKind,
    field: impl Into<String>,
    role: Option<&str>,
    obligation: impl Into<String>,
) {
    failures.push(ProviderInvocationValidationFailure {
        kind,
        field: field.into(),
        role: role.map(str::to_owned),
        obligation: obligation.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::{
        check_response_byte_limit, checked_response_byte_total,
        ProviderInvocationValidationFailureKind,
    };

    #[test]
    fn response_byte_accounting_rejects_u64_overflow() {
        assert_eq!(checked_response_byte_total([u64::MAX, 1]), None);
        assert_eq!(checked_response_byte_total([u64::MAX]), Some(u64::MAX));

        let mut failures = Vec::new();
        check_response_byte_limit(u64::MAX, [u64::MAX, 1], &mut failures);
        assert_eq!(
            failures
                .iter()
                .map(|failure| failure.kind)
                .collect::<Vec<_>>(),
            vec![ProviderInvocationValidationFailureKind::ResponseByteCountOverflow]
        );
    }
}
