//! Contract bundle checks for typed v1 bundle and assurance manifests.
//!
//! These tests assert public behavior: validation status and stable failure
//! kinds. They do not inspect diagnostic prose, serialized bytes, or repository
//! layout.

mod common;

use edict_syntax::{
    validate_contract_bundle_manifest, AssuranceEvidenceRef, AssuranceRole, BundleSubject,
    BundleSubjectKind, ContractBundleManifest, ContractBundleValidationFailureKind,
    ContractBundleValidationReport, ContractBundleValidationStatus, ResourceRef, SourceArtifactRef,
    CONTRACT_BUNDLE_API_VERSION,
};

fn digest(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
}

fn uppercase_digest() -> String {
    format!("sha256:{}", "A".repeat(64))
}

fn digest_locked(coordinate: &str, hex: char) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(digest(hex)),
    }
}

fn subject(kind: BundleSubjectKind, digest: &str) -> BundleSubject {
    BundleSubject {
        kind,
        digest: digest.to_owned(),
    }
}

fn evidence(
    role: AssuranceRole,
    semantic_digest: &str,
    target_profile_digest: &str,
    target_ir_digest: &str,
    artifact_hex: char,
) -> AssuranceEvidenceRef {
    AssuranceEvidenceRef {
        role,
        artifact: digest_locked(
            match role {
                AssuranceRole::Holmes => "holmes.lawfulness-certificate/v1",
                AssuranceRole::Watson => "watson.compile-explanation/v1",
                AssuranceRole::Moriarty => "moriarty.hash-impact-matrix/v1",
            },
            artifact_hex,
        ),
        subject: subject(BundleSubjectKind::Semantic, semantic_digest),
        target_profile_digest: target_profile_digest.to_owned(),
        target_ir_digest: target_ir_digest.to_owned(),
    }
}

fn echo_bundle() -> ContractBundleManifest {
    let semantic_bundle_digest = digest('a');
    let target_profile = digest_locked("echo.dpo@1", 'c');
    let target_ir = digest_locked("echo.span-ir/v1", 'd');
    let target_profile_digest = target_profile
        .digest
        .clone()
        .expect("target profile digest");
    let target_ir_digest = target_ir.digest.clone().expect("target IR digest");

    ContractBundleManifest {
        api_version: CONTRACT_BUNDLE_API_VERSION.to_owned(),
        semantic_bundle_digest: semantic_bundle_digest.clone(),
        release_bundle_digest: digest('b'),
        source_artifacts: vec![SourceArtifactRef {
            logical_path: "contracts/hello.edict".to_owned(),
            artifact: digest_locked("source.contracts.hello", 'e'),
        }],
        source_profile_semantic_facts: digest_locked("source-profile.hello/v1", 'f'),
        core_ir: digest_locked("edict.core/v1", '1'),
        target_profile,
        target_ir,
        lawpacks: vec![digest_locked("hello.optics@1", '2')],
        generated_artifacts: vec![digest_locked("echo.dpo.registration/v1", '3')],
        compiler: digest_locked("edict.compiler/v1", '4'),
        lowerer: digest_locked("echo.dpo.lowerer/v1", '5'),
        verifier: digest_locked("echo.dpo.verifier/v1", '6'),
        semantic_compile_options: digest_locked("edict.compile-options.semantic/v1", '7'),
        non_semantic_compile_options: digest_locked("edict.compile-options.nonsemantic/v1", '8'),
        build_provenance: digest_locked("edict.build-provenance/v1", '9'),
        canonicalization_profile: digest_locked("edict.canonical-cbor/v1", '8'),
        conformance_fixture_corpora: vec![digest_locked("echo.dpo.fixtures/v1", '9')],
        verifier_report: digest_locked("echo.dpo.verifier-report/v1", 'a'),
        compile_explanation: digest_locked("watson.compile-explanation/v1", 'b'),
        assurance_evidence: vec![
            evidence(
                AssuranceRole::Holmes,
                &semantic_bundle_digest,
                &target_profile_digest,
                &target_ir_digest,
                'c',
            ),
            evidence(
                AssuranceRole::Watson,
                &semantic_bundle_digest,
                &target_profile_digest,
                &target_ir_digest,
                'd',
            ),
            evidence(
                AssuranceRole::Moriarty,
                &semantic_bundle_digest,
                &target_profile_digest,
                &target_ir_digest,
                'e',
            ),
        ],
        admission_artifacts: Vec::new(),
    }
}

fn kv_bundle() -> ContractBundleManifest {
    let mut bundle = echo_bundle();
    "contracts/kv/read.edict".clone_into(&mut bundle.source_artifacts[0].logical_path);
    bundle.source_artifacts[0].artifact = digest_locked("source.contracts.kv-read", 'f');
    bundle.target_profile = digest_locked("kv.transactional@1", '1');
    bundle.target_ir = digest_locked("kv.transactional.ir/v1", '2');
    bundle.generated_artifacts = vec![digest_locked("kv.transactional.plan/v1", '3')];
    bundle.lowerer = digest_locked("kv.transactional.lowerer/v1", '4');
    bundle.verifier = digest_locked("kv.transactional.verifier/v1", '5');
    bundle.conformance_fixture_corpora = vec![digest_locked("kv.transactional.fixtures/v1", '6')];

    let target_profile_digest = bundle
        .target_profile
        .digest
        .clone()
        .expect("target profile digest");
    let target_ir_digest = bundle.target_ir.digest.clone().expect("target IR digest");
    for evidence in &mut bundle.assurance_evidence {
        evidence
            .target_profile_digest
            .clone_from(&target_profile_digest);
        evidence.target_ir_digest.clone_from(&target_ir_digest);
    }

    bundle
}

fn failure_kinds(
    report: &ContractBundleValidationReport,
) -> Vec<ContractBundleValidationFailureKind> {
    report.failures.iter().map(|failure| failure.kind).collect()
}

#[test]
fn echo_and_kv_bundles_validate_with_the_same_runtime_neutral_contract() {
    for bundle in [echo_bundle(), kv_bundle()] {
        let report = validate_contract_bundle_manifest(&bundle);

        assert_eq!(report.status, ContractBundleValidationStatus::Valid);
        assert_eq!(
            report.failures,
            [] as [edict_syntax::ContractBundleValidationFailure; 0]
        );
    }
}

#[test]
fn bundle_artifact_references_must_be_digest_locked() {
    let mut bundle = echo_bundle();
    bundle.target_profile.digest = None;

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::InvalidArtifactReference]
    );
    assert_eq!(report.failures[0].field, "target_profile");
}

#[test]
fn malformed_bundle_artifact_digest_is_rejected() {
    let mut bundle = echo_bundle();
    bundle.target_profile.digest = Some("not-a-digest".to_owned());

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::InvalidArtifactReference]
    );
    assert_eq!(report.failures[0].field, "target_profile");
}

#[test]
fn uppercase_bundle_digest_rendering_is_rejected() {
    let mut bundle = echo_bundle();
    bundle.semantic_bundle_digest = uppercase_digest();

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::InvalidBundleDigest]
    );
    assert_eq!(report.failures[0].field, "semantic_bundle_digest");
}

#[test]
fn uppercase_artifact_digest_rendering_is_rejected() {
    let mut bundle = echo_bundle();
    bundle.target_profile.digest = Some(uppercase_digest());

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::InvalidArtifactReference]
    );
    assert_eq!(report.failures[0].field, "target_profile");
}

#[test]
fn release_bundle_inputs_must_be_digest_locked() {
    let mut bundle = echo_bundle();
    bundle.build_provenance.digest = None;

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::InvalidArtifactReference]
    );
    assert_eq!(report.failures[0].field, "build_provenance");
}

#[test]
fn canonicalization_profile_must_be_the_v1_cbor_profile() {
    let mut bundle = echo_bundle();
    bundle.canonicalization_profile.coordinate = "custom-cbor/v2".to_owned();

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::UnsupportedCanonicalizationProfile]
    );
    assert_eq!(report.failures[0].field, "canonicalization_profile");
}

#[test]
fn optional_artifact_lists_may_be_empty() {
    let mut bundle = echo_bundle();
    bundle.lawpacks.clear();
    bundle.generated_artifacts.clear();
    bundle.conformance_fixture_corpora.clear();

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Valid);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::ContractBundleValidationFailure; 0]
    );
}

#[test]
fn source_artifact_paths_must_be_logical_package_relative_paths() {
    for logical_path in [
        "",
        "/Users/james/git/edict/contracts/hello.edict",
        "contracts/../hello.edict",
        "contracts/./hello.edict",
        "C:/contracts/hello.edict",
        r"contracts\hello.edict",
    ] {
        let mut bundle = echo_bundle();
        bundle.source_artifacts = vec![SourceArtifactRef {
            logical_path: logical_path.to_owned(),
            artifact: digest_locked("source.contracts.hello", 'e'),
        }];

        let report = validate_contract_bundle_manifest(&bundle);

        assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
        assert_eq!(
            failure_kinds(&report),
            vec![ContractBundleValidationFailureKind::InvalidSourcePath]
        );
    }
}

#[test]
fn assurance_evidence_must_match_bundle_subject() {
    let mut bundle = echo_bundle();
    bundle.assurance_evidence[0].subject.digest = digest('f');

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::AssuranceSubjectMismatch]
    );
}

#[test]
fn assurance_evidence_must_match_target_profile_digest() {
    let mut bundle = echo_bundle();
    bundle.assurance_evidence[1].target_profile_digest = digest('f');

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::AssuranceTargetProfileMismatch]
    );
}

#[test]
fn assurance_evidence_must_match_target_ir_digest() {
    let mut bundle = echo_bundle();
    bundle.assurance_evidence[2].target_ir_digest = digest('f');

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::AssuranceTargetIrMismatch]
    );
}

#[test]
fn external_assurance_evidence_is_optional() {
    let mut bundle = echo_bundle();
    bundle.assurance_evidence.clear();

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Valid);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::ContractBundleValidationFailure; 0]
    );
}

#[test]
fn admission_artifacts_are_rejected_from_contract_bundles() {
    let mut bundle = echo_bundle();
    bundle
        .admission_artifacts
        .push(digest_locked("continuum.admission-receipt/v1", 'f'));

    let report = validate_contract_bundle_manifest(&bundle);

    assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![ContractBundleValidationFailureKind::AdmissionArtifactUnsupported]
    );
}

mod contract_bundle_assembly {
    use super::{
        common::bounded_hello_core, digest, digest_locked, failure_kinds, uppercase_digest,
    };
    use edict_syntax::{
        assemble_contract_bundle, assemble_contract_bundle_from_target_ir, compile_to_core,
        digest_core_module, digest_target_ir_artifact, lower_to_target_ir, AssuranceRole,
        BundleSubjectKind, CompilerContext, ContractBundleAssemblyErrorKind,
        ContractBundleAssemblyFromTargetIrInput, ContractBundleAssemblyInput,
        ContractBundleAssuranceEvidenceInput, ContractBundleSourceArtifact,
        ContractBundleValidationFailureKind, ContractBundleValidationStatus, CoreBudget, CoreExpr,
        CoreImport, CoreImportKind, CoreValue, DigestLockedResource, ResourceRef,
        SuppliedTargetIrResource, TargetEffectLowering, TargetIrArtifact, TargetIrLoweringFacts,
        TargetIrSemanticClosure, WriteClass, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
    };

    const EFFECTFUL_REPLACE: &str = "package a.b@1;\n\
        use lawpack hello.optics@1 digest \"sha256:2222222222222222222222222222222222222222222222222222222222222222\" as hello;\n\
        type Input = { id: String<max=16>, };\n\
        type Receipt = { id: String<max=16>, };\n\
        type Output = { id: String<max=16>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.effectful\n\
          basis none\n\
          budget <= p.tiny {\n\
          let receipt: Receipt = target.replace(input.id)\n\
            else { rejected(reason) => domain.WriteRejected };\n\
          return { id: input.id };\n\
        }";

    fn resource(coordinate: &str, hex: char) -> DigestLockedResource {
        DigestLockedResource::new(coordinate, digest(hex)).expect("digest-locked resource")
    }

    fn target_ir(coordinate: &str, hex: char) -> SuppliedTargetIrResource {
        SuppliedTargetIrResource::new(coordinate, digest(hex))
            .expect("digest-locked target IR resource")
    }

    fn source_artifact(
        logical_path: &str,
        coordinate: &str,
        hex: char,
    ) -> ContractBundleSourceArtifact {
        ContractBundleSourceArtifact::new(logical_path, coordinate, digest(hex))
            .expect("source artifact descriptor")
    }

    fn assurance(role: AssuranceRole, hex: char) -> ContractBundleAssuranceEvidenceInput {
        ContractBundleAssuranceEvidenceInput::new(
            role,
            BundleSubjectKind::Semantic,
            "holmes.bundle-report/v1",
            digest(hex),
        )
        .expect("assurance evidence input")
    }

    fn assembly_input() -> ContractBundleAssemblyInput {
        ContractBundleAssemblyInput {
            core_module: bounded_hello_core(),
            core_ir_coordinate: "edict.core.bounded-hello/v1".to_owned(),
            source_artifacts: vec![source_artifact(
                "contracts/hello.edict",
                "source.contracts.hello",
                'e',
            )],
            source_profile_semantic_facts: resource("source-profile.hello/v1", 'f'),
            target_profile: resource("echo.dpo@1", 'c'),
            target_ir: target_ir("echo.span-ir/v1", 'd'),
            lawpacks: vec![resource("hello.optics@1", '2')],
            generated_artifacts: vec![resource("echo.dpo.registration/v1", '3')],
            compiler: resource("edict.compiler/v1", '4'),
            lowerer: resource("echo.dpo.lowerer/v1", '5'),
            verifier: resource("echo.dpo.verifier/v1", '6'),
            semantic_compile_options: resource("edict.compile-options.semantic/v1", '7'),
            non_semantic_compile_options: resource("edict.compile-options.nonsemantic/v1", '8'),
            build_provenance: resource("edict.build-provenance/v1", '9'),
            canonicalization_profile: resource("edict.canonical-cbor/v1", '8'),
            conformance_fixture_corpora: vec![resource("echo.dpo.fixtures/v1", '9')],
            verifier_report: resource("echo.dpo.verifier-report/v1", 'a'),
            compile_explanation: resource("watson.compile-explanation/v1", 'b'),
            assurance_evidence: Vec::new(),
        }
    }

    fn assembly_from_target_ir_input() -> ContractBundleAssemblyFromTargetIrInput {
        let (core_module, target_ir_artifact) = effectful_core_and_target_ir();
        ContractBundleAssemblyFromTargetIrInput {
            core_module,
            core_ir_coordinate: "edict.core.effectful-replace/v1".to_owned(),
            source_artifacts: vec![source_artifact(
                "contracts/effectful-replace.edict",
                "source.contracts.effectful-replace",
                'e',
            )],
            source_profile_semantic_facts: resource("source-profile.effectful/v1", 'f'),
            target_ir_artifact,
            lawpacks: vec![resource("hello.optics@1", '2')],
            generated_artifacts: vec![resource("echo.dpo.registration/v1", '3')],
            compiler: resource("edict.compiler/v1", '4'),
            lowerer: resource("echo.dpo.lowerer/v1", '5'),
            verifier: resource("echo.dpo.verifier/v1", '6'),
            semantic_compile_options: resource("edict.compile-options.semantic/v1", '7'),
            non_semantic_compile_options: resource("edict.compile-options.nonsemantic/v1", '8'),
            build_provenance: resource("edict.build-provenance/v1", '9'),
            canonicalization_profile: resource("edict.canonical-cbor/v1", '8'),
            conformance_fixture_corpora: vec![resource("echo.dpo.fixtures/v1", '9')],
            verifier_report: resource("echo.dpo.verifier-report/v1", 'a'),
            compile_explanation: resource("watson.compile-explanation/v1", 'b'),
            assurance_evidence: Vec::new(),
        }
    }

    fn assembly_from_provider_closed_target_ir_input() -> ContractBundleAssemblyFromTargetIrInput {
        let mut input = assembly_from_target_ir_input();
        input
            .core_module
            .imports
            .retain(|import| import.kind != CoreImportKind::Lawpack);
        input.target_ir_artifact = lower_to_target_ir(&input.core_module, &echo_facts())
            .artifact
            .expect("closure-free Core still lowers");
        assert!(input.target_ir_artifact.semantic_closure.is_none());
        input.target_ir_artifact.semantic_closure = Some(TargetIrSemanticClosure {
            source_core: ResourceRef {
                coordinate: input.core_module.coordinate.clone(),
                digest: Some(
                    digest_core_module(&input.core_module)
                        .expect("Core digest")
                        .to_review_string(),
                ),
            },
            lawpacks: input
                .lawpacks
                .iter()
                .map(DigestLockedResource::to_resource_ref)
                .collect(),
            capabilities: Vec::new(),
        });
        input
    }

    fn effectful_core_and_target_ir() -> (edict_syntax::CoreModule, TargetIrArtifact) {
        let module =
            edict_syntax::parse_module(EFFECTFUL_REPLACE).expect("effectful source parses");
        let core = compile_to_core(&module, &effectful_context())
            .expect("effectful source compiles to Core");
        let artifact = lower_to_target_ir(&core, &echo_facts())
            .artifact
            .expect("effectful Core lowers to Echo Target IR");
        (core, artifact)
    }

    fn effectful_context() -> CompilerContext {
        CompilerContext::new()
            .with_operation_profile("p.effectful", "continuum.profile.write/v1")
            .with_operation_profile_write_classes("p.effectful", [WriteClass::Replace])
            .with_effect_write_class("target.replace", WriteClass::Replace)
            .with_budget(
                "p.tiny",
                CoreBudget {
                    max_steps: 8,
                    max_allocated_bytes: 1024,
                    max_output_bytes: 256,
                },
            )
    }

    fn echo_facts() -> TargetIrLoweringFacts {
        TargetIrLoweringFacts {
            target_profile: ResourceRef {
                coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
                digest: Some(digest('c')),
            },
            target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
            operation_profiles: vec!["continuum.profile.write/v1".to_owned()],
            obstruction_coordinates: vec!["rejected".to_owned()],
            effect_lowerings: vec![TargetEffectLowering {
                effect: "target.replace".to_owned(),
                target_intrinsic: "echo.dpo@1.replace".to_owned(),
                failure_mappings: std::collections::BTreeMap::new(),
            }],
            effect_signatures: Vec::new(),
            pure_functions: Vec::new(),
        }
    }

    fn assembled(input: ContractBundleAssemblyInput) -> edict_syntax::ContractBundleManifest {
        assemble_contract_bundle(input).expect("bundle assembles")
    }

    fn assembled_from_target_ir(
        input: ContractBundleAssemblyFromTargetIrInput,
    ) -> edict_syntax::ContractBundleManifest {
        assemble_contract_bundle_from_target_ir(input).expect("bundle assembles from Target IR")
    }

    fn assert_valid(manifest: &edict_syntax::ContractBundleManifest) {
        let report = edict_syntax::validate_contract_bundle_manifest(manifest);
        assert_eq!(report.status, ContractBundleValidationStatus::Valid);
        assert_eq!(
            report.failures,
            [] as [edict_syntax::ContractBundleValidationFailure; 0]
        );
    }

    fn assert_semantic_mutation_changes(
        label: &str,
        mutate: impl FnOnce(&mut ContractBundleAssemblyInput),
    ) {
        let baseline = assembled(assembly_input());
        let mut input = assembly_input();
        mutate(&mut input);
        let changed = assembled(input);

        assert_ne!(
            baseline.semantic_bundle_digest, changed.semantic_bundle_digest,
            "{label}: semantic digest"
        );
        assert_ne!(
            baseline.release_bundle_digest, changed.release_bundle_digest,
            "{label}: release digest"
        );
    }

    fn assert_release_only_mutation_changes(
        label: &str,
        mutate: impl FnOnce(&mut ContractBundleAssemblyInput),
    ) {
        let baseline = assembled(assembly_input());
        let mut input = assembly_input();
        mutate(&mut input);
        let changed = assembled(input);

        assert_eq!(
            baseline.semantic_bundle_digest, changed.semantic_bundle_digest,
            "{label}: semantic digest"
        );
        assert_ne!(
            baseline.release_bundle_digest, changed.release_bundle_digest,
            "{label}: release digest"
        );
    }

    #[test]
    fn assembled_bundle_from_real_core_validates() {
        let input = assembly_input();
        let expected_core_digest = digest_core_module(&input.core_module)
            .expect("Core module digests")
            .to_review_string();

        let manifest = assembled(input);

        assert_eq!(
            manifest.core_ir.digest.as_deref(),
            Some(expected_core_digest.as_str())
        );
        assert_eq!(manifest.admission_artifacts, Vec::new());
        assert_valid(&manifest);
    }

    #[test]
    fn assembled_bundle_from_real_target_ir_computes_target_ir_digest() {
        let input = assembly_from_target_ir_input();
        let expected_target_profile = input.target_ir_artifact.target_profile.clone();
        let expected_target_ir_digest = digest_target_ir_artifact(&input.target_ir_artifact)
            .expect("Target IR artifact digests")
            .to_review_string();

        let manifest = assembled_from_target_ir(input);

        assert_eq!(manifest.target_profile, expected_target_profile);
        assert_eq!(manifest.target_ir.coordinate, ECHO_SPAN_IR_DOMAIN);
        assert_eq!(
            manifest.target_ir.digest.as_deref(),
            Some(expected_target_ir_digest.as_str())
        );
        assert_valid(&manifest);

        let baseline = manifest;
        let mut changed_input = assembly_from_target_ir_input();
        changed_input
            .target_ir_artifact
            .intents
            .get_mut("t")
            .expect("intent t")
            .core_evaluation_budget
            .max_steps += 1;
        let changed = assembled_from_target_ir(changed_input);

        assert_ne!(
            baseline.semantic_bundle_digest, changed.semantic_bundle_digest,
            "computed Target IR digest moves semantic bundle digest"
        );
        assert_ne!(
            baseline.release_bundle_digest, changed.release_bundle_digest,
            "computed Target IR digest moves release bundle digest"
        );
    }

    #[test]
    fn assembly_from_target_ir_rejects_mismatched_core_source() {
        let mut input = assembly_from_target_ir_input();
        input.target_ir_artifact.source_core_coordinate = "a.different@1".to_owned();

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("computed Target IR path rejects mismatched Core source");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(err.field(), "target_ir_artifact.source_core_coordinate");
    }

    #[test]
    fn assembly_from_target_ir_rejects_core_digest_substitution() {
        let mut input = assembly_from_target_ir_input();
        input
            .core_module
            .intents
            .get_mut("t")
            .expect("intent t")
            .core_evaluation_budget
            .max_steps += 1;

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("Target IR closure must bind the supplied Core digest");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(
            err.field(),
            "target_ir_artifact.semantic_closure.source_core"
        );
    }

    #[test]
    fn assembly_from_target_ir_rejects_stripped_lawpack_only_closure() {
        let mut input = assembly_from_target_ir_input();
        input.target_ir_artifact.semantic_closure = None;

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("lawpack-bearing Core requires its Target IR closure");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(err.field(), "target_ir_artifact.semantic_closure");
    }

    #[test]
    fn assembly_from_target_ir_rejects_lawpack_set_substitution() {
        let mut input = assembly_from_target_ir_input();
        input.lawpacks[0] = resource("hello.optics@1", 'e');

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("bundle lawpacks must equal the Target IR closure");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(err.field(), "lawpacks");
    }

    #[test]
    fn assembly_from_target_ir_accepts_exact_provider_supplied_closure() {
        let manifest = assemble_contract_bundle_from_target_ir(
            assembly_from_provider_closed_target_ir_input(),
        )
        .expect("exact provider-supplied semantic closure assembles");

        assert_eq!(
            manifest.lawpacks,
            vec![resource("hello.optics@1", '2').to_resource_ref()]
        );
    }

    #[test]
    fn assembly_from_target_ir_rejects_provider_closure_core_substitution() {
        let mut input = assembly_from_provider_closed_target_ir_input();
        input
            .target_ir_artifact
            .semantic_closure
            .as_mut()
            .expect("provider-supplied closure")
            .source_core
            .digest = Some(digest('e'));

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("provider closure must bind the exact supplied Core");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(
            err.field(),
            "target_ir_artifact.semantic_closure.source_core"
        );
    }

    #[test]
    fn assembly_from_target_ir_rejects_provider_closure_lawpack_substitution() {
        let mut input = assembly_from_provider_closed_target_ir_input();
        input
            .target_ir_artifact
            .semantic_closure
            .as_mut()
            .expect("provider-supplied closure")
            .lawpacks[0]
            .digest = Some(digest('e'));

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("provider closure lawpacks must equal explicit bundle lawpacks");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(err.field(), "lawpacks");
    }

    #[test]
    fn assembly_from_target_ir_rejects_missing_provider_closure() {
        let mut input = assembly_from_provider_closed_target_ir_input();
        input.target_ir_artifact.semantic_closure = None;

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("non-empty bundle lawpacks require a corroborating closure");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(err.field(), "lawpacks");
    }

    #[test]
    fn assembly_from_target_ir_rejects_mismatched_provider_semantic_closure() {
        let mut input = assembly_from_target_ir_input();
        let injected_closure = input
            .target_ir_artifact
            .semantic_closure
            .clone()
            .expect("lawpack-bearing fixture has a closure");
        input
            .core_module
            .imports
            .retain(|import| import.kind != CoreImportKind::Lawpack);
        input.target_ir_artifact = lower_to_target_ir(&input.core_module, &echo_facts())
            .artifact
            .expect("closure-free Core still lowers");
        assert!(input.target_ir_artifact.semantic_closure.is_none());
        input.target_ir_artifact.semantic_closure = Some(injected_closure);
        input.lawpacks.clear();

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("closure-free Core rejects a mismatched provider semantic closure");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(
            err.field(),
            "target_ir_artifact.semantic_closure.source_core"
        );
    }

    #[test]
    fn assembly_from_target_ir_rejects_artifact_lawpack_substitution() {
        let mut input = assembly_from_target_ir_input();
        input
            .target_ir_artifact
            .semantic_closure
            .as_mut()
            .expect("lawpack-bearing fixture has a closure")
            .lawpacks[0]
            .digest = Some(digest('e'));

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("artifact lawpacks must equal the supplied Core closure");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(err.field(), "target_ir_artifact.semantic_closure.lawpacks");
    }

    #[test]
    fn assembly_from_target_ir_rejects_artifact_capability_substitution() {
        let mut input = assembly_from_target_ir_input();
        input
            .target_ir_artifact
            .semantic_closure
            .as_mut()
            .expect("lawpack-bearing fixture has a closure")
            .capabilities
            .push(digest_locked("workspace.snapshot.observe@1", 'e'));

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("artifact capabilities must equal the supplied Core closure");

        assert_eq!(
            err.kind(),
            ContractBundleAssemblyErrorKind::TargetIrSourceMismatch
        );
        assert_eq!(
            err.field(),
            "target_ir_artifact.semantic_closure.capabilities"
        );
    }

    #[test]
    fn assembly_from_target_ir_preserves_canonical_core_failure() {
        let mut input = assembly_from_target_ir_input();
        input
            .core_module
            .intents
            .get_mut("t")
            .expect("intent t")
            .body
            .result = CoreExpr::Const(CoreValue::Int {
            width: "U32".to_owned(),
            value: "4294967296".to_owned(),
        });

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("malformed Core must retain its canonical failure category");

        assert_eq!(err.kind(), ContractBundleAssemblyErrorKind::CanonicalDigest);
        assert_eq!(err.field(), "core_module");
    }

    #[test]
    fn assembly_from_target_ir_canonicalizes_bundle_lawpacks() {
        let mut input = assembly_from_target_ir_input();
        input.core_module.imports.push(CoreImport {
            kind: CoreImportKind::Lawpack,
            resource: ResourceRef {
                coordinate: "zeta.optics@1".to_owned(),
                digest: Some(digest('e')),
            },
            alias: Some("zeta".to_owned()),
        });
        input.target_ir_artifact = lower_to_target_ir(&input.core_module, &echo_facts())
            .artifact
            .expect("two-lawpack Core lowers");
        let hello = resource("hello.optics@1", '2');
        let zeta = resource("zeta.optics@1", 'e');
        input.lawpacks = vec![zeta.clone(), hello.clone(), hello.clone()];

        let manifest = assemble_contract_bundle_from_target_ir(input)
            .expect("equivalent lawpack construction order assembles");

        assert_eq!(
            manifest.lawpacks,
            vec![hello.to_resource_ref(), zeta.to_resource_ref()]
        );
    }

    #[test]
    fn assembly_from_target_ir_rejects_invalid_target_profile_digest_with_stable_field() {
        let mut input = assembly_from_target_ir_input();
        input.target_ir_artifact.target_profile.digest = Some(uppercase_digest());

        let err = assemble_contract_bundle_from_target_ir(input)
            .expect_err("computed Target IR path rejects invalid target profile digest");

        assert_eq!(err.kind(), ContractBundleAssemblyErrorKind::InvalidDigest);
        assert_eq!(err.field(), "target_ir_artifact.target_profile");
    }

    #[test]
    fn assembly_rejects_uppercase_supplied_target_ir_digest() {
        let err = SuppliedTargetIrResource::new("echo.span-ir/v1", uppercase_digest())
            .expect_err("uppercase target IR digest is rejected");

        assert_eq!(err.kind(), ContractBundleAssemblyErrorKind::InvalidDigest);
    }

    #[test]
    fn assembly_rejects_uppercase_supplied_artifact_digest() {
        let err = DigestLockedResource::new("echo.dpo@1", uppercase_digest())
            .expect_err("uppercase artifact digest is rejected");

        assert_eq!(err.kind(), ContractBundleAssemblyErrorKind::InvalidDigest);
    }

    #[test]
    fn assembly_rejects_inputs_that_would_not_validate() {
        let mut empty_sources = assembly_input();
        empty_sources.source_artifacts.clear();
        let err = assemble_contract_bundle(empty_sources)
            .expect_err("empty assembled source artifact set is rejected");
        assert_eq!(err.kind(), ContractBundleAssemblyErrorKind::InvalidManifest);
        assert_eq!(err.field(), "source_artifacts");

        let mut unsupported_canonicalization = assembly_input();
        unsupported_canonicalization.canonicalization_profile =
            resource("edict.custom-cbor/v1", '8');
        let err = assemble_contract_bundle(unsupported_canonicalization)
            .expect_err("unsupported assembled canonicalization profile is rejected");
        assert_eq!(err.kind(), ContractBundleAssemblyErrorKind::InvalidManifest);
        assert_eq!(err.field(), "canonicalization_profile");
    }

    #[test]
    fn target_ir_digest_is_single_source_of_truth() {
        let supplied_target_ir_digest = digest('d');
        let manifest = assembled(assembly_input());

        assert_eq!(
            manifest.target_ir.digest.as_deref(),
            Some(supplied_target_ir_digest.as_str())
        );

        let baseline = manifest;
        let mut changed_input = assembly_input();
        changed_input.target_ir = target_ir("echo.span-ir/v1", 'e');
        let changed = assembled(changed_input);

        assert_ne!(
            baseline.semantic_bundle_digest, changed.semantic_bundle_digest,
            "target IR supplied digest moves semantic digest"
        );
        assert_ne!(
            baseline.release_bundle_digest, changed.release_bundle_digest,
            "target IR supplied digest moves release digest"
        );
    }

    #[test]
    fn semantic_preimage_mutations_change_semantic_and_release_digests() {
        assert_semantic_mutation_changes("Core semantic change", |input| {
            input
                .core_module
                .intents
                .get_mut("sayHello")
                .expect("intent exists")
                .core_evaluation_budget
                .max_steps += 1;
        });
        assert_semantic_mutation_changes("target IR digest", |input| {
            input.target_ir = target_ir("echo.span-ir/v1", 'e');
        });
        assert_semantic_mutation_changes("target profile digest", |input| {
            input.target_profile = resource("echo.dpo@1", 'e');
        });
        assert_semantic_mutation_changes("lawpack digest", |input| {
            input.lawpacks[0] = resource("hello.optics@1", 'e');
        });
        assert_semantic_mutation_changes("source profile semantic facts digest", |input| {
            input.source_profile_semantic_facts = resource("source-profile.hello/v1", 'e');
        });
        assert_semantic_mutation_changes("generated artifact digest", |input| {
            input.generated_artifacts[0] = resource("echo.dpo.registration/v1", 'e');
        });
        assert_semantic_mutation_changes("canonicalization profile digest", |input| {
            input.canonicalization_profile = resource("edict.canonical-cbor/v1", 'e');
        });
        assert_semantic_mutation_changes("semantic compile options digest", |input| {
            input.semantic_compile_options = resource("edict.compile-options.semantic/v1", 'e');
        });
        assert_semantic_mutation_changes("conformance fixture corpus digest", |input| {
            input.conformance_fixture_corpora[0] = resource("echo.dpo.fixtures/v1", 'e');
        });
        assert_semantic_mutation_changes("verifier report digest", |input| {
            input.verifier_report = resource("echo.dpo.verifier-report/v1", 'e');
        });
    }

    #[test]
    fn release_only_preimage_mutations_leave_semantic_digest_unchanged() {
        assert_release_only_mutation_changes("source artifact digest", |input| {
            input.source_artifacts[0] =
                source_artifact("contracts/hello.edict", "source.contracts.hello", 'f');
        });
        assert_release_only_mutation_changes("source logical path", |input| {
            input.source_artifacts[0] =
                source_artifact("contracts/renamed.edict", "source.contracts.hello", 'e');
        });
        assert_release_only_mutation_changes("compiler coordinate", |input| {
            input.compiler = resource("edict.compiler-alt/v1", '4');
        });
        assert_release_only_mutation_changes("compiler digest", |input| {
            input.compiler = resource("edict.compiler/v1", 'f');
        });
        assert_release_only_mutation_changes("lowerer coordinate", |input| {
            input.lowerer = resource("echo.dpo.lowerer-alt/v1", '5');
        });
        assert_release_only_mutation_changes("verifier coordinate", |input| {
            input.verifier = resource("echo.dpo.verifier-alt/v1", '6');
        });
        assert_release_only_mutation_changes("nonsemantic compile options digest", |input| {
            input.non_semantic_compile_options =
                resource("edict.compile-options.nonsemantic/v1", 'f');
        });
        assert_release_only_mutation_changes("build provenance digest", |input| {
            input.build_provenance = resource("edict.build-provenance/v1", 'f');
        });
        assert_release_only_mutation_changes("compile explanation digest", |input| {
            input.compile_explanation = resource("watson.compile-explanation/v1", 'f');
        });
    }

    #[test]
    fn optional_assurance_evidence_is_not_a_top_level_digest_preimage() {
        let baseline = assembled(assembly_input());

        let mut with_holmes = assembly_input();
        with_holmes.assurance_evidence = vec![assurance(AssuranceRole::Holmes, '1')];
        let holmes = assembled(with_holmes);
        assert_valid(&holmes);

        let mut changed_holmes = assembly_input();
        changed_holmes.assurance_evidence = vec![assurance(AssuranceRole::Holmes, '2')];
        let changed_holmes = assembled(changed_holmes);
        assert_valid(&changed_holmes);

        assert_eq!(
            baseline.semantic_bundle_digest,
            holmes.semantic_bundle_digest
        );
        assert_eq!(baseline.release_bundle_digest, holmes.release_bundle_digest);
        assert_eq!(
            holmes.semantic_bundle_digest,
            changed_holmes.semantic_bundle_digest
        );
        assert_eq!(
            holmes.release_bundle_digest,
            changed_holmes.release_bundle_digest
        );

        let mut subject_mismatch = holmes.clone();
        subject_mismatch.assurance_evidence[0].subject.digest = digest('f');
        let report = edict_syntax::validate_contract_bundle_manifest(&subject_mismatch);
        assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
        assert_eq!(
            failure_kinds(&report),
            vec![ContractBundleValidationFailureKind::AssuranceSubjectMismatch]
        );

        let mut target_mismatch = holmes;
        target_mismatch.assurance_evidence[0].target_ir_digest = digest('f');
        let report = edict_syntax::validate_contract_bundle_manifest(&target_mismatch);
        assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
        assert_eq!(
            failure_kinds(&report),
            vec![ContractBundleValidationFailureKind::AssuranceTargetIrMismatch]
        );
    }

    #[test]
    fn assembled_bundle_rejects_inserted_admission_artifacts() {
        let mut manifest = assembled(assembly_input());
        assert_eq!(
            manifest.admission_artifacts,
            [] as [edict_syntax::ResourceRef; 0]
        );

        manifest
            .admission_artifacts
            .push(digest_locked("continuum.admission-receipt/v1", 'f'));
        let report = edict_syntax::validate_contract_bundle_manifest(&manifest);

        assert_eq!(report.status, ContractBundleValidationStatus::Invalid);
        assert_eq!(
            failure_kinds(&report),
            vec![ContractBundleValidationFailureKind::AdmissionArtifactUnsupported]
        );
    }
}
