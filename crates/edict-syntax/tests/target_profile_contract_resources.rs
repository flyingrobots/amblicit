//! Edict-owned target-profile contract-resource authority checks.

use edict_syntax::{
    canonical_target_profile_contract_resources, decode_canonical_cbor,
    digest_target_profile_contract_resource, encode_canonical_cbor,
    validate_target_profile_contract_resources, validate_target_profile_manifest, CanonicalValue,
    ResourceRef, TargetProfileConformanceStatus, TargetProfileContractResourceFailureKind,
    TargetProfileManifest, CORE_API_VERSION,
};

const EXPECTED_COORDINATES: [&str; 5] = [
    "edict.canonical-cbor/v1",
    "edict.determinism/v1",
    "edict.diagnostics/v1",
    "edict.fuel/v1",
    "edict.wasm-component/v1",
];

#[test]
fn canonical_contract_resources_are_complete_and_reproducible() {
    let first = canonical_target_profile_contract_resources();
    let second = canonical_target_profile_contract_resources();

    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(|resource| resource.coordinate.as_str())
            .collect::<Vec<_>>(),
        EXPECTED_COORDINATES
    );
    for resource in first {
        let decoded = decode_canonical_cbor(&resource.canonical_bytes)
            .expect("published contract resource must be canonical CBOR");
        assert_eq!(
            encode_canonical_cbor(&decoded).expect("decoded resource re-encodes"),
            resource.canonical_bytes
        );
        assert_eq!(
            digest_target_profile_contract_resource(
                &resource.coordinate,
                &resource.canonical_bytes
            )
            .expect("published resource has a recognized digest domain"),
            resource.digest
        );
        assert!(resource.digest.starts_with("sha256:"));
        assert_eq!(resource.digest.len(), 71);
        assert_eq!(
            resource.provenance.repository,
            "https://github.com/flyingrobots/edict"
        );
        assert!(resource
            .provenance
            .source_path
            .starts_with("fixtures/target-profile/contract-resources/"));
    }
}

#[test]
fn semantic_mutation_moves_digest_without_gaining_authority() {
    let mut resources = canonical_target_profile_contract_resources();
    let resource = resources
        .iter_mut()
        .find(|resource| resource.coordinate == "edict.determinism/v1")
        .expect("determinism resource exists");
    let mut value = decode_canonical_cbor(&resource.canonical_bytes).expect("resource decodes");
    set_contract_bool(&mut value, "ambientClock", true);
    resource.canonical_bytes = encode_canonical_cbor(&value).expect("mutation stays canonical");
    let moved_digest =
        digest_target_profile_contract_resource(&resource.coordinate, &resource.canonical_bytes)
            .expect("mutated canonical resource can be identified");
    assert_ne!(moved_digest, resource.digest);
    resource.digest = moved_digest;

    let failures = validate_target_profile_contract_resources(resources)
        .expect_err("a recomputed digest cannot authorize replacement Edict policy");
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        vec![TargetProfileContractResourceFailureKind::ArtifactBytesMismatch]
    );
}

#[test]
fn contract_resource_boundary_rejects_every_identity_mismatch() {
    assert_single_mutation_failure(
        |resources| resources[0].coordinate = "vendor.canonical-cbor/v1".to_owned(),
        &[
            TargetProfileContractResourceFailureKind::MissingResource,
            TargetProfileContractResourceFailureKind::UnknownResource,
        ],
    );
    assert_single_mutation_failure(
        |resources| resources[0].canonical_bytes.push(0),
        &[TargetProfileContractResourceFailureKind::InvalidCanonicalArtifact],
    );
    assert_single_mutation_failure(
        |resources| resources[0].digest = format!("sha256:{}", "0".repeat(64)),
        &[TargetProfileContractResourceFailureKind::ArtifactDigestMismatch],
    );
    assert_single_mutation_failure(
        |resources| resources[0].provenance.source_path.push_str(".wrong"),
        &[TargetProfileContractResourceFailureKind::ProvenanceMismatch],
    );
    assert_single_mutation_failure(
        |resources| {
            resources.pop();
        },
        &[TargetProfileContractResourceFailureKind::MissingResource],
    );
    assert_single_mutation_failure(
        |resources| resources.push(resources[0].clone()),
        &[TargetProfileContractResourceFailureKind::DuplicateResource],
    );
}

#[test]
fn contract_resource_validation_is_input_order_independent() {
    let ordered =
        validate_target_profile_contract_resources(canonical_target_profile_contract_resources())
            .expect("canonical resources validate");
    let mut reversed = canonical_target_profile_contract_resources();
    reversed.reverse();
    let reversed = validate_target_profile_contract_resources(reversed)
        .expect("resource input order is non-semantic");

    assert_eq!(ordered, reversed);
    assert_eq!(
        ordered
            .resources()
            .iter()
            .map(|resource| resource.coordinate.as_str())
            .collect::<Vec<_>>(),
        EXPECTED_COORDINATES
    );
}

#[test]
fn validated_contract_resources_bind_runtime_neutral_profiles() {
    let resources =
        validate_target_profile_contract_resources(canonical_target_profile_contract_resources())
            .expect("canonical resources validate");

    for profile in [echo_profile(), kv_profile()] {
        let bound = resources.bind_manifest(profile);
        let report = validate_target_profile_manifest(&bound);
        assert_eq!(report.status, TargetProfileConformanceStatus::Conformant);
        assert_eq!(
            report.failures,
            [] as [edict_syntax::TargetProfileConformanceFailure; 0]
        );

        for (field, reference) in [
            ("edict.canonical-cbor/v1", &bound.canonical_encoding_rules),
            ("edict.determinism/v1", &bound.deterministic_execution),
            ("edict.diagnostics/v1", &bound.diagnostic_abi),
            ("edict.fuel/v1", &bound.fuel_model),
            ("edict.wasm-component/v1", &bound.sandbox),
        ] {
            assert_eq!(reference.coordinate, field);
            assert_eq!(
                reference.digest.as_deref(),
                resources
                    .resource(field)
                    .map(|resource| resource.digest.as_str())
            );
        }
    }
}

fn assert_single_mutation_failure(
    mutate: impl FnOnce(&mut Vec<edict_syntax::TargetProfileContractResource>),
    expected: &[TargetProfileContractResourceFailureKind],
) {
    let mut resources = canonical_target_profile_contract_resources();
    mutate(&mut resources);
    let failures = validate_target_profile_contract_resources(resources)
        .expect_err("mutated authority input must reject");
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>(),
        expected
    );
}

fn set_contract_bool(value: &mut CanonicalValue, field: &str, replacement: bool) {
    let CanonicalValue::Map(root) = value else {
        panic!("contract resource root must be a map");
    };
    let contract = root
        .iter_mut()
        .find_map(|(key, value)| {
            (key == &CanonicalValue::Text("contract".to_owned())).then_some(value)
        })
        .expect("contract field exists");
    let CanonicalValue::Map(contract) = contract else {
        panic!("contract field must be a map");
    };
    let value = contract
        .iter_mut()
        .find_map(|(key, value)| (key == &CanonicalValue::Text(field.to_owned())).then_some(value))
        .unwrap_or_else(|| panic!("contract field {field} exists"));
    *value = CanonicalValue::Bool(replacement);
}

fn echo_profile() -> TargetProfileManifest {
    TargetProfileManifest {
        api_version: "edict.target-profile/v1".to_owned(),
        id: "echo.dpo".to_owned(),
        version: "1".to_owned(),
        accepted_core_abi: vec![CORE_API_VERSION.to_owned()],
        intrinsic_namespace: "echo.dpo@1".to_owned(),
        intrinsics: resource("echo.dpo.intrinsics/v1"),
        operation_profiles: resource("echo.dpo.operation-profiles/v1"),
        footprint_algebra: resource("echo.dpo.footprint/v1"),
        cost_algebra: resource("echo.dpo.cost/v1"),
        target_ir: resource("echo.span-ir/v1"),
        obstruction_taxonomy: resource("echo.dpo.obstructions/v1"),
        verifier: resource("echo.dpo.verifier/v1"),
        lowerer: resource("echo.dpo.lowerer/v1"),
        sandbox: resource("placeholder.sandbox/v1"),
        fuel_model: resource("placeholder.fuel/v1"),
        bundle_profile: resource("echo.dpo.bundle/v1"),
        generated_artifact_profiles: vec![resource("echo.dpo.registration/v1")],
        canonical_encoding_rules: resource("edict.canonical-cbor/v1"),
        accepted_lawpack_adapter_abi: Vec::new(),
        diagnostic_abi: resource("placeholder.diagnostics/v1"),
        application_model: "atomic".to_owned(),
        read_consistency: "application-snapshot".to_owned(),
        guard_evaluation: "precommit-atomic".to_owned(),
        obstruction_rollback: "no-visible-effects".to_owned(),
        multi_target: false,
        postcondition_support: true,
        deterministic_execution: resource("placeholder.determinism/v1"),
        conformance_fixture_corpus: resource("echo.dpo.fixtures/v1"),
    }
}

fn kv_profile() -> TargetProfileManifest {
    let mut profile = echo_profile();
    "kv.transactional".clone_into(&mut profile.id);
    "kv.transactional@1".clone_into(&mut profile.intrinsic_namespace);
    profile.intrinsics = resource("kv.transactional.intrinsics/v1");
    profile.operation_profiles = resource("kv.transactional.operation-profiles/v1");
    profile.footprint_algebra = resource("kv.transactional.footprint/v1");
    profile.cost_algebra = resource("kv.transactional.cost/v1");
    profile.target_ir = resource("kv.transactional.ir/v1");
    profile.obstruction_taxonomy = resource("kv.transactional.obstructions/v1");
    profile.verifier = resource("kv.transactional.verifier/v1");
    profile.lowerer = resource("kv.transactional.lowerer/v1");
    profile.bundle_profile = resource("kv.transactional.bundle/v1");
    profile.generated_artifact_profiles = vec![resource("kv.transactional.plan/v1")];
    profile.conformance_fixture_corpus = resource("kv.transactional.fixtures/v1");
    profile
}

fn resource(coordinate: &str) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(format!("sha256:{}", "2".repeat(64))),
    }
}
