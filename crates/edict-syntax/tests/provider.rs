//! Provider manifest validation for generated-artifact provenance.
//!
//! These tests assert the public provider-boundary contract. They do not load
//! providers, execute lowerers, validate runtime-specific semantics, or inspect
//! documentation prose.

use edict_syntax::{
    bind_target_provider_manifest, select_provider_component, validate_target_provider_manifest,
    ProviderArtifactKind, ProviderArtifactSource, ProviderComponentSelectionFailureKind,
    ProviderInvocationKind, ProviderManifestValidationFailureKind,
    ProviderManifestValidationStatus, ResourceRef, TargetProviderManifest, TARGET_PROVIDER_ABI,
};

const ECHO_PROVIDER_FIXTURE: &str =
    include_str!("../../../fixtures/providers/echo-generated/provider-manifest.json");

fn digest_locked(coordinate: &str, digit: char) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(format!("sha256:{}", digit.to_string().repeat(64))),
    }
}

fn fixture_manifest() -> TargetProviderManifest {
    serde_json::from_str(ECHO_PROVIDER_FIXTURE).expect("provider fixture should deserialize")
}

fn failure_kinds(manifest: &TargetProviderManifest) -> Vec<ProviderManifestValidationFailureKind> {
    validate_target_provider_manifest(manifest)
        .failures
        .iter()
        .map(|failure| failure.kind)
        .collect()
}

#[test]
fn generated_provider_manifest_fixture_validates() {
    let manifest = fixture_manifest();

    let report = validate_target_provider_manifest(&manifest);

    assert_eq!(report.status, ProviderManifestValidationStatus::Valid);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::ProviderManifestValidationFailure; 0]
    );
}

#[test]
fn generation_provenance_is_generated_provider_metadata() {
    let manifest = fixture_manifest();
    let generation_provenance = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.role == "generation-provenance.echo")
        .expect("fixture should declare generation provenance");

    assert_eq!(
        generation_provenance.artifact_kind,
        ProviderArtifactKind::GenerationProvenance
    );
    assert!(matches!(
        generation_provenance.source,
        ProviderArtifactSource::Generated { .. }
    ));
    let encoded = serde_json::to_value(generation_provenance)
        .expect("generation provenance artifact should serialize");
    assert_eq!(encoded["artifactKind"], "generationProvenance");
    let decoded =
        serde_json::from_value(encoded).expect("generation provenance artifact should deserialize");
    assert_eq!(generation_provenance, &decoded);

    let mut component_sourced = manifest;
    let generation_provenance = component_sourced
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.role == "generation-provenance.echo")
        .expect("fixture should declare generation provenance");
    generation_provenance.source = ProviderArtifactSource::Component {
        component: generation_provenance.resource.clone(),
    };

    assert_eq!(
        failure_kinds(&component_sourced),
        vec![ProviderManifestValidationFailureKind::GeneratedRoleRequiresGeneratedSource]
    );
}

#[test]
fn provider_manifest_rejects_unknown_api_version() {
    let mut manifest = fixture_manifest();
    manifest.api_version = "edict.provider-manifest/v2".to_owned();

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::InvalidApiVersion]
    );
}

#[test]
fn provider_manifest_requires_the_exact_provider_abi() {
    let mut manifest = fixture_manifest();
    manifest.provider_abi = "edict:target-provider@1.0.1".to_owned();

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::InvalidProviderAbi]
    );
    assert_eq!(TARGET_PROVIDER_ABI, "edict:target-provider@1.0.0");
}

#[test]
fn provider_manifest_rejects_unlocked_provider() {
    let mut manifest = fixture_manifest();
    manifest.provider.digest = None;

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::NonDigestLockedProvider]
    );
}

#[test]
fn provider_manifest_rejects_unlocked_generated_artifact() {
    let mut manifest = fixture_manifest();
    manifest.artifacts[0].resource.digest = None;

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::NonDigestLockedArtifact]
    );
}

#[test]
fn provider_manifest_rejects_unlocked_generated_provenance() {
    let mut manifest = fixture_manifest();
    let ProviderArtifactSource::Generated {
        semantic_source, ..
    } = &mut manifest.artifacts[0].source
    else {
        panic!("fixture lawpack artifact should be generated");
    };
    semantic_source.digest =
        Some("sha256:ABCDEFabcdefABCDEFabcdefABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD".to_owned());

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::NonDigestLockedGeneratedSource]
    );
}

#[test]
fn provider_manifest_rejects_unlocked_generator_provenance() {
    let mut manifest = fixture_manifest();
    let ProviderArtifactSource::Generated { generator, .. } = &mut manifest.artifacts[0].source
    else {
        panic!("fixture lawpack artifact should be generated");
    };
    generator.digest = None;

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::NonDigestLockedGenerator]
    );
}

#[test]
fn provider_manifest_rejects_unlocked_component() {
    let mut manifest = fixture_manifest();
    let ProviderArtifactSource::Component { component } = &mut manifest.artifacts[4].source else {
        panic!("fixture lowerer artifact should be a component");
    };
    component.digest = None;

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::NonDigestLockedComponent]
    );
}

#[test]
fn provider_manifest_rejects_component_identity_disagreement() {
    let mut manifest = fixture_manifest();
    let ProviderArtifactSource::Component { component } = &mut manifest.artifacts[4].source else {
        panic!("fixture lowerer artifact should be a component");
    };
    *component = digest_locked("echo.other-lowerer/component@1", 'b');

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::ComponentResourceMismatch]
    );
}

#[test]
fn provider_manifest_rejects_missing_artifacts() {
    let mut manifest = fixture_manifest();
    manifest.artifacts.clear();

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::MissingArtifact]
    );
}

#[test]
fn provider_manifest_rejects_generated_component_roles() {
    let mut manifest = fixture_manifest();
    manifest.artifacts[4].source = ProviderArtifactSource::Generated {
        semantic_source: digest_locked("echo.semantic-source/v1", '7'),
        generator: digest_locked("echo-wesley.provider-generator/v1", '8'),
    };

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::ComponentRoleRequiresComponentSource]
    );
}

#[test]
fn provider_manifest_rejects_component_metadata_roles() {
    let mut manifest = fixture_manifest();
    manifest.artifacts[0].source = ProviderArtifactSource::Component {
        component: digest_locked("echo.dpo.lowerer/component", '9'),
    };

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::GeneratedRoleRequiresGeneratedSource]
    );
}

#[test]
fn provider_manifest_rejects_duplicate_artifact_roles() {
    let mut manifest = fixture_manifest();
    let duplicate_role = manifest.artifacts[0].role.clone();
    manifest.artifacts[1].role = duplicate_role;

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::DuplicateArtifactRole]
    );
}

#[test]
fn provider_manifest_rejects_empty_artifact_role() {
    let mut manifest = fixture_manifest();
    manifest.artifacts[0].role.clear();

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::MissingRole]
    );
}

#[test]
fn provider_manifest_rejects_path_like_artifact_roles() {
    for unsafe_role in [
        "../outside",
        "nested/role",
        r"nested\role",
        "drive:role",
        "control\nrole",
    ] {
        let mut manifest = fixture_manifest();
        let lowerer = manifest
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.artifact_kind == ProviderArtifactKind::Lowerer)
            .expect("fixture should declare a lowerer");
        lowerer.role = unsafe_role.to_owned();

        let report = validate_target_provider_manifest(&manifest);

        assert_eq!(
            report.status,
            ProviderManifestValidationStatus::Invalid,
            "role {unsafe_role}"
        );
        assert_eq!(report.failures.len(), 1, "role {unsafe_role}");
        assert_eq!(
            report.failures[0].kind,
            ProviderManifestValidationFailureKind::UnsafeArtifactRole,
            "role {unsafe_role}"
        );
        assert_eq!(
            report.failures[0].field, "artifacts.role",
            "role {unsafe_role}"
        );
    }
}

#[test]
fn provider_manifest_requires_schema_bindings() {
    let mut manifest = fixture_manifest();
    manifest.schema_bindings.clear();

    assert_eq!(
        failure_kinds(&manifest),
        vec![ProviderManifestValidationFailureKind::MissingSchemaBinding]
    );
}

#[test]
fn provider_manifest_rejects_missing_duplicate_and_out_of_order_schema_domains() {
    let mut missing = fixture_manifest();
    missing.schema_bindings[0].domain.clear();
    assert_eq!(
        failure_kinds(&missing),
        vec![ProviderManifestValidationFailureKind::MissingSchemaDomain]
    );

    let mut duplicate = fixture_manifest();
    duplicate.schema_bindings[1].domain = duplicate.schema_bindings[0].domain.clone();
    assert_eq!(
        failure_kinds(&duplicate),
        vec![
            ProviderManifestValidationFailureKind::DuplicateSchemaDomain,
            ProviderManifestValidationFailureKind::OutOfOrderSchemaDomain,
        ]
    );

    let mut out_of_order = fixture_manifest();
    out_of_order.schema_bindings.swap(0, 1);
    assert_eq!(
        failure_kinds(&out_of_order),
        vec![ProviderManifestValidationFailureKind::OutOfOrderSchemaDomain]
    );
}

#[test]
fn provider_manifest_rejects_missing_or_unknown_schema_roles() {
    let mut missing = fixture_manifest();
    missing.schema_bindings[0].schema_role.clear();
    assert_eq!(
        failure_kinds(&missing),
        vec![ProviderManifestValidationFailureKind::MissingSchemaRole]
    );

    let mut unknown = fixture_manifest();
    unknown.schema_bindings[0].schema_role = "schema.absent".to_owned();
    assert_eq!(
        failure_kinds(&unknown),
        vec![ProviderManifestValidationFailureKind::UnknownSchemaRole]
    );
}

#[test]
fn provider_manifest_requires_schema_kind_and_root_rule() {
    let mut wrong_kind = fixture_manifest();
    wrong_kind.schema_bindings[0].schema_role = wrong_kind.artifacts[0].role.clone();
    assert_eq!(
        failure_kinds(&wrong_kind),
        vec![ProviderManifestValidationFailureKind::SchemaRoleKindMismatch]
    );

    let mut missing_root = fixture_manifest();
    missing_root.schema_bindings[0].root_rule.clear();
    assert_eq!(
        failure_kinds(&missing_root),
        vec![ProviderManifestValidationFailureKind::MissingSchemaRootRule]
    );
}

#[test]
fn validated_manifest_selects_one_exact_world_contract() {
    let manifest = fixture_manifest();
    let validated = bind_target_provider_manifest(&manifest).expect("fixture manifest validates");

    let lowerer = select_provider_component(
        &validated,
        "lowerer.echo-dpo",
        ProviderInvocationKind::Lowering,
    )
    .expect("lowerer role selects");
    assert_eq!(lowerer.role(), "lowerer.echo-dpo");
    assert_eq!(lowerer.resource(), &manifest.artifacts[4].resource);
    assert_eq!(
        lowerer.contract_identity(),
        "edict:target-provider/lowerer@1.0.0"
    );

    let verifier = select_provider_component(
        &validated,
        "verifier.echo-dpo",
        ProviderInvocationKind::Verification,
    )
    .expect("verifier role selects");
    assert_eq!(
        verifier.contract_identity(),
        "edict:target-provider/verifier@1.0.0"
    );
}

#[test]
fn selected_component_outlives_temporary_manifest_proof() {
    let manifest = fixture_manifest();
    let selected = {
        let validated =
            bind_target_provider_manifest(&manifest).expect("fixture manifest validates");
        select_provider_component(
            &validated,
            "lowerer.echo-dpo",
            ProviderInvocationKind::Lowering,
        )
        .expect("lowerer role selects")
    };

    assert_eq!(selected.manifest(), &manifest);
    assert_eq!(selected.role(), "lowerer.echo-dpo");
}

#[test]
fn component_selection_rejects_unknown_and_wrong_kind_roles() {
    let manifest = fixture_manifest();
    let validated = bind_target_provider_manifest(&manifest).expect("fixture manifest validates");

    let missing = select_provider_component(
        &validated,
        "lowerer.absent",
        ProviderInvocationKind::Lowering,
    )
    .expect_err("unknown role must reject");
    assert_eq!(
        missing.kind(),
        ProviderComponentSelectionFailureKind::ComponentRoleNotFound
    );

    let wrong_world = select_provider_component(
        &validated,
        "verifier.echo-dpo",
        ProviderInvocationKind::Lowering,
    )
    .expect_err("verifier cannot be selected as lowerer");
    assert_eq!(
        wrong_world.kind(),
        ProviderComponentSelectionFailureKind::ComponentKindMismatch
    );
}
