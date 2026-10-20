//! Pure provider invocation envelope validation.
//!
//! These tests exercise WIT-shaped in-memory values. They do not instantiate a
//! component, touch the filesystem, or interpret provider-owned semantics.

use std::sync::Mutex;

use edict_syntax::{
    encode_canonical_cbor, validate_provider_lowering_limit_independence,
    validate_provider_lowering_request as validate_lowering_request_with_schemas,
    validate_provider_lowering_result, validate_provider_verification_limit_independence,
    validate_provider_verification_result, CanonicalValue, ProviderArtifact,
    ProviderArtifactBinding, ProviderArtifactSchemaValidationErrorKind,
    ProviderArtifactSchemaValidator, ProviderBoundArtifact, ProviderDiagnostic,
    ProviderDiagnosticSeverity, ProviderDigest, ProviderDigestAlgorithm, ProviderInvocationKind,
    ProviderInvocationValidationFailureKind, ProviderLoweringInvocationContract,
    ProviderLoweringOutputArtifact, ProviderLoweringOutputKind, ProviderLoweringOutputRequest,
    ProviderLoweringRequest, ProviderLoweringResult, ProviderLoweringSuccess,
    ProviderProtocolVersion, ProviderRefusal, ProviderRefusalKind, ProviderResourceRef,
    ProviderResponseLimits, ProviderSemanticInput, ProviderSemanticInputBinding,
    ProviderSemanticInputKind, ProviderVerificationInvocationContract,
    ProviderVerificationOutputArtifact, ProviderVerificationOutputKind,
    ProviderVerificationOutputRequest, ProviderVerificationRequest, ProviderVerificationResult,
    ProviderVerificationSuccess, AUTHORITY_FACTS_API_VERSION, CORE_DIGEST_FRAME,
    CORE_MODULE_DIGEST_DOMAIN, MAX_CANONICAL_NESTING_DEPTH, TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
    TARGET_PROFILE_API_VERSION,
};
use sha2::{Digest, Sha256};

const LAWPACK_DOMAIN: &str = "edict.lawpack/v1";
const GENERATED_DOMAIN: &str = "echo.generated-artifact/v1";
const REVIEW_DOMAIN: &str = "echo.review-payload/v1";
const REPORT_DOMAIN: &str = "echo.verifier-report/v1";
const CORE_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/core/canonical/bounded-hello.core.cbor");
const TARGET_IR_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/target-ir/canonical/echo-effectful.target-ir.cbor");
const ALTERNATE_TARGET_IR_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/target-ir/canonical/gitwarp-append.target-ir.cbor");

#[derive(Debug)]
struct TestArtifactSchemas;

impl ProviderArtifactSchemaValidator for TestArtifactSchemas {
    fn supports_domain(&self, domain: &str) -> bool {
        matches!(
            domain,
            CORE_MODULE_DIGEST_DOMAIN
                | TARGET_PROFILE_API_VERSION
                | AUTHORITY_FACTS_API_VERSION
                | LAWPACK_DOMAIN
                | "echo.review-context/v1"
                | "echo.lowerability-facts/v1"
                | TARGET_IR_ARTIFACT_DIGEST_DOMAIN
                | GENERATED_DOMAIN
                | REVIEW_DOMAIN
                | REPORT_DOMAIN
        )
    }

    fn validate_canonical_value(
        &self,
        domain: &str,
        value: &CanonicalValue,
    ) -> Result<(), ProviderArtifactSchemaValidationErrorKind> {
        if !self.supports_domain(domain) {
            return Err(ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain);
        }
        let CanonicalValue::Map(entries) = value else {
            return Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch);
        };
        if domain == TARGET_IR_ARTIFACT_DIGEST_DOMAIN {
            let target_ir_shape_is_valid = map_text_field(entries, "kind")
                == Some("targetIrArtifact")
                && matches!(map_value(entries, "domain"), Some(CanonicalValue::Text(_)))
                && matches!(
                    map_value(entries, "targetProfile"),
                    Some(CanonicalValue::Map(_))
                )
                && matches!(
                    map_value(entries, "sourceCoreCoordinate"),
                    Some(CanonicalValue::Text(_))
                )
                && matches!(map_value(entries, "intents"), Some(CanonicalValue::Map(_)));
            if !target_ir_shape_is_valid {
                return Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch);
            }
        }
        Ok(())
    }
}

static ARTIFACT_SCHEMAS: TestArtifactSchemas = TestArtifactSchemas;

#[derive(Debug, Default)]
struct RecordingArtifactSchemas {
    calls: Mutex<Vec<String>>,
}

impl ProviderArtifactSchemaValidator for RecordingArtifactSchemas {
    fn supports_domain(&self, domain: &str) -> bool {
        self.calls
            .lock()
            .expect("schema recorder lock remains available")
            .push(format!("supports:{domain}"));
        ARTIFACT_SCHEMAS.supports_domain(domain)
    }

    fn validate_canonical_value(
        &self,
        domain: &str,
        value: &CanonicalValue,
    ) -> Result<(), ProviderArtifactSchemaValidationErrorKind> {
        self.calls
            .lock()
            .expect("schema recorder lock remains available")
            .push(format!("validate:{domain}"));
        ARTIFACT_SCHEMAS.validate_canonical_value(domain, value)
    }
}

fn map_text_field<'a>(
    entries: &'a [(CanonicalValue, CanonicalValue)],
    field: &str,
) -> Option<&'a str> {
    entries.iter().find_map(|(key, value)| match (key, value) {
        (CanonicalValue::Text(key), CanonicalValue::Text(value)) if key == field => {
            Some(value.as_str())
        }
        _ => None,
    })
}

fn map_value<'a>(
    entries: &'a [(CanonicalValue, CanonicalValue)],
    field: &str,
) -> Option<&'a CanonicalValue> {
    entries.iter().find_map(|(key, value)| match key {
        CanonicalValue::Text(key) if key == field => Some(value),
        _ => None,
    })
}

fn validate_provider_lowering_request<'a>(
    contract: &'a ProviderLoweringInvocationContract,
    request: &'a ProviderLoweringRequest,
) -> Result<
    edict_syntax::ValidatedProviderLoweringRequest<'a>,
    edict_syntax::ProviderInvocationValidationReport,
> {
    validate_lowering_request_with_schemas(&ARTIFACT_SCHEMAS, contract, request)
}

fn validate_provider_verification_request<'a>(
    contract: &'a ProviderVerificationInvocationContract,
    request: &'a ProviderVerificationRequest,
) -> Result<
    edict_syntax::ValidatedProviderVerificationRequest<'a>,
    edict_syntax::ProviderInvocationValidationReport,
> {
    edict_syntax::validate_provider_verification_request(&ARTIFACT_SCHEMAS, contract, request)
}

fn canonical_bytes(label: &str) -> Vec<u8> {
    encode_canonical_cbor(&CanonicalValue::Map(vec![
        (
            CanonicalValue::Text("kind".to_owned()),
            CanonicalValue::Text(label.to_owned()),
        ),
        (
            CanonicalValue::Text("version".to_owned()),
            CanonicalValue::Integer(1),
        ),
    ]))
    .expect("test value is canonical")
}

fn digest(domain: &str, bytes: &[u8]) -> ProviderDigest {
    let value = edict_syntax::decode_canonical_cbor(bytes).expect("test bytes decode");
    let frame = CanonicalValue::Array(vec![
        CanonicalValue::Text(CORE_DIGEST_FRAME.to_owned()),
        CanonicalValue::Text(domain.to_owned()),
        value,
    ]);
    let preimage = encode_canonical_cbor(&frame).expect("digest frame encodes");
    ProviderDigest {
        algorithm: ProviderDigestAlgorithm::Sha256,
        bytes: Sha256::digest(preimage).to_vec(),
    }
}

fn bound(coordinate: &str, domain: &str, label: &str) -> ProviderBoundArtifact {
    bound_bytes(coordinate, domain, canonical_bytes(label))
}

fn bound_bytes(coordinate: &str, domain: &str, bytes: impl Into<Vec<u8>>) -> ProviderBoundArtifact {
    let bytes = bytes.into();
    ProviderBoundArtifact {
        reference: ProviderResourceRef {
            coordinate: coordinate.to_owned(),
            digest: digest(domain, &bytes),
        },
        artifact: ProviderArtifact {
            domain: domain.to_owned(),
            bytes,
        },
    }
}

fn binding(artifact: &ProviderBoundArtifact) -> ProviderArtifactBinding {
    ProviderArtifactBinding {
        reference: artifact.reference.clone(),
        domain: artifact.artifact.domain.clone(),
    }
}

fn limits() -> ProviderResponseLimits {
    ProviderResponseLimits {
        max_output_count: 8,
        max_diagnostic_count: 8,
        max_total_response_bytes: 64 * 1024,
    }
}

fn lowering_fixture() -> (ProviderLoweringInvocationContract, ProviderLoweringRequest) {
    let core = bound_bytes("core.example@1", CORE_MODULE_DIGEST_DOMAIN, CORE_FIXTURE);
    let target_profile = bound("echo.dpo@1", TARGET_PROFILE_API_VERSION, "targetProfile");
    let authority = bound(
        "echo.authority@1",
        AUTHORITY_FACTS_API_VERSION,
        "authorityFacts",
    );
    let lawpack = bound("echo.lawpack@1", LAWPACK_DOMAIN, "lawpack");
    let auxiliary = bound(
        "echo.review-context@1",
        "echo.review-context/v1",
        "auxiliary",
    );
    let lowerability = bound(
        "echo.lowerability@1",
        "echo.lowerability-facts/v1",
        "lowerabilityFacts",
    );
    let semantic_inputs = vec![
        ProviderSemanticInput {
            role: "authority".to_owned(),
            kind: ProviderSemanticInputKind::AuthorityFacts,
            artifact: authority.clone(),
        },
        ProviderSemanticInput {
            role: "auxiliary".to_owned(),
            kind: ProviderSemanticInputKind::Auxiliary("review-context".to_owned()),
            artifact: auxiliary,
        },
        ProviderSemanticInput {
            role: "lawpack".to_owned(),
            kind: ProviderSemanticInputKind::Lawpack,
            artifact: lawpack.clone(),
        },
        ProviderSemanticInput {
            role: "lowerability".to_owned(),
            kind: ProviderSemanticInputKind::LowerabilityFacts,
            artifact: lowerability,
        },
    ];
    let contract = ProviderLoweringInvocationContract {
        core: binding(&core),
        target_profile: binding(&target_profile),
        semantic_inputs: semantic_inputs
            .iter()
            .map(|input| ProviderSemanticInputBinding {
                role: input.role.clone(),
                kind: input.kind.clone(),
                artifact: binding(&input.artifact),
            })
            .collect(),
    };
    let request = ProviderLoweringRequest {
        protocol_version: ProviderProtocolVersion::V1_0_0,
        core,
        target_profile,
        semantic_inputs,
        requested_outputs: vec![
            ProviderLoweringOutputRequest {
                role: "artifact".to_owned(),
                kind: ProviderLoweringOutputKind::GeneratedArtifact,
                domain: GENERATED_DOMAIN.to_owned(),
            },
            ProviderLoweringOutputRequest {
                role: "target-ir".to_owned(),
                kind: ProviderLoweringOutputKind::TargetIr,
                domain: TARGET_IR_ARTIFACT_DIGEST_DOMAIN.to_owned(),
            },
        ],
        limits: limits(),
    };
    (contract, request)
}

fn verification_fixture() -> (
    ProviderVerificationInvocationContract,
    ProviderVerificationRequest,
) {
    let (lowering_contract, lowering_request) = lowering_fixture();
    let target_ir = bound_bytes(
        "echo.target-ir@1",
        TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
        TARGET_IR_FIXTURE,
    );
    (
        ProviderVerificationInvocationContract {
            core: lowering_contract.core,
            target_profile: lowering_contract.target_profile,
            target_ir: binding(&target_ir),
            semantic_inputs: lowering_contract.semantic_inputs,
        },
        ProviderVerificationRequest {
            protocol_version: ProviderProtocolVersion::V1_0_0,
            core: lowering_request.core,
            target_profile: lowering_request.target_profile,
            target_ir,
            semantic_inputs: lowering_request.semantic_inputs,
            requested_outputs: vec![ProviderVerificationOutputRequest {
                role: "report".to_owned(),
                kind: ProviderVerificationOutputKind::VerifierReport,
                domain: REPORT_DOMAIN.to_owned(),
            }],
            limits: limits(),
        },
    )
}

#[allow(clippy::unnecessary_wraps)] // The fixture deliberately returns the WIT result arm.
fn lowering_success(request: &ProviderLoweringRequest) -> ProviderLoweringResult {
    Ok(ProviderLoweringSuccess {
        outputs: request
            .requested_outputs
            .iter()
            .map(|output| ProviderLoweringOutputArtifact {
                role: output.role.clone(),
                kind: output.kind,
                artifact: ProviderArtifact {
                    domain: output.domain.clone(),
                    bytes: if output.kind == ProviderLoweringOutputKind::TargetIr {
                        TARGET_IR_FIXTURE.to_vec()
                    } else {
                        canonical_bytes(&output.role)
                    },
                },
                logical_path: Some(format!("generated/{}.cbor", output.role)),
            })
            .collect(),
        diagnostics: vec![],
    })
}

#[allow(clippy::unnecessary_wraps)] // The fixture deliberately returns the WIT result arm.
fn verification_success(request: &ProviderVerificationRequest) -> ProviderVerificationResult {
    Ok(ProviderVerificationSuccess {
        outputs: vec![ProviderVerificationOutputArtifact {
            role: request.requested_outputs[0].role.clone(),
            kind: ProviderVerificationOutputKind::VerifierReport,
            artifact: ProviderArtifact {
                domain: request.requested_outputs[0].domain.clone(),
                bytes: canonical_bytes("verifierReport"),
            },
            logical_path: Some("reports/verification.cbor".to_owned()),
        }],
        diagnostics: vec![],
    })
}

fn kinds(
    report: &edict_syntax::ProviderInvocationValidationReport,
) -> Vec<ProviderInvocationValidationFailureKind> {
    report.failures.iter().map(|failure| failure.kind).collect()
}

#[test]
fn lowering_request_accepts_digest_bound_canonical_inputs() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request)
        .expect("valid lowering request should be trusted");

    assert_eq!(validated.request(), &request);
    assert_eq!(validated.contract(), &contract);
}

#[test]
fn verification_request_binds_canonical_target_ir() {
    let (contract, request) = verification_fixture();
    let validated = validate_provider_verification_request(&contract, &request)
        .expect("valid verification request should be trusted");

    assert_eq!(validated.request(), &request);
    assert_eq!(validated.contract(), &contract);
}

#[test]
fn requests_reject_unsupported_protocol_version() {
    let (contract, mut request) = lowering_fixture();
    request.protocol_version = ProviderProtocolVersion {
        major: 1,
        minor: 1,
        patch: 0,
    };

    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("unsupported protocol should reject");
    assert_eq!(
        kinds(&report),
        vec![ProviderInvocationValidationFailureKind::UnsupportedProtocolVersion]
    );
}

#[test]
fn requests_reject_malformed_and_mismatched_input_digests() {
    let (contract, mut request) = lowering_fixture();
    request.core.reference.digest.bytes.pop();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("short digest should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::MalformedDigest));

    let (contract, mut request) = lowering_fixture();
    request.target_profile.artifact.bytes = canonical_bytes("substitutedTargetProfile");
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("bytes not bound by the reference should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactDigestMismatch)
    );
}

#[test]
fn requests_reject_malformed_or_unbound_contract_fields() {
    let (contract, mut request) = lowering_fixture();
    request.core.reference.coordinate.clear();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("empty resource coordinate should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::EmptyResourceCoordinate)
    );

    let (contract, mut request) = lowering_fixture();
    request.core.reference.coordinate = "core.substituted@1".to_owned();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("resource reference substitution should reject");
    assert!(kinds(&report)
        .contains(&ProviderInvocationValidationFailureKind::ArtifactReferenceMismatch));

    let (contract, mut request) = lowering_fixture();
    request.semantic_inputs[0].kind = ProviderSemanticInputKind::LowerabilityFacts;
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("semantic kind substitution should reject");
    assert!(kinds(&report)
        .contains(&ProviderInvocationValidationFailureKind::SemanticInputBindingMismatch));

    let (contract, mut request) = lowering_fixture();
    request.requested_outputs[0].domain.clear();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("empty requested artifact domain should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::MissingArtifactDomain)
    );
}

#[test]
fn every_semantic_input_kind_and_target_ir_recomputes_its_digest() {
    let (contract, request) = lowering_fixture();
    for index in 0..request.semantic_inputs.len() {
        let mut mutated = request.clone();
        mutated.semantic_inputs[index].artifact.artifact.bytes =
            canonical_bytes("substitutedSemanticInput");
        let report = validate_provider_lowering_request(&contract, &mutated)
            .expect_err("every semantic input kind must reproduce its bound digest");
        assert!(
            kinds(&report)
                .contains(&ProviderInvocationValidationFailureKind::ArtifactDigestMismatch),
            "semantic input index {index} was not digest-bound"
        );
    }

    let (contract, mut request) = verification_fixture();
    request.target_ir.artifact.bytes = ALTERNATE_TARGET_IR_FIXTURE.to_vec();
    let report = validate_provider_verification_request(&contract, &request)
        .expect_err("Target IR must reproduce its bound digest");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactDigestMismatch)
    );
}

#[test]
fn every_input_class_must_match_its_owning_schema() {
    let null = encode_canonical_cbor(&CanonicalValue::Null).expect("null encodes canonically");
    let (contract, request) = lowering_fixture();

    let mut core = request.clone();
    core.core.artifact.bytes = null.clone();
    let report = validate_provider_lowering_request(&contract, &core)
        .expect_err("Core schema mismatch should reject");
    assert_eq!(
        kinds(&report),
        vec![ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch],
        "schema rejection must precede digest identity comparison"
    );

    let mut profile = request.clone();
    profile.target_profile.artifact.bytes = null.clone();
    let report = validate_provider_lowering_request(&contract, &profile)
        .expect_err("target-profile schema mismatch should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch)
    );

    for index in 0..request.semantic_inputs.len() {
        let mut semantic = request.clone();
        semantic.semantic_inputs[index].artifact.artifact.bytes = null.clone();
        let report = validate_provider_lowering_request(&contract, &semantic)
            .expect_err("semantic input schema mismatch should reject");
        assert!(
            kinds(&report)
                .contains(&ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch),
            "semantic input index {index} bypassed its schema"
        );
    }

    let (contract, mut request) = verification_fixture();
    request.target_ir.artifact.bytes = null;
    let report = validate_provider_verification_request(&contract, &request)
        .expect_err("Target IR schema mismatch should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch)
    );
}

#[test]
fn requests_reject_wrong_fixed_and_semantic_input_domains() {
    let (mut contract, request) = lowering_fixture();
    contract.core.domain = "wrong.core/v1".to_owned();
    let report =
        validate_provider_lowering_request(&contract, &request).expect_err("Core domain is fixed");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactDomainMismatch)
    );

    let (mut contract, mut request) = lowering_fixture();
    contract.semantic_inputs[0].artifact.domain = "wrong.authority/v1".to_owned();
    request.semantic_inputs[0].artifact.artifact.domain = "wrong.authority/v1".to_owned();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("known semantic kind has a fixed domain");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactDomainMismatch)
    );
}

#[test]
fn semantic_input_closure_and_role_order_are_exact() {
    let (contract, mut request) = lowering_fixture();
    request.semantic_inputs.remove(0);
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("missing semantic input should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::MissingSemanticInput));

    let (contract, mut request) = lowering_fixture();
    let mut extra = request.semantic_inputs[0].clone();
    extra.role = "review".to_owned();
    request.semantic_inputs.push(extra);
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("undeclared semantic input should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::UndeclaredSemanticInput)
    );

    let (contract, mut request) = lowering_fixture();
    request.semantic_inputs[0].role.clear();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("empty role should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::EmptyRole));

    let (contract, mut request) = lowering_fixture();
    request.semantic_inputs[1].role = request.semantic_inputs[0].role.clone();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("duplicate role should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::DuplicateRole));

    let (contract, mut request) = lowering_fixture();
    request.semantic_inputs.swap(0, 1);
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("out-of-order role should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::OutOfOrderRole));

    let (contract, mut request) = lowering_fixture();
    request.requested_outputs[0].role.clear();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("empty requested-output role should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::EmptyRole));

    let (contract, mut request) = lowering_fixture();
    request.requested_outputs[1].role = request.requested_outputs[0].role.clone();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("duplicate requested-output role should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::DuplicateRole));

    let (contract, mut request) = lowering_fixture();
    request.requested_outputs.swap(0, 1);
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("out-of-order requested-output role should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::OutOfOrderRole));

    let (mut contract, mut request) = lowering_fixture();
    contract.semantic_inputs.truncate(2);
    request.semantic_inputs.truncate(2);
    contract.semantic_inputs[0].role = "z".to_owned();
    request.semantic_inputs[0].role = "z".to_owned();
    contract.semantic_inputs[1].role = "é".to_owned();
    request.semantic_inputs[1].role = "é".to_owned();
    validate_provider_lowering_request(&contract, &request)
        .expect("UTF-8 bytes order ASCII z before multibyte é");

    let (mut contract, mut request) = lowering_fixture();
    contract.semantic_inputs.truncate(2);
    request.semantic_inputs.truncate(2);
    contract.semantic_inputs[0].role = "e\u{301}".to_owned();
    request.semantic_inputs[0].role = "e\u{301}".to_owned();
    contract.semantic_inputs[1].role = "é".to_owned();
    request.semantic_inputs[1].role = "é".to_owned();
    validate_provider_lowering_request(&contract, &request)
        .expect("normalization-distinct UTF-8 role bytes remain distinct");
}

#[test]
fn success_requires_exact_requested_role_kind_and_domain_set() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs.remove(0);
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("missing output should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::MissingRequestedOutput)
    );

    let mut result = lowering_success(&request);
    let mut extra = result.as_ref().unwrap().outputs[0].clone();
    extra.role = "zz-extra".to_owned();
    result.as_mut().unwrap().outputs.push(extra);
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("undeclared output should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::UndeclaredOutput));

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs[0].kind = ProviderLoweringOutputKind::ReviewPayload;
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("kind substitution should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::OutputKindMismatch));

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs[0].artifact.domain = "wrong.output/v1".to_owned();
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("domain substitution should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactDomainMismatch)
    );
}

#[test]
fn returned_output_roles_require_nonempty_unique_utf8_order() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs[0].role.clear();
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("empty returned-output role should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::EmptyRole));

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs.swap(0, 1);
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("output order should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::OutOfOrderRole));

    let mut result = lowering_success(&request);
    let duplicate = result.as_ref().unwrap().outputs[0].role.clone();
    result.as_mut().unwrap().outputs[1].role = duplicate;
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("duplicate output should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::DuplicateRole));
}

#[test]
fn logical_paths_are_package_relative_and_collision_free() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    for invalid in ["", "/root", "a\\b", "a:b", "a//b", "a/./b", "a/../b"] {
        let mut result = lowering_success(&request);
        result.as_mut().unwrap().outputs[0].logical_path = Some(invalid.to_owned());
        let report = validate_provider_lowering_result(&validated, &result)
            .expect_err("invalid logical path should reject");
        assert!(
            kinds(&report).contains(&ProviderInvocationValidationFailureKind::InvalidLogicalPath)
        );
    }

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs[0].logical_path = Some("same.cbor".to_owned());
    result.as_mut().unwrap().outputs[1].logical_path = Some("same.cbor".to_owned());
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("exact path collision should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::DuplicateLogicalPath));

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs[0].logical_path = Some("Case.cbor".to_owned());
    result.as_mut().unwrap().outputs[1].logical_path = Some("case.cbor".to_owned());
    validate_provider_lowering_result(&validated, &result)
        .expect("case-distinct logical paths remain distinct");

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs[0].logical_path = Some("é.cbor".to_owned());
    result.as_mut().unwrap().outputs[1].logical_path = Some("e\u{301}.cbor".to_owned());
    validate_provider_lowering_result(&validated, &result)
        .expect("canonically equivalent Unicode path bytes are not normalized");
}

#[test]
fn diagnostics_require_wit_tuple_order_without_duplicates() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let first = ProviderDiagnostic {
        code: "A".to_owned(),
        severity: ProviderDiagnosticSeverity::Error,
        message: "first".to_owned(),
        repair: None,
    };
    let second = ProviderDiagnostic {
        code: "B".to_owned(),
        severity: ProviderDiagnosticSeverity::Info,
        message: "second".to_owned(),
        repair: Some("repair".to_owned()),
    };

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().diagnostics = vec![second.clone(), first.clone()];
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("diagnostic order should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::OutOfOrderDiagnostic));

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().diagnostics = vec![first.clone(), first.clone()];
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("duplicate diagnostic should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::DuplicateDiagnostic));

    let mut result = lowering_success(&request);
    result.as_mut().unwrap().diagnostics = vec![first.clone(), second, first];
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("non-adjacent duplicate diagnostic should reject as duplicate");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::DuplicateDiagnostic));
}

#[test]
fn diagnostic_order_uses_every_wit_tuple_key() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let diagnostic = |severity, message: &str, repair: Option<&str>| ProviderDiagnostic {
        code: "SAME".to_owned(),
        severity,
        message: message.to_owned(),
        repair: repair.map(str::to_owned),
    };
    let ordered = vec![
        diagnostic(ProviderDiagnosticSeverity::Error, "a", None),
        diagnostic(ProviderDiagnosticSeverity::Error, "b", None),
        diagnostic(ProviderDiagnosticSeverity::Error, "b", Some("a")),
        diagnostic(ProviderDiagnosticSeverity::Error, "b", Some("b")),
        diagnostic(ProviderDiagnosticSeverity::Warning, "a", None),
        diagnostic(ProviderDiagnosticSeverity::Info, "a", None),
    ];
    let mut result = lowering_success(&request);
    result.as_mut().unwrap().diagnostics = ordered.clone();
    validate_provider_lowering_result(&validated, &result)
        .expect("all WIT diagnostic tuple keys in ascending order should validate");

    for pair in [0usize, 1, 2, 3, 4] {
        let mut result = lowering_success(&request);
        let mut diagnostics = ordered.clone();
        diagnostics.swap(pair, pair + 1);
        result.as_mut().unwrap().diagnostics = diagnostics;
        let report = validate_provider_lowering_result(&validated, &result)
            .expect_err("reversing any WIT tuple key should reject");
        assert!(
            kinds(&report).contains(&ProviderInvocationValidationFailureKind::OutOfOrderDiagnostic)
        );
    }
}

#[test]
fn response_limits_apply_exactly_to_success_and_refusal() {
    let (contract, mut request) = lowering_fixture();
    let result = lowering_success(&request);
    let success = result.as_ref().unwrap();
    let exact_bytes: u64 = success
        .outputs
        .iter()
        .map(|output| {
            output.role.len()
                + output.artifact.domain.len()
                + output.artifact.bytes.len()
                + output.logical_path.as_ref().map_or(0, String::len)
        })
        .sum::<usize>() as u64;
    request.limits = ProviderResponseLimits {
        max_output_count: u32::try_from(success.outputs.len()).expect("two fixture outputs"),
        max_diagnostic_count: 0,
        max_total_response_bytes: exact_bytes,
    };
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    validate_provider_lowering_result(&validated, &result).expect("exact limits should pass");

    request.limits.max_total_response_bytes -= 1;
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("success byte total over limit should reject");
    assert!(kinds(&report)
        .contains(&ProviderInvocationValidationFailureKind::ResponseByteLimitExceeded));

    request.limits.max_total_response_bytes += 1;
    request.limits.max_output_count -= 1;
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("output count over limit should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::OutputCountLimitExceeded)
    );

    let (contract, mut request) = lowering_fixture();
    let refusal = ProviderRefusal {
        kind: ProviderRefusalKind::UnsupportedSemantics,
        subject: Some("subject".to_owned()),
        diagnostics: vec![ProviderDiagnostic {
            code: "E".to_owned(),
            severity: ProviderDiagnosticSeverity::Error,
            message: "message".to_owned(),
            repair: Some("repair".to_owned()),
        }],
    };
    let refusal_bytes = ("subject".len() + "E".len() + "message".len() + "repair".len()) as u64;
    request.limits = ProviderResponseLimits {
        max_output_count: 0,
        max_diagnostic_count: 1,
        max_total_response_bytes: refusal_bytes,
    };
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    validate_provider_lowering_result(&validated, &Err(refusal.clone()))
        .expect("exact refusal limits should pass");

    request.limits.max_diagnostic_count = 0;
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let report = validate_provider_lowering_result(&validated, &Err(refusal.clone()))
        .expect_err("refusal diagnostic count over limit should reject");
    assert!(kinds(&report)
        .contains(&ProviderInvocationValidationFailureKind::DiagnosticCountLimitExceeded));

    request.limits.max_diagnostic_count = 1;
    request.limits.max_total_response_bytes -= 1;
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let report = validate_provider_lowering_result(&validated, &Err(refusal))
        .expect_err("refusal byte limit should reject");
    assert!(kinds(&report)
        .contains(&ProviderInvocationValidationFailureKind::ResponseByteLimitExceeded));
}

#[test]
fn zero_response_limits_allow_only_empty_counted_values() {
    let (contract, mut request) = lowering_fixture();
    request.requested_outputs.clear();
    request.limits = ProviderResponseLimits {
        max_output_count: 0,
        max_diagnostic_count: 0,
        max_total_response_bytes: 0,
    };
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    validate_provider_lowering_result(
        &validated,
        &Ok(ProviderLoweringSuccess {
            outputs: vec![],
            diagnostics: vec![],
        }),
    )
    .expect("empty success fits zero limits");

    let refusal = ProviderRefusal {
        kind: ProviderRefusalKind::UnsupportedSemantics,
        subject: Some("x".to_owned()),
        diagnostics: vec![],
    };
    let report = validate_provider_lowering_result(&validated, &Err(refusal))
        .expect_err("one refusal byte exceeds zero");
    assert!(kinds(&report)
        .contains(&ProviderInvocationValidationFailureKind::ResponseByteLimitExceeded));
}

#[test]
fn outputs_reject_noncanonical_bytes_before_identity() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let mut result = lowering_success(&request);
    result.as_mut().unwrap().outputs[0].artifact.bytes = vec![0x18, 0x00];

    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("noncanonical output should reject");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::NonCanonicalArtifact));

    let (contract, mut request) = lowering_fixture();
    request.core.artifact.bytes = vec![0x18, 0x00];
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("noncanonical input should reject before invocation");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::NonCanonicalArtifact));

    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let mut result = lowering_success(&request);
    let mut over_nested = vec![0x81; MAX_CANONICAL_NESTING_DEPTH + 1];
    over_nested.push(0xf6);
    result.as_mut().unwrap().outputs[0].artifact.bytes = over_nested;
    let report = validate_provider_lowering_result(&validated, &result)
        .expect_err("over-nested canonical artifact should reject safely");
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::NonCanonicalArtifact));
}

#[test]
fn owning_schema_validation_precedes_output_identity() {
    let (contract, mut request) = lowering_fixture();
    request.requested_outputs[0].domain = "unregistered.runtime-artifact/v1".to_owned();
    let report = validate_provider_lowering_request(&contract, &request)
        .expect_err("a request cannot authorize an output with no host schema");
    assert!(kinds(&report)
        .contains(&ProviderInvocationValidationFailureKind::UnsupportedArtifactDomain));

    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    for schema_invalid in [
        encode_canonical_cbor(&CanonicalValue::Null).expect("null encodes canonically"),
        canonical_bytes("wrongTargetIrShape"),
    ] {
        let mut result = lowering_success(&request);
        result.as_mut().unwrap().outputs[1].artifact.bytes = schema_invalid;
        let report = validate_provider_lowering_result(&validated, &result)
            .expect_err("canonical but schema-invalid Target IR must not receive identity");
        assert!(kinds(&report)
            .contains(&ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch));
    }
}

#[test]
fn runtime_owned_output_domains_require_their_registered_schemas() {
    let (contract, mut request) = lowering_fixture();
    request.requested_outputs.insert(
        1,
        ProviderLoweringOutputRequest {
            role: "review".to_owned(),
            kind: ProviderLoweringOutputKind::ReviewPayload,
            domain: REVIEW_DOMAIN.to_owned(),
        },
    );
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let valid = lowering_success(&request);
    validate_provider_lowering_result(&validated, &valid)
        .expect("registered generated, review, and Target IR schemas should validate");

    for output_index in [0usize, 1] {
        let mut invalid = valid.clone();
        invalid.as_mut().unwrap().outputs[output_index]
            .artifact
            .bytes =
            encode_canonical_cbor(&CanonicalValue::Null).expect("null encodes canonically");
        let report = validate_provider_lowering_result(&validated, &invalid)
            .expect_err("same-domain wrong output shape should reject");
        assert!(kinds(&report)
            .contains(&ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch));
    }

    let (contract, request) = verification_fixture();
    let validated = validate_provider_verification_request(&contract, &request).unwrap();
    let mut invalid = verification_success(&request);
    invalid.as_mut().unwrap().outputs[0].artifact.bytes =
        encode_canonical_cbor(&CanonicalValue::Null).expect("null encodes canonically");
    let report = validate_provider_verification_result(&validated, &invalid)
        .expect_err("same-domain wrong verifier report shape should reject");
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::ArtifactSchemaMismatch)
    );
}

#[test]
fn valid_success_produces_host_digested_manifest_and_refusal_is_preserved() {
    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let result = lowering_success(&request);
    let outcome = validate_provider_lowering_result(&validated, &result)
        .expect("valid success should produce a trusted outcome");
    let response = outcome.response().expect("success response is inspectable");
    let manifest = outcome.manifest().expect("success manifest is inspectable");
    assert_eq!(response, result.as_ref().unwrap());
    assert_eq!(manifest.invocation(), ProviderInvocationKind::Lowering);
    assert_eq!(manifest.protocol_version(), request.protocol_version);
    assert_eq!(manifest.inputs().core(), &contract.core);
    assert_eq!(manifest.inputs().target_profile(), &contract.target_profile);
    assert_eq!(manifest.inputs().target_ir(), None);
    assert_eq!(manifest.outputs().len(), request.requested_outputs.len());
    assert_eq!(
        manifest.requested_outputs().len(),
        request.requested_outputs.len()
    );
    for (binding, requested) in manifest
        .requested_outputs()
        .iter()
        .zip(&request.requested_outputs)
    {
        assert_eq!(binding.role, requested.role);
        assert_eq!(binding.kind, requested.kind);
        assert_eq!(binding.domain, requested.domain);
    }
    for (entry, output) in manifest.outputs().iter().zip(&response.outputs) {
        assert_eq!(entry.role, output.role);
        assert_eq!(entry.kind, output.kind);
        assert_eq!(entry.domain, output.artifact.domain);
        assert_eq!(entry.logical_path, output.logical_path);
        assert_eq!(entry.digest, digest(&entry.domain, &output.artifact.bytes));
    }
    assert_eq!(
        manifest.inputs().semantic_inputs(),
        contract.semantic_inputs
    );

    let refusal = ProviderRefusal {
        kind: ProviderRefusalKind::UnsupportedSemantics,
        subject: Some("echo.feature/v2".to_owned()),
        diagnostics: vec![],
    };
    let outcome = validate_provider_lowering_result(&validated, &Err(refusal.clone()))
        .expect("well-formed refusal is a valid provider outcome");
    assert_eq!(outcome.refusal(), Some(&refusal));
    assert!(outcome.response().is_none());
    assert!(outcome.manifest().is_none());
}

#[test]
fn verification_success_uses_verifier_only_output_vocabulary() {
    let (contract, request) = verification_fixture();
    let validated = validate_provider_verification_request(&contract, &request).unwrap();
    let result = verification_success(&request);
    let outcome = validate_provider_verification_result(&validated, &result)
        .expect("verification result should validate");
    let response = outcome.response().expect("success response is inspectable");
    let manifest = outcome.manifest().expect("success manifest is inspectable");
    assert_eq!(response, result.as_ref().unwrap());
    assert_eq!(manifest.invocation(), ProviderInvocationKind::Verification);
    assert_eq!(manifest.protocol_version(), request.protocol_version);
    assert_eq!(manifest.inputs().core(), &contract.core);
    assert_eq!(manifest.inputs().target_profile(), &contract.target_profile);
    assert_eq!(manifest.inputs().target_ir(), Some(&contract.target_ir));
    assert_eq!(
        manifest.inputs().semantic_inputs(),
        contract.semantic_inputs
    );
    assert_eq!(manifest.requested_outputs().len(), 1);
    assert_eq!(
        manifest.requested_outputs()[0].role,
        request.requested_outputs[0].role
    );
    assert_eq!(
        manifest.requested_outputs()[0].kind,
        request.requested_outputs[0].kind
    );
    assert_eq!(
        manifest.requested_outputs()[0].domain,
        request.requested_outputs[0].domain
    );
    assert_eq!(
        manifest.outputs()[0].kind,
        ProviderVerificationOutputKind::VerifierReport
    );
    assert_eq!(manifest.outputs()[0].role, response.outputs[0].role);
    assert_eq!(
        manifest.outputs()[0].domain,
        response.outputs[0].artifact.domain
    );
    assert_eq!(
        manifest.outputs()[0].logical_path,
        response.outputs[0].logical_path
    );
    assert_eq!(
        manifest.outputs()[0].digest,
        digest(
            &manifest.outputs()[0].domain,
            &response.outputs[0].artifact.bytes
        )
    );
}

#[test]
fn sufficient_limit_changes_cannot_change_provider_result() {
    let (contract, first_request) = lowering_fixture();
    let mut second_request = first_request.clone();
    second_request.limits.max_total_response_bytes *= 2;
    let first = validate_provider_lowering_request(&contract, &first_request).unwrap();
    let second = validate_provider_lowering_request(&contract, &second_request).unwrap();
    let baseline = lowering_success(&first_request);

    let report =
        validate_provider_lowering_limit_independence(&first, &baseline, &second, &baseline);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::ProviderInvocationValidationFailure; 0]
    );

    let mut substituted = baseline.clone();
    substituted.as_mut().unwrap().outputs[0].artifact.bytes = canonical_bytes("substitute");
    let report =
        validate_provider_lowering_limit_independence(&first, &baseline, &second, &substituted);
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::LimitDependentResult));

    let mut truncated = baseline.clone();
    truncated.as_mut().unwrap().outputs.pop();
    let report =
        validate_provider_lowering_limit_independence(&first, &baseline, &second, &truncated);
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::LimitDependentResult));

    let mut reordered = baseline.clone();
    reordered.as_mut().unwrap().outputs.swap(0, 1);
    let report =
        validate_provider_lowering_limit_independence(&first, &baseline, &second, &reordered);
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::LimitDependentResult));

    let mut diagnostic_substitution = baseline.clone();
    diagnostic_substitution.as_mut().unwrap().diagnostics = vec![ProviderDiagnostic {
        code: "I".to_owned(),
        severity: ProviderDiagnosticSeverity::Info,
        message: "changed".to_owned(),
        repair: None,
    }];
    let report = validate_provider_lowering_limit_independence(
        &first,
        &baseline,
        &second,
        &diagnostic_substitution,
    );
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::LimitDependentResult));

    let refusal = Err(ProviderRefusal {
        kind: ProviderRefusalKind::UnsupportedSemantics,
        subject: None,
        diagnostics: vec![],
    });
    let report =
        validate_provider_lowering_limit_independence(&first, &baseline, &second, &refusal);
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::LimitDependentResult));

    let mut noncomparable_request = second_request.clone();
    noncomparable_request.requested_outputs[0].domain = REPORT_DOMAIN.to_owned();
    let noncomparable =
        validate_provider_lowering_request(&contract, &noncomparable_request).unwrap();
    let report =
        validate_provider_lowering_limit_independence(&first, &baseline, &noncomparable, &baseline);
    assert!(
        kinds(&report).contains(&ProviderInvocationValidationFailureKind::NonComparableRequests)
    );

    let first_outcome = validate_provider_lowering_result(&first, &baseline).unwrap();
    let second_outcome = validate_provider_lowering_result(&second, &baseline).unwrap();
    let first_manifest = first_outcome
        .manifest()
        .expect("first success manifest is inspectable");
    let second_manifest = second_outcome
        .manifest()
        .expect("second success manifest is inspectable");
    assert_eq!(first_manifest, second_manifest);
}

#[test]
fn verifier_results_are_limit_independent_too() {
    let (contract, first_request) = verification_fixture();
    let mut second_request = first_request.clone();
    second_request.limits.max_total_response_bytes *= 2;
    let first = validate_provider_verification_request(&contract, &first_request).unwrap();
    let second = validate_provider_verification_request(&contract, &second_request).unwrap();
    let baseline = verification_success(&first_request);

    let report =
        validate_provider_verification_limit_independence(&first, &baseline, &second, &baseline);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::ProviderInvocationValidationFailure; 0]
    );

    let mut substituted = baseline.clone();
    substituted.as_mut().unwrap().outputs[0].artifact.bytes = canonical_bytes("changedReport");
    let report =
        validate_provider_verification_limit_independence(&first, &baseline, &second, &substituted);
    assert!(kinds(&report).contains(&ProviderInvocationValidationFailureKind::LimitDependentResult));
}

#[test]
fn repeated_validation_is_structurally_deterministic() {
    let (contract, mut request) = lowering_fixture();
    request.protocol_version.patch = 1;
    let first = validate_provider_lowering_request(&contract, &request)
        .expect_err("invalid request should produce a report");
    let second = validate_provider_lowering_request(&contract, &request)
        .expect_err("same invalid request should produce a report");
    assert_eq!(first, second);

    let (contract, request) = lowering_fixture();
    let validated = validate_provider_lowering_request(&contract, &request).unwrap();
    let result = lowering_success(&request);
    let first = validate_provider_lowering_result(&validated, &result).unwrap();
    let second = validate_provider_lowering_result(&validated, &result).unwrap();
    assert_eq!(first, second);
}

#[test]
fn owning_schema_validation_order_is_explicit_and_deterministic() {
    let (contract, request) = lowering_fixture();
    let result = lowering_success(&request);
    let first_schemas = RecordingArtifactSchemas::default();
    let first = validate_lowering_request_with_schemas(&first_schemas, &contract, &request)
        .expect("first request validates");
    validate_provider_lowering_result(&first, &result).expect("first result validates");
    let first_calls = first_schemas
        .calls
        .lock()
        .expect("schema recorder lock remains available")
        .clone();

    let second_schemas = RecordingArtifactSchemas::default();
    let second = validate_lowering_request_with_schemas(&second_schemas, &contract, &request)
        .expect("second request validates");
    validate_provider_lowering_result(&second, &result).expect("second result validates");
    let second_calls = second_schemas
        .calls
        .lock()
        .expect("schema recorder lock remains available")
        .clone();

    assert_eq!(first_calls, second_calls);
    for domain in [
        CORE_MODULE_DIGEST_DOMAIN,
        TARGET_PROFILE_API_VERSION,
        AUTHORITY_FACTS_API_VERSION,
        LAWPACK_DOMAIN,
        "echo.review-context/v1",
        "echo.lowerability-facts/v1",
        GENERATED_DOMAIN,
        TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
    ] {
        assert!(
            first_calls
                .iter()
                .any(|call| call == &format!("validate:{domain}")
                    || call == &format!("supports:{domain}")),
            "schema domain {domain} was not checked"
        );
    }
}
