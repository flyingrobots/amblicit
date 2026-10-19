//! Target-profile conformance checks for typed v1 manifest values.
//!
//! These tests assert public behavior: runtime-neutral conformance status and
//! stable failure kinds. They do not inspect documentation prose, repository
//! layout, or CDDL text.

use edict_syntax::{
    validate_target_profile_manifest, ResourceRef, TargetProfileConformanceFailureKind,
    TargetProfileConformanceStatus, TargetProfileManifest, CORE_API_VERSION,
    LAWPACK_ADAPTER_API_VERSION,
};

fn digest_locked(coordinate: &str) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(
            "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_owned(),
        ),
    }
}

fn echo_profile() -> TargetProfileManifest {
    TargetProfileManifest {
        api_version: "edict.target-profile/v1".to_owned(),
        id: "echo.dpo".to_owned(),
        version: "1".to_owned(),
        accepted_core_abi: vec![CORE_API_VERSION.to_owned()],
        intrinsic_namespace: "echo.dpo@1".to_owned(),
        intrinsics: digest_locked("echo.dpo.intrinsics/v1"),
        operation_profiles: digest_locked("echo.dpo.operation-profiles/v1"),
        footprint_algebra: digest_locked("echo.dpo.footprint/v1"),
        cost_algebra: digest_locked("echo.dpo.cost/v1"),
        target_ir: digest_locked("echo.span-ir/v1"),
        obstruction_taxonomy: digest_locked("echo.dpo.obstructions/v1"),
        verifier: digest_locked("echo.dpo.verifier/v1"),
        lowerer: digest_locked("echo.dpo.lowerer/v1"),
        sandbox: digest_locked("edict.wasm-component/v1"),
        fuel_model: digest_locked("edict.fuel/v1"),
        bundle_profile: digest_locked("echo.dpo.bundle/v1"),
        generated_artifact_profiles: vec![digest_locked("echo.dpo.registration/v1")],
        canonical_encoding_rules: digest_locked("edict.canonical-cbor/v1"),
        accepted_lawpack_adapter_abi: vec![LAWPACK_ADAPTER_API_VERSION.to_owned()],
        diagnostic_abi: digest_locked("edict.diagnostics/v1"),
        application_model: "atomic".to_owned(),
        read_consistency: "application-snapshot".to_owned(),
        guard_evaluation: "precommit-atomic".to_owned(),
        obstruction_rollback: "no-visible-effects".to_owned(),
        multi_target: false,
        postcondition_support: true,
        deterministic_execution: digest_locked("edict.determinism/v1"),
        conformance_fixture_corpus: digest_locked("echo.dpo.fixtures/v1"),
    }
}

fn kv_profile() -> TargetProfileManifest {
    let mut profile = echo_profile();
    "kv.transactional".clone_into(&mut profile.id);
    "kv.transactional@1".clone_into(&mut profile.intrinsic_namespace);
    profile.intrinsics = digest_locked("kv.transactional.intrinsics/v1");
    profile.operation_profiles = digest_locked("kv.transactional.operation-profiles/v1");
    profile.footprint_algebra = digest_locked("kv.transactional.footprint/v1");
    profile.cost_algebra = digest_locked("kv.transactional.cost/v1");
    profile.target_ir = digest_locked("kv.transactional.ir/v1");
    profile.obstruction_taxonomy = digest_locked("kv.transactional.obstructions/v1");
    profile.verifier = digest_locked("kv.transactional.verifier/v1");
    profile.lowerer = digest_locked("kv.transactional.lowerer/v1");
    profile.bundle_profile = digest_locked("kv.transactional.bundle/v1");
    profile.generated_artifact_profiles = vec![digest_locked("kv.transactional.plan/v1")];
    profile.conformance_fixture_corpus = digest_locked("kv.transactional.fixtures/v1");
    profile
}

fn failure_kinds(profile: &TargetProfileManifest) -> Vec<TargetProfileConformanceFailureKind> {
    validate_target_profile_manifest(profile)
        .failures
        .iter()
        .map(|failure| failure.kind)
        .collect()
}

#[test]
fn echo_and_kv_profiles_conform_to_the_same_runtime_neutral_manifest_contract() {
    for profile in [echo_profile(), kv_profile()] {
        let report = validate_target_profile_manifest(&profile);

        assert_eq!(report.status, TargetProfileConformanceStatus::Conformant);
        assert_eq!(
            report.failures,
            [] as [edict_syntax::TargetProfileConformanceFailure; 0]
        );
    }
}

#[test]
fn missing_digest_on_normative_manifest_slot_is_rejected() {
    let mut profile = echo_profile();
    profile.verifier.digest = None;

    let report = validate_target_profile_manifest(&profile);

    assert_eq!(report.status, TargetProfileConformanceStatus::NonConformant);
    assert_eq!(
        report.failures[0].kind,
        TargetProfileConformanceFailureKind::NonDigestLockedResource
    );
    assert_eq!(report.failures[0].field, "verifier");
}

#[test]
fn malformed_digest_on_normative_manifest_slot_is_rejected() {
    let mut profile = echo_profile();
    profile.verifier.digest = Some("not-a-digest".to_owned());

    let report = validate_target_profile_manifest(&profile);

    assert_eq!(report.status, TargetProfileConformanceStatus::NonConformant);
    assert_eq!(
        report.failures[0].kind,
        TargetProfileConformanceFailureKind::NonDigestLockedResource
    );
    assert_eq!(report.failures[0].field, "verifier");
}

#[test]
fn accepted_core_abi_must_include_v1_core() {
    let mut profile = echo_profile();
    profile.accepted_core_abi = vec!["edict.core/v2".to_owned()];

    assert_eq!(
        failure_kinds(&profile),
        vec![TargetProfileConformanceFailureKind::MissingAcceptedCoreAbi]
    );
}

#[test]
fn direct_lawpack_adapter_abi_is_supported_in_v1() {
    let profile = echo_profile();

    assert_eq!(
        failure_kinds(&profile),
        [] as [edict_syntax::TargetProfileConformanceFailureKind; 0]
    );
}

#[test]
fn unknown_or_duplicate_lawpack_adapter_abis_are_rejected() {
    let mut profile = echo_profile();
    profile.accepted_lawpack_adapter_abi = vec!["edict.lawpack-adapter/v2".to_owned()];

    assert_eq!(
        failure_kinds(&profile),
        vec![TargetProfileConformanceFailureKind::UnsupportedLawpackAdapterAbi]
    );

    profile.accepted_lawpack_adapter_abi = vec![LAWPACK_ADAPTER_API_VERSION.to_owned(); 2];
    assert_eq!(
        failure_kinds(&profile),
        vec![TargetProfileConformanceFailureKind::UnsupportedLawpackAdapterAbi]
    );
}

#[test]
fn multi_target_profiles_are_rejected_until_composite_validation_exists() {
    let mut profile = echo_profile();
    profile.multi_target = true;

    assert_eq!(
        failure_kinds(&profile),
        vec![TargetProfileConformanceFailureKind::UnsupportedCompositeProfile]
    );
}

#[test]
fn atomic_application_semantics_are_required_for_v1_conformance() {
    let mut profile = echo_profile();
    "eventual".clone_into(&mut profile.application_model);
    "read-committed".clone_into(&mut profile.read_consistency);
    "postcommit".clone_into(&mut profile.guard_evaluation);
    "best-effort".clone_into(&mut profile.obstruction_rollback);

    assert_eq!(
        failure_kinds(&profile),
        vec![
            TargetProfileConformanceFailureKind::UnsupportedApplicationModel,
            TargetProfileConformanceFailureKind::UnsupportedReadConsistency,
            TargetProfileConformanceFailureKind::UnsupportedGuardEvaluation,
            TargetProfileConformanceFailureKind::UnsupportedRollbackSemantics,
        ]
    );
}
