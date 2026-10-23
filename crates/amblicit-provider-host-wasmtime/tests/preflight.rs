use std::borrow::Cow;
use std::sync::Arc;

use amblicit_provider_host_wasmtime::{
    ProviderComponentHost, ProviderHostFailureKind, ProviderHostPhase, ResolvedProviderComponent,
    PROVIDER_CONTRACT_CUSTOM_SECTION,
};
use amblicit_syntax::{
    bind_target_provider_manifest, select_provider_component, ProviderArtifactKind,
    ProviderArtifactRef, ProviderArtifactSource, ProviderInvocationKind, ProviderSchemaBinding,
    ProviderSchemaFormat, ResourceRef, TargetProviderManifest, TARGET_PROVIDER_ABI,
    TARGET_PROVIDER_LOWERER_CONTRACT, TARGET_PROVIDER_MANIFEST_API_VERSION,
};
use sha2::{Digest, Sha256};
use wasm_encoder::{CustomSection, Encode, Section};

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn resource(coordinate: &str, digest: String) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(digest),
    }
}

fn append_contract_section(bytes: &mut Vec<u8>, contract: &[u8]) {
    let section = CustomSection {
        name: Cow::Borrowed(PROVIDER_CONTRACT_CUSTOM_SECTION),
        data: Cow::Borrowed(contract),
    };
    bytes.push(section.id());
    section.encode(bytes);
}

fn resolved_component(
    authorized_bytes: &[u8],
    resolved_bytes: Vec<u8>,
) -> ResolvedProviderComponent<'static> {
    let component = resource("runtime.lowerer/component@1", digest(authorized_bytes));
    let schema = resource(
        "runtime.artifacts.cddl@1",
        format!("sha256:{}", "a".repeat(64)),
    );
    let manifest = Box::leak(Box::new(TargetProviderManifest {
        api_version: TARGET_PROVIDER_MANIFEST_API_VERSION.to_owned(),
        provider_abi: TARGET_PROVIDER_ABI.to_owned(),
        provider: resource("runtime.provider@1", format!("sha256:{}", "b".repeat(64))),
        artifacts: vec![
            ProviderArtifactRef {
                role: "lowerer.runtime".to_owned(),
                artifact_kind: ProviderArtifactKind::Lowerer,
                resource: component.clone(),
                source: ProviderArtifactSource::Component {
                    component: component.clone(),
                },
            },
            ProviderArtifactRef {
                role: "schema.runtime".to_owned(),
                artifact_kind: ProviderArtifactKind::ArtifactSchema,
                resource: schema,
                source: ProviderArtifactSource::Generated {
                    semantic_source: resource(
                        "runtime.semantic-source@1",
                        format!("sha256:{}", "c".repeat(64)),
                    ),
                    generator: resource(
                        "runtime.provider-generator@1",
                        format!("sha256:{}", "d".repeat(64)),
                    ),
                },
            },
        ],
        schema_bindings: vec![ProviderSchemaBinding {
            domain: "runtime.output/v1".to_owned(),
            schema_role: "schema.runtime".to_owned(),
            format: ProviderSchemaFormat::SelfContainedCddlV1,
            root_rule: "output".to_owned(),
        }],
    }));
    let validated = Box::leak(Box::new(
        bind_target_provider_manifest(manifest).expect("test manifest validates"),
    ));
    let selected = select_provider_component(
        validated,
        "lowerer.runtime",
        ProviderInvocationKind::Lowering,
    )
    .expect("lowerer selects");
    ResolvedProviderComponent::new(selected, Arc::from(resolved_bytes))
}

#[test]
fn digest_mismatch_rejects_before_component_decoding() {
    let authorized = wat::parse_str("(component)").expect("valid component");
    let resolved = resolved_component(&authorized, b"not a component".to_vec());
    let host = ProviderComponentHost::new().expect("host configures");

    let failure = host
        .prepare(&resolved)
        .expect_err("wrong bytes must reject before decode");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentDigestMismatch
    );
}

#[test]
fn digest_matching_invalid_bytes_have_a_stable_decode_failure() {
    let bytes = b"not a component".to_vec();
    let resolved = resolved_component(&bytes, bytes.clone());
    let host = ProviderComponentHost::new().expect("host configures");

    let failure = host.prepare(&resolved).expect_err("invalid bytes reject");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentDecodeFailed
    );
}

#[test]
fn exact_contract_attestation_precedes_structural_type_checking() {
    let host = ProviderComponentHost::new().expect("host configures");

    let missing = wat::parse_str("(component)").expect("valid component");
    let failure = host
        .prepare(&resolved_component(&missing, missing.clone()))
        .expect_err("missing attestation rejects");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentContractMismatch
    );

    let nested = wat::parse_str(
        r#"(component
            (core module
                (@custom "edict:target-provider-contract"
                    "edict:target-provider/lowerer@1.0.0"))
        )"#,
    )
    .expect("nested custom section component parses");
    let failure = host
        .prepare(&resolved_component(&nested, nested.clone()))
        .expect_err("nested attestation cannot authorize the outer component");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentContractMismatch
    );

    let mut wrong = wat::parse_str("(component)").expect("valid component");
    append_contract_section(&mut wrong, b"edict:target-provider/lowerer@1.0.1");
    let failure = host
        .prepare(&resolved_component(&wrong, wrong.clone()))
        .expect_err("wrong version rejects");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentContractMismatch
    );

    let mut duplicate = wat::parse_str("(component)").expect("valid component");
    append_contract_section(&mut duplicate, TARGET_PROVIDER_LOWERER_CONTRACT.as_bytes());
    append_contract_section(&mut duplicate, TARGET_PROVIDER_LOWERER_CONTRACT.as_bytes());
    let failure = host
        .prepare(&resolved_component(&duplicate, duplicate.clone()))
        .expect_err("duplicate attestation rejects");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentContractMismatch
    );
}

#[test]
fn exact_attestation_does_not_replace_structural_contract_validation() {
    let mut bytes = wat::parse_str("(component)").expect("valid component");
    append_contract_section(&mut bytes, TARGET_PROVIDER_LOWERER_CONTRACT.as_bytes());
    let resolved = resolved_component(&bytes, bytes.clone());
    let host = ProviderComponentHost::new().expect("host configures");

    let failure = host
        .prepare(&resolved)
        .expect_err("empty component is not a lowerer");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentContractMismatch
    );
}

#[test]
fn checked_in_lowerer_fixture_passes_complete_preflight() {
    let bytes = include_bytes!("../../../fixtures/providers/components/lowerer.component.wasm");
    let resolved = resolved_component(bytes, bytes.to_vec());
    let host = ProviderComponentHost::new().expect("host configures");

    host.prepare(&resolved)
        .expect("digest-bound capability-denied typed lowerer prepares");
}

#[test]
fn any_callable_component_import_is_denied_before_instantiation() {
    let mut bytes = wat::parse_str(
        r#"(component
            (type $ambient (func))
            (import "ambient" (func (type $ambient)))
        )"#,
    )
    .expect("importing component parses");
    append_contract_section(&mut bytes, TARGET_PROVIDER_LOWERER_CONTRACT.as_bytes());
    let resolved = resolved_component(&bytes, bytes.clone());
    let host = ProviderComponentHost::new().expect("host configures");

    let failure = host
        .prepare(&resolved)
        .expect_err("any import is denied by the empty host contract");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentContractMismatch
    );
}

#[test]
fn concrete_ambient_capability_imports_are_denied_during_preflight() {
    for capability in [
        "wasi:filesystem/types@0.2.0",
        "wasi:sockets/tcp@0.2.0",
        "wasi:cli/environment@0.2.0",
        "wasi:clocks/wall-clock@0.2.0",
        "wasi:random/random@0.2.0",
    ] {
        let mut bytes = wat::parse_str(format!(
            r#"(component
                (type $capability
                    (instance
                        (type $operation (func))
                        (export "operation" (func (type $operation)))
                    )
                )
                (import "{capability}" (instance (type $capability)))
            )"#
        ))
        .expect("named capability import parses");
        append_contract_section(&mut bytes, TARGET_PROVIDER_LOWERER_CONTRACT.as_bytes());
        let host = ProviderComponentHost::new().expect("host configures");

        let failure = host
            .prepare(&resolved_component(&bytes, bytes.clone()))
            .expect_err("ambient capability import must reject");
        assert_eq!(
            failure.kind(),
            ProviderHostFailureKind::ComponentContractMismatch,
            "{capability}: {}",
            failure.diagnostic()
        );
        assert_eq!(
            failure.phase(),
            ProviderHostPhase::Preflight,
            "{capability}"
        );
        assert_eq!(
            failure.diagnostic(),
            "provider component imports callable or unknown host authority",
            "{capability}"
        );
    }
}
