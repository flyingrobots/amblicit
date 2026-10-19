use std::fmt::Write as _;
use std::process::Command;
use std::sync::Arc;
use std::thread;

use edict_provider_host_wasmtime::{
    provider_lowering_input_bytes, PreparedProviderComponent, ProviderComponentHost,
    ProviderHostFailureKind, ProviderHostLimits, ProviderReplayObservation,
    ResolvedProviderComponent,
};
use edict_provider_schema::{ProviderArtifactSchemaRegistry, ResolvedProviderSchemaArtifact};
use edict_syntax::{
    bind_target_provider_manifest, encode_canonical_cbor, select_provider_component,
    validate_provider_lowering_request, validate_provider_verification_request, CanonicalValue,
    ProviderArtifact, ProviderArtifactBinding, ProviderArtifactKind, ProviderArtifactRef,
    ProviderArtifactSchemaValidationErrorKind, ProviderArtifactSchemaValidator,
    ProviderArtifactSource, ProviderBoundArtifact, ProviderDigest, ProviderDigestAlgorithm,
    ProviderInvocationKind, ProviderInvocationValidationFailureKind,
    ProviderLoweringInvocationContract, ProviderLoweringOutputKind, ProviderLoweringOutputRequest,
    ProviderLoweringRequest, ProviderResourceRef, ProviderResponseLimits, ProviderSchemaBinding,
    ProviderSchemaFormat, ProviderVerificationInvocationContract, ProviderVerificationOutputKind,
    ProviderVerificationOutputRequest, ProviderVerificationRequest, ResourceRef,
    TargetProviderManifest, ValidatedProviderLoweringRequest, ValidatedProviderVerificationRequest,
    CORE_MODULE_DIGEST_DOMAIN, TARGET_IR_ARTIFACT_DIGEST_DOMAIN, TARGET_PROFILE_API_VERSION,
    TARGET_PROVIDER_ABI, TARGET_PROVIDER_MANIFEST_API_VERSION, TARGET_PROVIDER_PROTOCOL_VERSION,
};
use sha2::{Digest, Sha256};

const SCHEMA: &[u8] = br#"
artifact = null / {
  kind: "targetIrArtifact",
  domain: tstr,
  intents: { * tstr => any },
  targetProfile: { * tstr => any },
  sourceCoreCoordinate: tstr,
}
"#;
const NULL_BYTES: &[u8] = &[0xf6];
const LOWERER_BYTES: &[u8] =
    include_bytes!("../../../fixtures/providers/components/lowerer.component.wasm");
const VERIFIER_BYTES: &[u8] =
    include_bytes!("../../../fixtures/providers/components/verifier.component.wasm");
const MALFORMED_LOWERER_BYTES: &[u8] =
    include_bytes!("../../../fixtures/providers/components/malformed-lowerer.component.wasm");
const INSTANTIATION_FAILURE_LOWERER_BYTES: &[u8] = include_bytes!(
    "../../../fixtures/providers/components/instantiation-failure-lowerer.component.wasm"
);
const INSTANTIATION_FUEL_LOWERER_BYTES: &[u8] = include_bytes!(
    "../../../fixtures/providers/components/instantiation-fuel-lowerer.component.wasm"
);
const REVIEWED_TARGET_IR_BYTES: &[u8] =
    include_bytes!("../../../fixtures/target-ir/canonical/echo-effectful.target-ir.cbor");
const REVIEWED_TARGET_IR_DIGEST: &str =
    include_str!("../../../fixtures/target-ir/canonical/echo-effectful.target-ir.sha256");
const OUTPUT_DOMAIN: &str = "runtime.output/v1";
const REPLAY_OBSERVATION_MARKER: &str = "EDICT_PROVIDER_REPLAY_TARGET_IR=";

struct LowerHarness {
    host: ProviderComponentHost,
    prepared: PreparedProviderComponent<'static>,
    request: ValidatedProviderLoweringRequest<'static>,
    schema: &'static ProviderArtifactSchemaRegistry,
}

struct PreparedLowerInvocation {
    prepared: PreparedProviderComponent<'static>,
    request: ValidatedProviderLoweringRequest<'static>,
    schema: &'static ProviderArtifactSchemaRegistry,
}

struct VerifyHarness {
    host: ProviderComponentHost,
    prepared: PreparedProviderComponent<'static>,
    request: ValidatedProviderVerificationRequest<'static>,
    schema: &'static ProviderArtifactSchemaRegistry,
}

#[repr(C)]
#[derive(Debug)]
struct PermissiveRegistryWrapper {
    registry: ProviderArtifactSchemaRegistry,
}

impl ProviderArtifactSchemaValidator for PermissiveRegistryWrapper {
    fn supports_domain(&self, _domain: &str) -> bool {
        true
    }

    fn validate_canonical_value(
        &self,
        _domain: &str,
        _value: &CanonicalValue,
    ) -> Result<(), ProviderArtifactSchemaValidationErrorKind> {
        Ok(())
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn manifest_resource(coordinate: &str, bytes: &[u8]) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(sha256(bytes)),
    }
}

fn locked_resource(coordinate: &str, digit: char) -> ResourceRef {
    ResourceRef {
        coordinate: coordinate.to_owned(),
        digest: Some(format!("sha256:{}", digit.to_string().repeat(64))),
    }
}

fn provider_manifest() -> &'static TargetProviderManifest {
    provider_manifest_with_lowerer(LOWERER_BYTES)
}

fn provider_manifest_with_lowerer(lowerer_bytes: &'static [u8]) -> &'static TargetProviderManifest {
    Box::leak(Box::new(TargetProviderManifest {
        api_version: TARGET_PROVIDER_MANIFEST_API_VERSION.to_owned(),
        provider_abi: TARGET_PROVIDER_ABI.to_owned(),
        provider: locked_resource("runtime.provider@1", '1'),
        artifacts: vec![
            ProviderArtifactRef {
                role: "lowerer.runtime".to_owned(),
                artifact_kind: ProviderArtifactKind::Lowerer,
                resource: manifest_resource("runtime.lowerer/component@1", lowerer_bytes),
                source: ProviderArtifactSource::Component {
                    component: manifest_resource("runtime.lowerer/component@1", lowerer_bytes),
                },
            },
            ProviderArtifactRef {
                role: "schema.runtime".to_owned(),
                artifact_kind: ProviderArtifactKind::ArtifactSchema,
                resource: manifest_resource("runtime.artifacts.cddl@1", SCHEMA),
                source: ProviderArtifactSource::Generated {
                    semantic_source: locked_resource("runtime.semantic-source@1", '2'),
                    generator: locked_resource("runtime.provider-generator@1", '3'),
                },
            },
            ProviderArtifactRef {
                role: "verifier.runtime".to_owned(),
                artifact_kind: ProviderArtifactKind::Verifier,
                resource: manifest_resource("runtime.verifier/component@1", VERIFIER_BYTES),
                source: ProviderArtifactSource::Component {
                    component: manifest_resource("runtime.verifier/component@1", VERIFIER_BYTES),
                },
            },
        ],
        schema_bindings: [
            CORE_MODULE_DIGEST_DOMAIN,
            TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
            TARGET_PROFILE_API_VERSION,
            OUTPUT_DOMAIN,
        ]
        .into_iter()
        .map(|domain| ProviderSchemaBinding {
            domain: domain.to_owned(),
            schema_role: "schema.runtime".to_owned(),
            format: ProviderSchemaFormat::SelfContainedCddlV1,
            root_rule: "artifact".to_owned(),
        })
        .collect(),
    }))
}

fn registry(manifest: &'static TargetProviderManifest) -> &'static ProviderArtifactSchemaRegistry {
    Box::leak(Box::new(registry_value(manifest)))
}

fn registry_value(manifest: &'static TargetProviderManifest) -> ProviderArtifactSchemaRegistry {
    let validated = bind_target_provider_manifest(manifest).expect("manifest validates");
    ProviderArtifactSchemaRegistry::from_manifest(
        &validated,
        [ResolvedProviderSchemaArtifact {
            role: "schema.runtime".to_owned(),
            bytes: Arc::from(SCHEMA),
        }],
        [
            CORE_MODULE_DIGEST_DOMAIN,
            TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
            TARGET_PROFILE_API_VERSION,
            OUTPUT_DOMAIN,
        ],
    )
    .expect("registry constructs")
}

fn provider_digest(domain: &str) -> ProviderDigest {
    let frame = CanonicalValue::Array(vec![
        CanonicalValue::Text("edict.digest/v1".to_owned()),
        CanonicalValue::Text(domain.to_owned()),
        CanonicalValue::Null,
    ]);
    let bytes = encode_canonical_cbor(&frame).expect("digest frame encodes");
    ProviderDigest {
        algorithm: ProviderDigestAlgorithm::Sha256,
        bytes: Sha256::digest(bytes).to_vec(),
    }
}

fn binding(coordinate: &str, domain: &str) -> ProviderArtifactBinding {
    ProviderArtifactBinding {
        reference: ProviderResourceRef {
            coordinate: coordinate.to_owned(),
            digest: provider_digest(domain),
        },
        domain: domain.to_owned(),
    }
}

fn bound(coordinate: &str, domain: &str) -> ProviderBoundArtifact {
    ProviderBoundArtifact {
        reference: ProviderResourceRef {
            coordinate: coordinate.to_owned(),
            digest: provider_digest(domain),
        },
        artifact: ProviderArtifact {
            domain: domain.to_owned(),
            bytes: NULL_BYTES.to_vec(),
        },
    }
}

fn response_limits() -> ProviderResponseLimits {
    ProviderResponseLimits {
        max_output_count: 4,
        max_diagnostic_count: 4,
        max_total_response_bytes: 1024 * 1024,
    }
}

fn host_limits() -> ProviderHostLimits {
    ProviderHostLimits {
        max_input_bytes: 1024 * 1024,
        max_output_bytes: 3 * 1024 * 1024,
        max_diagnostic_bytes: 3 * 1024 * 1024,
        max_wasm_memory_bytes: 16 * 1024 * 1024,
        max_table_elements: 10_000,
        max_instances: 100,
        max_memories: 8,
        max_tables: 8,
        max_wasm_fuel: 50_000_000,
        max_hostcall_bytes: 4 * 1024 * 1024,
        max_host_diagnostic_bytes: 512,
    }
}

fn lower_harness(role: &str) -> LowerHarness {
    lower_harness_with_component(role, LOWERER_BYTES)
}

fn lower_harness_with_component(role: &str, component: &'static [u8]) -> LowerHarness {
    let host = ProviderComponentHost::new().expect("host configures");
    let invocation = prepared_lowerer_invocation(&host, role, component);
    LowerHarness {
        host,
        prepared: invocation.prepared,
        request: invocation.request,
        schema: invocation.schema,
    }
}

fn prepared_lowerer_invocation(
    host: &ProviderComponentHost,
    role: &str,
    component: &'static [u8],
) -> PreparedLowerInvocation {
    let manifest = provider_manifest_with_lowerer(component);
    let manifest_proof = Box::leak(Box::new(
        bind_target_provider_manifest(manifest).expect("manifest validates"),
    ));
    let selected = select_provider_component(
        manifest_proof,
        "lowerer.runtime",
        ProviderInvocationKind::Lowering,
    )
    .expect("lowerer selects");
    let resolved = ResolvedProviderComponent::new(selected, Arc::from(component));
    let prepared = host.prepare(&resolved).expect("lowerer prepares");
    let schema = registry(manifest);
    let request = validated_lowering_request(
        schema,
        role,
        ProviderLoweringOutputKind::GeneratedArtifact,
        OUTPUT_DOMAIN,
    );
    PreparedLowerInvocation {
        prepared,
        request,
        schema,
    }
}

fn validated_lowering_request(
    schema: &'static ProviderArtifactSchemaRegistry,
    role: &str,
    kind: ProviderLoweringOutputKind,
    domain: &str,
) -> ValidatedProviderLoweringRequest<'static> {
    let contract = Box::leak(Box::new(ProviderLoweringInvocationContract {
        core: binding("core@1", CORE_MODULE_DIGEST_DOMAIN),
        target_profile: binding("profile@1", TARGET_PROFILE_API_VERSION),
        semantic_inputs: Vec::new(),
    }));
    let request = Box::leak(Box::new(ProviderLoweringRequest {
        protocol_version: TARGET_PROVIDER_PROTOCOL_VERSION,
        core: bound("core@1", CORE_MODULE_DIGEST_DOMAIN),
        target_profile: bound("profile@1", TARGET_PROFILE_API_VERSION),
        semantic_inputs: Vec::new(),
        requested_outputs: vec![ProviderLoweringOutputRequest {
            role: role.to_owned(),
            kind,
            domain: domain.to_owned(),
        }],
        limits: response_limits(),
    }));
    validate_provider_lowering_request(schema, contract, request)
        .expect("lowering request validates")
}

fn verify_harness() -> VerifyHarness {
    let manifest = provider_manifest();
    let manifest_proof = Box::leak(Box::new(
        bind_target_provider_manifest(manifest).expect("manifest validates"),
    ));
    let selected = select_provider_component(
        manifest_proof,
        "verifier.runtime",
        ProviderInvocationKind::Verification,
    )
    .expect("verifier selects");
    let resolved = ResolvedProviderComponent::new(selected, Arc::from(VERIFIER_BYTES));
    let host = ProviderComponentHost::new().expect("host configures");
    let prepared = host.prepare(&resolved).expect("verifier prepares");
    let schema = registry(manifest);
    let contract = Box::leak(Box::new(ProviderVerificationInvocationContract {
        core: binding("core@1", CORE_MODULE_DIGEST_DOMAIN),
        target_profile: binding("profile@1", TARGET_PROFILE_API_VERSION),
        target_ir: binding("target-ir@1", TARGET_IR_ARTIFACT_DIGEST_DOMAIN),
        semantic_inputs: Vec::new(),
    }));
    let request = Box::leak(Box::new(ProviderVerificationRequest {
        protocol_version: TARGET_PROVIDER_PROTOCOL_VERSION,
        core: bound("core@1", CORE_MODULE_DIGEST_DOMAIN),
        target_profile: bound("profile@1", TARGET_PROFILE_API_VERSION),
        target_ir: bound("target-ir@1", TARGET_IR_ARTIFACT_DIGEST_DOMAIN),
        semantic_inputs: Vec::new(),
        requested_outputs: vec![ProviderVerificationOutputRequest {
            role: "report.runtime".to_owned(),
            kind: ProviderVerificationOutputKind::VerifierReport,
            domain: OUTPUT_DOMAIN.to_owned(),
        }],
        limits: response_limits(),
    }));
    let request = validate_provider_verification_request(schema, contract, request)
        .expect("verification request validates");
    VerifyHarness {
        host,
        prepared,
        request,
        schema,
    }
}

#[test]
fn conforming_lowerer_and_verifier_results_cross_complete_admission() {
    let lower = lower_harness("output.runtime");
    let outcome = lower
        .host
        .invoke_lowerer(&lower.prepared, &lower.request, lower.schema, host_limits())
        .expect("valid lowerer result is admitted");
    assert_eq!(
        outcome
            .manifest()
            .expect("success manifest")
            .outputs()
            .len(),
        1
    );

    let verify = verify_harness();
    let outcome = verify
        .host
        .invoke_verifier(
            &verify.prepared,
            &verify.request,
            verify.schema,
            host_limits(),
        )
        .expect("valid verifier result is admitted");
    assert_eq!(
        outcome
            .manifest()
            .expect("success manifest")
            .outputs()
            .len(),
        1
    );
}

#[test]
fn typed_provider_refusal_remains_distinct_from_host_failure() {
    let harness = lower_harness("fixture.refusal");
    let outcome = harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            harness.schema,
            host_limits(),
        )
        .expect("typed refusal crosses transport");
    assert!(outcome.refusal().is_some());
    assert!(outcome.manifest().is_none());
}

#[test]
fn input_and_authorized_output_limits_reject_before_invocation() {
    let harness = lower_harness("output.runtime");
    let exact_input =
        provider_lowering_input_bytes(&harness.request).expect("input size is representable");
    let mut limits = host_limits();
    limits.max_input_bytes = exact_input - 1;
    let failure = harness
        .host
        .invoke_lowerer(&harness.prepared, &harness.request, harness.schema, limits)
        .expect_err("zero input limit rejects");
    assert_eq!(failure.kind(), ProviderHostFailureKind::InputLimitExceeded);

    let mut exact_limits = host_limits();
    exact_limits.max_input_bytes = exact_input;
    harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            harness.schema,
            exact_limits,
        )
        .expect("the exact logical input-byte boundary passes");

    let mut limits = host_limits();
    limits.max_output_bytes = harness.request.request().limits.max_total_response_bytes - 1;
    let failure = harness
        .host
        .invoke_lowerer(&harness.prepared, &harness.request, harness.schema, limits)
        .expect_err("host output ceiling rejects larger request authority");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResponseLimitExceeded
    );
}

#[test]
fn prepared_component_request_and_registry_must_share_one_authority() {
    let harness = lower_harness("output.runtime");
    let other_registry = registry(provider_manifest());
    let failure = harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            other_registry,
            host_limits(),
        )
        .expect_err("a different registry instance cannot satisfy the request proof");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::HostInvariantViolated
    );
}

#[test]
fn prepared_component_cannot_cross_the_host_engine_boundary() {
    let harness = lower_harness("output.runtime");
    let other_host = ProviderComponentHost::new().expect("second host configures");
    let failure = other_host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            harness.schema,
            host_limits(),
        )
        .expect_err("a prepared component cannot run under another host engine");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::HostInvariantViolated
    );
    assert_eq!(
        failure.phase(),
        edict_provider_host_wasmtime::ProviderHostPhase::Preflight
    );
}

#[test]
fn registry_address_alias_cannot_substitute_another_validator_type() {
    let harness = lower_harness("fixture.schema-invalid");
    let wrapper = Box::leak(Box::new(PermissiveRegistryWrapper {
        registry: registry_value(provider_manifest()),
    }));
    let wrapped_request = validate_provider_lowering_request(
        wrapper,
        harness.request.contract(),
        harness.request.request(),
    )
    .expect("permissive wrapper accepts the otherwise valid request");

    let failure = harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &wrapped_request,
            &wrapper.registry,
            host_limits(),
        )
        .expect_err("same-address wrapper cannot impersonate the concrete registry");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::HostInvariantViolated
    );
}

#[test]
fn fuel_resource_and_guest_traps_remain_distinct() {
    let looping = lower_harness("fixture.loop");
    let mut limits = host_limits();
    limits.max_wasm_fuel = 10_000;
    let failure = looping
        .host
        .invoke_lowerer(&looping.prepared, &looping.request, looping.schema, limits)
        .expect_err("loop must exhaust fuel");
    assert_eq!(failure.kind(), ProviderHostFailureKind::FuelExhausted);

    let memory = lower_harness("fixture.memory");
    let mut limits = host_limits();
    limits.max_wasm_memory_bytes = 4 * 1024 * 1024;
    let failure = memory
        .host
        .invoke_lowerer(&memory.prepared, &memory.request, memory.schema, limits)
        .expect_err("guest memory pressure must be denied");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResourceLimitExceeded
    );

    let instance_limited = lower_harness("output.runtime");
    let mut limits = host_limits();
    limits.max_instances = 0;
    let failure = instance_limited
        .host
        .invoke_lowerer(
            &instance_limited.prepared,
            &instance_limited.request,
            instance_limited.schema,
            limits,
        )
        .expect_err("store instance-count exhaustion must be a resource failure");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResourceLimitExceeded
    );

    let trapped = lower_harness("fixture.trap");
    let failure = trapped
        .host
        .invoke_lowerer(
            &trapped.prepared,
            &trapped.request,
            trapped.schema,
            host_limits(),
        )
        .expect_err("explicit guest trap rejects");
    assert_eq!(failure.kind(), ProviderHostFailureKind::GuestTrap);

    let trapped = lower_harness("fixture.trap");
    let mut limits = host_limits();
    limits.max_host_diagnostic_bytes = 0;
    let failure = trapped
        .host
        .invoke_lowerer(&trapped.prepared, &trapped.request, trapped.schema, limits)
        .expect_err("engine diagnostic retention must honor an exact zero bound");
    assert_eq!(failure.kind(), ProviderHostFailureKind::GuestTrap);
    assert_eq!(failure.diagnostic(), "");
}

#[test]
fn hostcall_and_logical_response_limits_are_separate() {
    let output_flood = lower_harness("fixture.output-flood");
    let mut limits = host_limits();
    limits.max_hostcall_bytes = 64 * 1024;
    let failure = output_flood
        .host
        .invoke_lowerer(
            &output_flood.prepared,
            &output_flood.request,
            output_flood.schema,
            limits,
        )
        .expect_err("guest-to-host lifting must be bounded");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResponseLiftLimitExceeded,
        "{}",
        failure.diagnostic()
    );

    let output_flood = lower_harness("fixture.output-flood");
    let mut limits = host_limits();
    limits.max_hostcall_bytes = 64 * 1024;
    limits.max_host_diagnostic_bytes = 0;
    let failure = output_flood
        .host
        .invoke_lowerer(
            &output_flood.prepared,
            &output_flood.request,
            output_flood.schema,
            limits,
        )
        .expect_err("diagnostic truncation cannot change lifting failure identity");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResponseLiftLimitExceeded
    );
    assert_eq!(failure.diagnostic(), "");

    let output_limit = lower_harness("fixture.output-flood");
    let failure = output_limit
        .host
        .invoke_lowerer(
            &output_limit.prepared,
            &output_limit.request,
            output_limit.schema,
            host_limits(),
        )
        .expect_err("lifted output must still pass the logical response bound");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResponseLimitExceeded
    );
    assert!(failure.validation_report().is_some());
}

#[test]
fn diagnostic_and_schema_envelope_limits_are_separate() {
    let diagnostic_flood = lower_harness("fixture.diagnostic-flood");
    let mut limits = host_limits();
    limits.max_diagnostic_bytes = 64;
    let failure = diagnostic_flood
        .host
        .invoke_lowerer(
            &diagnostic_flood.prepared,
            &diagnostic_flood.request,
            diagnostic_flood.schema,
            limits,
        )
        .expect_err("provider diagnostic bytes must be bounded");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::DiagnosticLimitExceeded
    );

    let bad_envelope = lower_harness("fixture.bad-envelope");
    let failure = bad_envelope
        .host
        .invoke_lowerer(
            &bad_envelope.prepared,
            &bad_envelope.request,
            bad_envelope.schema,
            host_limits(),
        )
        .expect_err("typed but unauthorized response rejects");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResponseEnvelopeInvalid
    );
    assert!(failure.validation_report().is_some());

    let schema_invalid = lower_harness("fixture.schema-invalid");
    let failure = schema_invalid
        .host
        .invoke_lowerer(
            &schema_invalid.prepared,
            &schema_invalid.request,
            schema_invalid.schema,
            host_limits(),
        )
        .expect_err("canonical guest artifact must satisfy its bound schema");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ResponseEnvelopeInvalid
    );
    assert!(failure
        .validation_report()
        .expect("schema rejection preserves the pure report")
        .failures
        .iter()
        .any(|failure| failure.kind
            == edict_syntax::ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch));
}

#[test]
fn provider_output_failure_matrix_preserves_stable_validation_kinds() {
    for (role, expected_kind) in [
        (
            "fixture.noncanonical",
            ProviderInvocationValidationFailureKind::NonCanonicalArtifact,
        ),
        (
            "fixture.wrong-domain",
            ProviderInvocationValidationFailureKind::ArtifactDomainMismatch,
        ),
        (
            "fixture.duplicate-role",
            ProviderInvocationValidationFailureKind::DuplicateRole,
        ),
        (
            "fixture.undeclared-output",
            ProviderInvocationValidationFailureKind::UndeclaredOutput,
        ),
        (
            "fixture.path-traversal",
            ProviderInvocationValidationFailureKind::InvalidLogicalPath,
        ),
    ] {
        let harness = lower_harness(role);
        let failure = harness
            .host
            .invoke_lowerer(
                &harness.prepared,
                &harness.request,
                harness.schema,
                host_limits(),
            )
            .expect_err("invalid provider output must not be admitted");
        assert_eq!(
            failure.kind(),
            ProviderHostFailureKind::ResponseEnvelopeInvalid,
            "{role}: {}",
            failure.diagnostic()
        );
        let kinds = failure
            .validation_report()
            .expect("response rejection retains its pure validation report")
            .failures
            .iter()
            .map(|failure| failure.kind)
            .collect::<Vec<_>>();
        assert!(kinds.contains(&expected_kind), "{role}: {kinds:?}");
    }

    let recovery = lower_harness("output.runtime");
    recovery
        .host
        .invoke_lowerer(
            &recovery.prepared,
            &recovery.request,
            recovery.schema,
            host_limits(),
        )
        .expect("failure matrix leaves the compiler host usable");
}

#[test]
fn malformed_canonical_abi_result_is_not_a_guest_trap_or_envelope_failure() {
    let harness = lower_harness_with_component("output.runtime", MALFORMED_LOWERER_BYTES);
    let failure = harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            harness.schema,
            host_limits(),
        )
        .expect_err("invalid result discriminant must fail during lifting");
    assert_eq!(failure.kind(), ProviderHostFailureKind::MalformedResponse);
    assert!(failure.validation_report().is_none());
}

#[test]
fn component_instantiation_failure_is_stable() {
    let harness =
        lower_harness_with_component("output.runtime", INSTANTIATION_FAILURE_LOWERER_BYTES);
    let failure = harness
        .host
        .invoke_lowerer(
            &harness.prepared,
            &harness.request,
            harness.schema,
            host_limits(),
        )
        .expect_err("out-of-bounds active data must fail during instantiation");
    assert_eq!(
        failure.kind(),
        ProviderHostFailureKind::ComponentInstantiationFailed
    );
}

#[test]
fn instantiation_fuel_exhaustion_preserves_budget_identity() {
    let harness = lower_harness_with_component("output.runtime", INSTANTIATION_FUEL_LOWERER_BYTES);
    let mut limits = host_limits();
    limits.max_wasm_fuel = 10_000;
    limits.max_host_diagnostic_bytes = 0;
    let failure = harness
        .host
        .invoke_lowerer(&harness.prepared, &harness.request, harness.schema, limits)
        .expect_err("start work must exhaust deterministic fuel during instantiation");
    assert_eq!(failure.kind(), ProviderHostFailureKind::FuelExhausted);
    assert_eq!(
        failure.phase(),
        edict_provider_host_wasmtime::ProviderHostPhase::Instantiate
    );
    assert_eq!(failure.diagnostic(), "");
}

#[test]
fn replay_proves_equal_completed_and_rejected_observations() {
    for role in ["output.runtime", "fixture.refusal"] {
        let harness = lower_harness(role);
        let replay = harness
            .host
            .replay_lowerer(
                &harness.prepared,
                &harness.request,
                harness.schema,
                host_limits(),
            )
            .expect("deterministic completed invocation replays");
        assert!(matches!(
            replay.observation(),
            ProviderReplayObservation::Completed(_)
        ));
    }

    for (role, expected_kind) in [
        ("fixture.trap", ProviderHostFailureKind::GuestTrap),
        ("fixture.loop", ProviderHostFailureKind::FuelExhausted),
        (
            "fixture.schema-invalid",
            ProviderHostFailureKind::ResponseEnvelopeInvalid,
        ),
    ] {
        let harness = lower_harness(role);
        let replay = harness
            .host
            .replay_lowerer(
                &harness.prepared,
                &harness.request,
                harness.schema,
                host_limits(),
            )
            .expect("deterministic rejected invocation replays");
        let ProviderReplayObservation::Rejected(failure) = replay.observation() else {
            panic!("host failure must remain a rejected replay observation");
        };
        assert_eq!(failure.kind(), expected_kind);
    }

    let malformed = lower_harness_with_component("output.runtime", MALFORMED_LOWERER_BYTES);
    let replay = malformed
        .host
        .replay_lowerer(
            &malformed.prepared,
            &malformed.request,
            malformed.schema,
            host_limits(),
        )
        .expect("malformed lifting failure replays");
    let ProviderReplayObservation::Rejected(failure) = replay.observation() else {
        panic!("malformed lifting must remain a rejected replay observation");
    };
    assert_eq!(failure.kind(), ProviderHostFailureKind::MalformedResponse);

    let harness = verify_harness();
    let replay = harness
        .host
        .replay_verifier(
            &harness.prepared,
            &harness.request,
            harness.schema,
            host_limits(),
        )
        .expect("deterministic verifier invocation replays");
    assert!(matches!(
        replay.observation(),
        ProviderReplayObservation::Completed(_)
    ));
}

fn reviewed_target_ir_replay_observation() -> String {
    let harness = lower_harness("fixture.target-ir");
    let request = validated_lowering_request(
        harness.schema,
        "fixture.target-ir",
        ProviderLoweringOutputKind::TargetIr,
        TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
    );
    let replay = harness
        .host
        .replay_lowerer(&harness.prepared, &request, harness.schema, host_limits())
        .expect("reviewed Target IR invocation replays");
    let ProviderReplayObservation::Completed(outcome) = replay.observation() else {
        panic!("reviewed Target IR fixture must be a completed observation");
    };
    let response = outcome.response().expect("fixture returns Target IR");
    assert_eq!(response.outputs.len(), 1);
    assert_eq!(response.outputs[0].artifact.bytes, REVIEWED_TARGET_IR_BYTES);
    let manifest = outcome.manifest().expect("fixture output is admitted");
    assert_eq!(manifest.outputs().len(), 1);
    let mut domain_digest = String::with_capacity("sha256:".len() + 64);
    domain_digest.push_str("sha256:");
    for byte in &manifest.outputs()[0].digest.bytes {
        write!(&mut domain_digest, "{byte:02x}").expect("writing a digest to String cannot fail");
    }
    assert_eq!(domain_digest, REVIEWED_TARGET_IR_DIGEST.trim());
    format!("{}:{domain_digest}", sha256(REVIEWED_TARGET_IR_BYTES))
}

#[test]
fn generic_lowerer_matches_reviewed_target_ir_bytes_and_digest() {
    let observation = reviewed_target_ir_replay_observation();
    assert!(observation.ends_with(REVIEWED_TARGET_IR_DIGEST.trim()));
}

#[test]
#[ignore = "child entrypoint exercised by the independent-process replay test"]
fn emit_reviewed_target_ir_replay_observation() {
    println!(
        "{REPLAY_OBSERVATION_MARKER}{}",
        reviewed_target_ir_replay_observation()
    );
}

#[test]
fn independent_processes_reproduce_reviewed_target_ir_observation() {
    let executable = std::env::current_exe().expect("current test executable is discoverable");
    let run_child = || {
        let output = Command::new(&executable)
            .arg("emit_reviewed_target_ir_replay_observation")
            .args(["--exact", "--ignored", "--nocapture", "--test-threads=1"])
            .output()
            .expect("child replay process launches");
        assert!(
            output.status.success(),
            "child replay failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("child replay output is UTF-8");
        stdout
            .lines()
            .find_map(|line| {
                line.split_once(REPLAY_OBSERVATION_MARKER)
                    .map(|(_, observation)| observation)
            })
            .unwrap_or_else(|| panic!("child replay omitted its stable observation:\n{stdout}"))
            .to_owned()
    };

    let first = run_child();
    let second = run_child();
    assert_eq!(first, second);
    assert!(first.ends_with(REVIEWED_TARGET_IR_DIGEST.trim()));
}

#[test]
fn failed_invocations_cannot_poison_a_later_fresh_store() {
    for (role, expected_kind) in [
        ("fixture.trap", ProviderHostFailureKind::GuestTrap),
        ("fixture.loop", ProviderHostFailureKind::FuelExhausted),
        (
            "fixture.memory",
            ProviderHostFailureKind::ResourceLimitExceeded,
        ),
        (
            "fixture.schema-invalid",
            ProviderHostFailureKind::ResponseEnvelopeInvalid,
        ),
        (
            "fixture.bad-envelope",
            ProviderHostFailureKind::ResponseEnvelopeInvalid,
        ),
    ] {
        let harness = lower_harness(role);
        let failure = harness
            .host
            .invoke_lowerer(
                &harness.prepared,
                &harness.request,
                harness.schema,
                host_limits(),
            )
            .expect_err("poisoning fixture mode must reject");
        assert_eq!(failure.kind(), expected_kind);

        let recovery = validated_lowering_request(
            harness.schema,
            "output.runtime",
            ProviderLoweringOutputKind::GeneratedArtifact,
            OUTPUT_DOMAIN,
        );
        let recovered = harness
            .host
            .invoke_lowerer(&harness.prepared, &recovery, harness.schema, host_limits())
            .expect("later invocation receives independent fresh state");
        assert_eq!(
            recovered.response().expect("successful recovery").outputs[0]
                .artifact
                .bytes,
            NULL_BYTES
        );
    }
}

#[test]
fn concurrent_invocations_share_no_guest_store_state() {
    let harness = lower_harness("output.runtime");
    thread::scope(|scope| {
        let calls = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    harness.host.invoke_lowerer(
                        &harness.prepared,
                        &harness.request,
                        harness.schema,
                        host_limits(),
                    )
                })
            })
            .collect::<Vec<_>>();
        let mut outcomes = calls.into_iter().map(|call| {
            call.join()
                .expect("provider invocation thread does not panic")
                .expect("concurrent provider invocation succeeds")
        });
        let first = outcomes.next().expect("at least one invocation");
        assert!(outcomes.all(|outcome| outcome == first));
    });
}

#[test]
fn failed_provider_does_not_poison_another_prepared_provider() {
    let host = ProviderComponentHost::new().expect("host configures");
    let malformed = prepared_lowerer_invocation(&host, "output.runtime", MALFORMED_LOWERER_BYTES);
    let failure = host
        .invoke_lowerer(
            &malformed.prepared,
            &malformed.request,
            malformed.schema,
            host_limits(),
        )
        .expect_err("malformed provider result rejects");
    assert_eq!(failure.kind(), ProviderHostFailureKind::MalformedResponse);

    let conforming = prepared_lowerer_invocation(&host, "output.runtime", LOWERER_BYTES);
    host.invoke_lowerer(
        &conforming.prepared,
        &conforming.request,
        conforming.schema,
        host_limits(),
    )
    .expect("independent provider remains usable on the same engine");
}

#[test]
fn repeated_preparation_preserves_invocation_observation() {
    let host = ProviderComponentHost::new().expect("host configures");
    let first = prepared_lowerer_invocation(&host, "output.runtime", LOWERER_BYTES);
    let second = prepared_lowerer_invocation(&host, "output.runtime", LOWERER_BYTES);

    let first = host
        .invoke_lowerer(&first.prepared, &first.request, first.schema, host_limits())
        .expect("first preparation invokes");
    let second = host
        .invoke_lowerer(
            &second.prepared,
            &second.request,
            second.schema,
            host_limits(),
        )
        .expect("second preparation invokes");
    assert_eq!(first, second);
}
