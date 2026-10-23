//! Built-in lowerer compatibility evidence for the provider migration seam.
//!
//! These tests prove that the temporary in-process provider seam preserves the
//! existing direct target-lowering contract. They do not resolve manifests,
//! load components, or define the external WIT provider ABI.

use amblicit_syntax::{
    assemble_contract_bundle_from_target_ir, compile_to_core, digest_target_ir_artifact,
    encode_target_ir_artifact, lower_to_target_ir, lower_with_builtin_lowerer,
    BuiltinLowererCompatibilityFailure, BuiltinLowererCompatibilityFailureKind,
    BuiltinLowererRequest, BuiltinTargetLowerer, CompilerContext,
    ContractBundleAssemblyFromTargetIrInput, ContractBundleSourceArtifact, CoreBudget, CoreModule,
    DigestLockedResource, ResourceRef, TargetEffectLowering, TargetIrArtifact,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringReport, TargetLoweringStatus,
    WriteClass, CANONICAL_CBOR_ABI, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
    GITWARP_COMMIT_REDUCER_IR_DOMAIN, GITWARP_REF_CRDT_TARGET_PROFILE,
};

const EFFECTFUL_REPLACE: &str = "package a.b@1;\n\
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

const GITWARP_APPEND_EVENT: &str = "package a.git@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.gitwarp\n\
      basis none\n\
      budget <= p.tiny\n\
      where input.id != \"\" {\n\
      let receipt: Receipt = gitwarp.appendEvent(input.id)\n\
        else { conflict(reason) => domain.MergeConflict };\n\
      return { id: receipt.id };\n\
    }";

#[derive(Debug)]
struct LoweringFixture {
    name: &'static str,
    lowerer: BuiltinTargetLowerer,
    lowerer_coordinate: &'static str,
    core: CoreModule,
    facts: TargetIrLoweringFacts,
}

fn digest(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
}

fn resource(coordinate: &str, hex: char) -> DigestLockedResource {
    DigestLockedResource::new(coordinate, digest(hex)).expect("digest-locked resource")
}

fn source_artifact(
    logical_path: &str,
    coordinate: &str,
    hex: char,
) -> ContractBundleSourceArtifact {
    ContractBundleSourceArtifact::new(logical_path, coordinate, digest(hex))
        .expect("source artifact")
}

fn echo_fixture() -> LoweringFixture {
    let module = amblicit_syntax::parse_module(EFFECTFUL_REPLACE).expect("Echo source parses");
    let context = CompilerContext::new()
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
        );
    let core = compile_to_core(&module, &context).expect("Echo source compiles to Core");
    let facts = TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(digest('1')),
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
    };

    LoweringFixture {
        name: "echo",
        lowerer: BuiltinTargetLowerer::EchoDpo,
        lowerer_coordinate: "echo.dpo.lowerer/v1",
        core,
        facts,
    }
}

fn gitwarp_fixture() -> LoweringFixture {
    let module =
        amblicit_syntax::parse_module(GITWARP_APPEND_EVENT).expect("git-warp source parses");
    let context = CompilerContext::new()
        .with_operation_profile("p.gitwarp", "continuum.profile.append/v1")
        .with_operation_profile_write_classes("p.gitwarp", [WriteClass::Append])
        .with_effect_write_class("gitwarp.appendEvent", WriteClass::Append)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 13,
                max_allocated_bytes: 2048,
                max_output_bytes: 512,
            },
        );
    let core = compile_to_core(&module, &context).expect("git-warp source compiles to Core");
    let facts = TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: GITWARP_REF_CRDT_TARGET_PROFILE.to_owned(),
            digest: Some(digest('2')),
        },
        target_ir_domain: GITWARP_COMMIT_REDUCER_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.append/v1".to_owned()],
        obstruction_coordinates: vec!["conflict".to_owned()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "gitwarp.appendEvent".to_owned(),
            target_intrinsic: "gitwarp.ref_crdt@1.appendEvent".to_owned(),
            failure_mappings: std::collections::BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    };

    LoweringFixture {
        name: "gitwarp",
        lowerer: BuiltinTargetLowerer::GitwarpRefCrdt,
        lowerer_coordinate: "gitwarp.ref-crdt.lowerer/v1",
        core,
        facts,
    }
}

fn provider_report(fixture: &LoweringFixture) -> TargetLoweringReport {
    lower_with_builtin_lowerer(
        fixture.lowerer,
        BuiltinLowererRequest {
            core: &fixture.core,
            facts: &fixture.facts,
        },
    )
    .expect("fixture lowerer matches its target profile")
}

fn assert_success_parity(fixture: &LoweringFixture) {
    let direct = lower_to_target_ir(&fixture.core, &fixture.facts);
    let provider = provider_report(fixture);

    assert_eq!(provider, direct, "{} report parity", fixture.name);
    let direct_artifact = direct.artifact.as_ref().expect("direct artifact");
    let provider_artifact = provider.artifact.as_ref().expect("provider artifact");
    assert_eq!(
        provider_artifact, direct_artifact,
        "{} artifact",
        fixture.name
    );
    assert_eq!(
        encode_target_ir_artifact(provider_artifact).expect("provider canonical bytes"),
        encode_target_ir_artifact(direct_artifact).expect("direct canonical bytes"),
        "{} canonical bytes",
        fixture.name
    );
    assert_eq!(
        digest_target_ir_artifact(provider_artifact).expect("provider digest"),
        digest_target_ir_artifact(direct_artifact).expect("direct digest"),
        "{} digest",
        fixture.name
    );
}

fn bundle_input(
    fixture: &LoweringFixture,
    artifact: TargetIrArtifact,
    lowerer_coordinate: &str,
) -> ContractBundleAssemblyFromTargetIrInput {
    ContractBundleAssemblyFromTargetIrInput {
        core_module: fixture.core.clone(),
        core_ir_coordinate: format!("edict.core.provider-{}-fixture/v1", fixture.name),
        source_artifacts: vec![source_artifact(
            &format!("contracts/provider-{}.edict", fixture.name),
            &format!("source.contracts.provider-{}", fixture.name),
            'e',
        )],
        source_profile_semantic_facts: resource("source-profile.provider-fixture/v1", 'f'),
        target_ir_artifact: artifact,
        lawpacks: Vec::new(),
        generated_artifacts: vec![resource("provider-fixture.registration/v1", '3')],
        compiler: resource("edict.compiler/v1", '4'),
        lowerer: resource(lowerer_coordinate, '5'),
        verifier: resource("provider-fixture.verifier/v1", '6'),
        semantic_compile_options: resource("edict.compile-options.semantic/v1", '7'),
        non_semantic_compile_options: resource("edict.compile-options.nonsemantic/v1", '8'),
        build_provenance: resource("edict.build-provenance/v1", '9'),
        canonicalization_profile: resource(CANONICAL_CBOR_ABI, '8'),
        conformance_fixture_corpora: vec![resource("provider-fixture.corpus/v1", '9')],
        verifier_report: resource("provider-fixture.verifier-report/v1", 'a'),
        compile_explanation: resource("provider-fixture.explanation/v1", 'b'),
        assurance_evidence: Vec::new(),
    }
}

#[test]
fn builtin_echo_lowerer_matches_direct_target_ir() {
    assert_success_parity(&echo_fixture());
}

#[test]
fn builtin_gitwarp_lowerer_matches_direct_target_ir() {
    assert_success_parity(&gitwarp_fixture());
}

#[test]
fn builtin_lowerers_preserve_structured_lowering_failures() {
    for mut fixture in [echo_fixture(), gitwarp_fixture()] {
        fixture.facts.effect_lowerings.clear();
        let direct = lower_to_target_ir(&fixture.core, &fixture.facts);
        let provider = provider_report(&fixture);

        assert_eq!(provider, direct, "{} failure report", fixture.name);
        assert_eq!(provider.status, TargetLoweringStatus::Unsupported);
        assert!(provider.artifact.is_none());
        assert_eq!(
            provider.failures[0].kind,
            TargetLoweringFailureKind::MissingEffectLowering
        );
    }
}

#[test]
fn builtin_lowerers_preserve_target_profile_digest_failures() {
    for fixture in [echo_fixture(), gitwarp_fixture()] {
        for digest in [None, Some("not-a-digest".to_owned())] {
            let mut facts = fixture.facts.clone();
            facts.target_profile.digest = digest;
            let direct = lower_to_target_ir(&fixture.core, &facts);
            let provider = lower_with_builtin_lowerer(
                fixture.lowerer,
                BuiltinLowererRequest {
                    core: &fixture.core,
                    facts: &facts,
                },
            )
            .expect("matching profile coordinate invokes the built-in lowerer");

            assert_eq!(provider, direct, "{} digest failure", fixture.name);
            assert_eq!(provider.status, TargetLoweringStatus::Unsupported);
            assert!(provider.artifact.is_none());
            assert_eq!(
                provider.failures[0].kind,
                TargetLoweringFailureKind::UndigestedTargetProfile
            );
        }
    }
}

#[test]
fn builtin_lowerers_reject_mismatched_target_profiles() {
    let echo = echo_fixture();
    let gitwarp = gitwarp_fixture();

    for (lowerer, fixture, expected_profile) in [
        (
            BuiltinTargetLowerer::EchoDpo,
            &gitwarp,
            ECHO_DPO_TARGET_PROFILE,
        ),
        (
            BuiltinTargetLowerer::GitwarpRefCrdt,
            &echo,
            GITWARP_REF_CRDT_TARGET_PROFILE,
        ),
    ] {
        let failure = lower_with_builtin_lowerer(
            lowerer,
            BuiltinLowererRequest {
                core: &fixture.core,
                facts: &fixture.facts,
            },
        )
        .expect_err("cross-profile lowerer selection rejects");

        assert_eq!(
            failure.kind,
            BuiltinLowererCompatibilityFailureKind::TargetProfileMismatch
        );
        assert_eq!(failure.lowerer, lowerer);
        assert_eq!(failure.expected_target_profile, expected_profile);
        assert_eq!(
            failure.actual_target_profile,
            fixture.facts.target_profile.coordinate
        );
    }
}

#[test]
fn builtin_lowerer_compatibility_failure_is_standard_error() {
    fn assert_standard_error(error: &(dyn std::error::Error + 'static)) {
        assert!(error.source().is_none());
    }

    let failure = BuiltinLowererCompatibilityFailure {
        kind: BuiltinLowererCompatibilityFailureKind::TargetProfileMismatch,
        lowerer: BuiltinTargetLowerer::EchoDpo,
        expected_target_profile: ECHO_DPO_TARGET_PROFILE.to_owned(),
        actual_target_profile: GITWARP_REF_CRDT_TARGET_PROFILE.to_owned(),
    };

    assert_standard_error(&failure);

    let rendered = failure.to_string();
    for expected in [
        "TargetProfileMismatch",
        "EchoDpo",
        ECHO_DPO_TARGET_PROFILE,
        GITWARP_REF_CRDT_TARGET_PROFILE,
    ] {
        assert!(
            rendered.contains(expected),
            "compatibility error display omitted {expected:?}: {rendered}"
        );
    }
}

#[test]
fn builtin_lowerer_bundles_preserve_semantic_and_release_identity() {
    for fixture in [echo_fixture(), gitwarp_fixture()] {
        let direct_artifact = lower_to_target_ir(&fixture.core, &fixture.facts)
            .artifact
            .expect("direct artifact");
        let provider_artifact = provider_report(&fixture)
            .artifact
            .expect("provider artifact");

        let direct = assemble_contract_bundle_from_target_ir(bundle_input(
            &fixture,
            direct_artifact,
            fixture.lowerer_coordinate,
        ))
        .expect("direct bundle");
        let provider = assemble_contract_bundle_from_target_ir(bundle_input(
            &fixture,
            provider_artifact,
            fixture.lowerer_coordinate,
        ))
        .expect("provider bundle");

        assert_eq!(provider, direct, "{} bundle parity", fixture.name);
    }
}

#[test]
fn changing_builtin_lowerer_identity_changes_only_release_identity() {
    let fixture = echo_fixture();
    let artifact = provider_report(&fixture)
        .artifact
        .expect("provider artifact");
    let baseline = assemble_contract_bundle_from_target_ir(bundle_input(
        &fixture,
        artifact.clone(),
        fixture.lowerer_coordinate,
    ))
    .expect("baseline bundle");
    let changed = assemble_contract_bundle_from_target_ir(bundle_input(
        &fixture,
        artifact,
        "echo.dpo.lowerer.alternative/v1",
    ))
    .expect("alternate-lowerer bundle");

    assert_eq!(baseline.target_ir, changed.target_ir);
    assert_eq!(
        baseline.semantic_bundle_digest,
        changed.semantic_bundle_digest
    );
    assert_ne!(
        baseline.release_bundle_digest,
        changed.release_bundle_digest
    );
}
