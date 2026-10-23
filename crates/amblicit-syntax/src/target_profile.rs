//! Typed v1 target-profile manifest conformance checks.
//!
//! This module models the manifest boundary before any target lowerer runs. It
//! validates hash-significant profile metadata and v1 application doctrine; it
//! does not load files, lower Core, verify Target IR, or perform admission.

use crate::core_ir::{ResourceRef, CORE_API_VERSION};
use crate::lawpack_adapter::LAWPACK_ADAPTER_API_VERSION;

/// Target-profile manifest ABI supported by this crate.
pub const TARGET_PROFILE_API_VERSION: &str = "edict.target-profile/v1";

/// Canonical encoding required by v1 target-profile manifests.
pub const CANONICAL_CBOR_ABI: &str = "edict.canonical-cbor/v1";

const ATOMIC_APPLICATION_MODEL: &str = "atomic";
const APPLICATION_SNAPSHOT_READS: &str = "application-snapshot";
const PRECOMMIT_ATOMIC_GUARDS: &str = "precommit-atomic";
const NO_VISIBLE_EFFECTS_ROLLBACK: &str = "no-visible-effects";

/// Typed target-profile manifest value for `edict.target-profile/v1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProfileManifest {
    pub api_version: String,
    pub id: String,
    pub version: String,
    pub accepted_core_abi: Vec<String>,
    pub intrinsic_namespace: String,
    pub intrinsics: ResourceRef,
    pub operation_profiles: ResourceRef,
    pub footprint_algebra: ResourceRef,
    pub cost_algebra: ResourceRef,
    pub target_ir: ResourceRef,
    pub obstruction_taxonomy: ResourceRef,
    pub verifier: ResourceRef,
    pub lowerer: ResourceRef,
    pub sandbox: ResourceRef,
    pub fuel_model: ResourceRef,
    pub bundle_profile: ResourceRef,
    pub generated_artifact_profiles: Vec<ResourceRef>,
    pub canonical_encoding_rules: ResourceRef,
    pub accepted_lawpack_adapter_abi: Vec<String>,
    pub diagnostic_abi: ResourceRef,
    pub application_model: String,
    pub read_consistency: String,
    pub guard_evaluation: String,
    pub obstruction_rollback: String,
    pub multi_target: bool,
    pub postcondition_support: bool,
    pub deterministic_execution: ResourceRef,
    pub conformance_fixture_corpus: ResourceRef,
}

/// Overall target-profile conformance classification for v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetProfileConformanceStatus {
    Conformant,
    NonConformant,
}

/// Stable failure categories returned by target-profile conformance checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetProfileConformanceFailureKind {
    InvalidApiVersion,
    MissingProfileIdentity,
    MissingAcceptedCoreAbi,
    MissingIntrinsicNamespace,
    NonDigestLockedResource,
    UnsupportedCanonicalEncoding,
    UnsupportedLawpackAdapterAbi,
    UnsupportedCompositeProfile,
    UnsupportedApplicationModel,
    UnsupportedReadConsistency,
    UnsupportedGuardEvaluation,
    UnsupportedRollbackSemantics,
}

/// One failed target-profile conformance obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProfileConformanceFailure {
    pub kind: TargetProfileConformanceFailureKind,
    pub field: String,
    pub obligation: String,
}

/// Complete v1 target-profile conformance report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProfileConformanceReport {
    pub status: TargetProfileConformanceStatus,
    pub profile: String,
    pub failures: Vec<TargetProfileConformanceFailure>,
}

/// Validate a typed target-profile manifest value against the v1 contract.
///
/// The check is runtime-neutral: Echo-specific and non-Echo profiles are
/// accepted or rejected by the same manifest obligations.
#[must_use]
pub fn validate_target_profile_manifest(
    manifest: &TargetProfileManifest,
) -> TargetProfileConformanceReport {
    let mut failures = Vec::new();

    if manifest.api_version != TARGET_PROFILE_API_VERSION {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::InvalidApiVersion,
            "api_version",
            TARGET_PROFILE_API_VERSION,
        );
    }
    if manifest.id.is_empty() {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::MissingProfileIdentity,
            "id",
            "non-empty profile id",
        );
    }
    if manifest.version.is_empty() {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::MissingProfileIdentity,
            "version",
            "non-empty profile version",
        );
    }
    if !manifest
        .accepted_core_abi
        .iter()
        .any(|abi| abi == CORE_API_VERSION)
    {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::MissingAcceptedCoreAbi,
            "accepted_core_abi",
            CORE_API_VERSION,
        );
    }
    if manifest.intrinsic_namespace.is_empty() {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::MissingIntrinsicNamespace,
            "intrinsic_namespace",
            "non-empty intrinsic namespace",
        );
    }

    check_resource_refs(manifest, &mut failures);

    if manifest.canonical_encoding_rules.coordinate != CANONICAL_CBOR_ABI {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::UnsupportedCanonicalEncoding,
            "canonical_encoding_rules",
            CANONICAL_CBOR_ABI,
        );
    }
    let lawpack_adapter_abi_supported = match manifest.accepted_lawpack_adapter_abi.as_slice() {
        [] => true,
        [abi] => abi == LAWPACK_ADAPTER_API_VERSION,
        _ => false,
    };
    if !lawpack_adapter_abi_supported {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::UnsupportedLawpackAdapterAbi,
            "accepted_lawpack_adapter_abi",
            format!("empty or exactly [`{LAWPACK_ADAPTER_API_VERSION}`]"),
        );
    }
    if manifest.multi_target {
        push_failure(
            &mut failures,
            TargetProfileConformanceFailureKind::UnsupportedCompositeProfile,
            "multi_target",
            "false until composite profile validation is implemented",
        );
    }

    check_application_doctrine(manifest, &mut failures);

    let status = if failures.is_empty() {
        TargetProfileConformanceStatus::Conformant
    } else {
        TargetProfileConformanceStatus::NonConformant
    };

    TargetProfileConformanceReport {
        status,
        profile: format!("{}@{}", manifest.id, manifest.version),
        failures,
    }
}

fn check_resource_refs(
    manifest: &TargetProfileManifest,
    failures: &mut Vec<TargetProfileConformanceFailure>,
) {
    for (field, resource) in [
        ("intrinsics", &manifest.intrinsics),
        ("operation_profiles", &manifest.operation_profiles),
        ("footprint_algebra", &manifest.footprint_algebra),
        ("cost_algebra", &manifest.cost_algebra),
        ("target_ir", &manifest.target_ir),
        ("obstruction_taxonomy", &manifest.obstruction_taxonomy),
        ("verifier", &manifest.verifier),
        ("lowerer", &manifest.lowerer),
        ("sandbox", &manifest.sandbox),
        ("fuel_model", &manifest.fuel_model),
        ("bundle_profile", &manifest.bundle_profile),
        (
            "canonical_encoding_rules",
            &manifest.canonical_encoding_rules,
        ),
        ("diagnostic_abi", &manifest.diagnostic_abi),
        ("deterministic_execution", &manifest.deterministic_execution),
        (
            "conformance_fixture_corpus",
            &manifest.conformance_fixture_corpus,
        ),
    ] {
        check_digest_locked_resource(field, resource, failures);
    }

    for resource in &manifest.generated_artifact_profiles {
        check_digest_locked_resource("generated_artifact_profiles", resource, failures);
    }
}

fn check_digest_locked_resource(
    field: &str,
    resource: &ResourceRef,
    failures: &mut Vec<TargetProfileConformanceFailure>,
) {
    if !resource.is_digest_locked() {
        push_failure(
            failures,
            TargetProfileConformanceFailureKind::NonDigestLockedResource,
            field,
            "non-empty coordinate and sha256 digest",
        );
    }
}

fn check_application_doctrine(
    manifest: &TargetProfileManifest,
    failures: &mut Vec<TargetProfileConformanceFailure>,
) {
    if manifest.application_model != ATOMIC_APPLICATION_MODEL {
        push_failure(
            failures,
            TargetProfileConformanceFailureKind::UnsupportedApplicationModel,
            "application_model",
            ATOMIC_APPLICATION_MODEL,
        );
    }
    if manifest.read_consistency != APPLICATION_SNAPSHOT_READS {
        push_failure(
            failures,
            TargetProfileConformanceFailureKind::UnsupportedReadConsistency,
            "read_consistency",
            APPLICATION_SNAPSHOT_READS,
        );
    }
    if manifest.guard_evaluation != PRECOMMIT_ATOMIC_GUARDS {
        push_failure(
            failures,
            TargetProfileConformanceFailureKind::UnsupportedGuardEvaluation,
            "guard_evaluation",
            PRECOMMIT_ATOMIC_GUARDS,
        );
    }
    if manifest.obstruction_rollback != NO_VISIBLE_EFFECTS_ROLLBACK {
        push_failure(
            failures,
            TargetProfileConformanceFailureKind::UnsupportedRollbackSemantics,
            "obstruction_rollback",
            NO_VISIBLE_EFFECTS_ROLLBACK,
        );
    }
}

fn push_failure(
    failures: &mut Vec<TargetProfileConformanceFailure>,
    kind: TargetProfileConformanceFailureKind,
    field: impl Into<String>,
    obligation: impl Into<String>,
) {
    let failure = TargetProfileConformanceFailure {
        kind,
        field: field.into(),
        obligation: obligation.into(),
    };
    if !failures.contains(&failure) {
        failures.push(failure);
    }
}
