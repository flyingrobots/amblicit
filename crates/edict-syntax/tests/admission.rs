//! Gate C admission checks for typed v1 admission-boundary artifacts.
//!
//! These tests assert public behavior: validation status and stable failure
//! kinds. They do not inspect diagnostic prose, serialized bytes, documentation,
//! or repository layout.

use edict_syntax::{
    check_gate_c_invocation, digest_admission_request, validate_admission_receipt,
    validate_admission_request, AdmissionDecision, AdmissionEvidenceRef, AdmissionRequest,
    AdmissionValidationFailureKind, AdmissionValidationReport, AdmissionValidationStatus,
    AuthoringProvenance, BundleSubject, BundleSubjectKind, CapabilityReceipt,
    CapabilityReceiptKind, ContractBundleManifest, ExecutionInputKind, ExecutionInputRef,
    GateCInvocation, OperationRequirementRef, ResourceRef, SourceArtifactRef,
    ADMISSION_RECEIPT_API_VERSION, ADMISSION_REQUEST_API_VERSION, CONTRACT_BUNDLE_API_VERSION,
};

fn digest(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
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

fn echo_bundle() -> ContractBundleManifest {
    ContractBundleManifest {
        api_version: CONTRACT_BUNDLE_API_VERSION.to_owned(),
        semantic_bundle_digest: digest('a'),
        release_bundle_digest: digest('b'),
        source_artifacts: vec![SourceArtifactRef {
            logical_path: "contracts/hello.edict".to_owned(),
            artifact: digest_locked("source.contracts.hello", 'c'),
        }],
        source_profile_semantic_facts: digest_locked("source-profile.hello/v1", 'd'),
        core_ir: digest_locked("edict.core/v1", 'e'),
        target_profile: digest_locked("echo.dpo@1", 'f'),
        target_ir: digest_locked("echo.span-ir/v1", '1'),
        lawpacks: vec![digest_locked("hello.optics@1", '2')],
        generated_artifacts: vec![digest_locked("echo.dpo.registration/v1", '3')],
        compiler: digest_locked("edict.compiler/v1", '4'),
        lowerer: digest_locked("echo.dpo.lowerer/v1", '5'),
        verifier: digest_locked("echo.dpo.verifier/v1", '6'),
        semantic_compile_options: digest_locked("edict.compile-options.semantic/v1", '7'),
        non_semantic_compile_options: digest_locked("edict.compile-options.nonsemantic/v1", '8'),
        build_provenance: digest_locked("edict.build-provenance/v1", '9'),
        canonicalization_profile: digest_locked("edict.canonical-cbor/v1", 'a'),
        conformance_fixture_corpora: vec![digest_locked("echo.dpo.fixtures/v1", 'b')],
        verifier_report: digest_locked("echo.dpo.verifier-report/v1", 'c'),
        compile_explanation: digest_locked("watson.compile-explanation/v1", 'd'),
        assurance_evidence: Vec::new(),
        admission_artifacts: Vec::new(),
    }
}

fn canonical_input(coordinate: &str, hex: char) -> ExecutionInputRef {
    ExecutionInputRef {
        kind: ExecutionInputKind::CanonicalInput,
        artifact: digest_locked(coordinate, hex),
    }
}

fn operation_with_coordinate(
    bundle_subject: BundleSubject,
    operation_coordinate: &str,
) -> OperationRequirementRef {
    OperationRequirementRef {
        bundle_subject,
        operation_coordinate: operation_coordinate.to_owned(),
        basis: digest_locked("hello.sayHello.basis/v1", 'e'),
        variables_digest: digest('f'),
        requirements_digest: digest('1'),
        execution_inputs: vec![canonical_input("hello.sayHello.input/v1", '2')],
    }
}

fn operation(bundle_subject: BundleSubject) -> OperationRequirementRef {
    operation_with_coordinate(bundle_subject, "hello.sayHello")
}

fn request_for(bundle: &ContractBundleManifest, kind: BundleSubjectKind) -> AdmissionRequest {
    let bundle_subject = subject(
        kind,
        match kind {
            BundleSubjectKind::Semantic => &bundle.semantic_bundle_digest,
            BundleSubjectKind::Release => &bundle.release_bundle_digest,
        },
    );
    AdmissionRequest {
        api_version: ADMISSION_REQUEST_API_VERSION.to_owned(),
        bundle_subject: bundle_subject.clone(),
        participant_descriptor: digest_locked("continuum.participant.echo-lab/v1", '3'),
        catalog_snapshot: digest_locked("continuum.catalog.snapshot/v1", '4'),
        admission_policy: digest_locked("continuum.policy.admission/v1", '5'),
        policy_epoch: "epoch-2026-06-25T02:00Z".to_owned(),
        requested_operations: vec![operation(bundle_subject)],
        requested_capabilities: vec![digest_locked("continuum.capability.invoke/v1", '6')],
        requested_runtime_budget: digest_locked("continuum.runtime-budget/v1", '7'),
        admission_evidence: vec![AdmissionEvidenceRef {
            artifact: digest_locked("holmes.lawfulness-certificate/v1", '8'),
        }],
    }
}

fn accepted_receipt_for(request: &AdmissionRequest) -> edict_syntax::AdmissionReceiptBody {
    edict_syntax::AdmissionReceiptBody {
        api_version: ADMISSION_RECEIPT_API_VERSION.to_owned(),
        admission_request_digest: digest_admission_request(request),
        bundle_subject: request.bundle_subject.clone(),
        participant: digest_locked("continuum.participant.echo-lab/v1", '3'),
        decision: AdmissionDecision::Accepted,
        admitted_operations: vec!["hello.sayHello".to_owned()],
        admitted_capabilities: vec![digest_locked("continuum.capability.invoke/v1", '6')],
        admitted_runtime_budget: digest_locked("continuum.runtime-budget/v1", '7'),
        policy_epoch: request.policy_epoch.clone(),
        rejection: None,
        signing_envelope: None,
    }
}

fn invocation_capability(request: &AdmissionRequest) -> CapabilityReceipt {
    CapabilityReceipt {
        kind: CapabilityReceiptKind::Invocation,
        issuer_bundle_subject: request.bundle_subject.clone(),
        participant: digest_locked("continuum.participant.echo-lab/v1", '3'),
        operation_coordinate: "hello.sayHello".to_owned(),
        scope: request.requested_capabilities[0].clone(),
        policy_epoch: request.policy_epoch.clone(),
    }
}

fn invocation_packet(
    bundle: ContractBundleManifest,
    request: AdmissionRequest,
    receipt: Option<edict_syntax::AdmissionReceiptBody>,
    capability_receipts: Vec<CapabilityReceipt>,
) -> GateCInvocation {
    GateCInvocation {
        bundle,
        request,
        operation_coordinate: "hello.sayHello".to_owned(),
        receipt,
        capability_receipts,
        authoring_provenance: AuthoringProvenance::Agent,
    }
}

fn failure_kinds(report: &AdmissionValidationReport) -> Vec<AdmissionValidationFailureKind> {
    report.failures.iter().map(|failure| failure.kind).collect()
}

#[test]
fn semantic_and_release_subjects_are_checked_independently() {
    let bundle = echo_bundle();
    let semantic_request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut stale_release_request = request_for(&bundle, BundleSubjectKind::Release);
    stale_release_request.bundle_subject.digest = digest('f');
    stale_release_request.requested_operations[0]
        .bundle_subject
        .digest = digest('f');

    let semantic_report = validate_admission_request(&bundle, &semantic_request);
    let release_report = validate_admission_request(&bundle, &stale_release_request);

    assert_eq!(semantic_report.status, AdmissionValidationStatus::Valid);
    assert_eq!(
        semantic_report.failures,
        [] as [edict_syntax::AdmissionValidationFailure; 0]
    );
    assert_eq!(release_report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&release_report),
        vec![AdmissionValidationFailureKind::BundleSubjectMismatch]
    );
}

#[test]
fn operation_requirements_bind_subject_basis_and_canonical_variables() {
    let bundle = echo_bundle();
    let mut request = request_for(&bundle, BundleSubjectKind::Semantic);
    request.requested_operations[0].bundle_subject.digest = digest('f');

    let report = validate_admission_request(&bundle, &request);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::OperationRequirementMismatch]
    );
}

#[test]
fn hidden_host_inputs_are_rejected_below_the_determinism_boundary() {
    let bundle = echo_bundle();
    let mut request = request_for(&bundle, BundleSubjectKind::Semantic);
    request.requested_operations[0]
        .execution_inputs
        .push(ExecutionInputRef {
            kind: ExecutionInputKind::HiddenHostInput,
            artifact: digest_locked("llm.prompt-context/v1", 'f'),
        });

    let report = validate_admission_request(&bundle, &request);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::HiddenExecutionInput]
    );
}

#[test]
fn admission_receipt_must_echo_request_subject() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.bundle_subject.digest = digest('f');

    let report = validate_admission_receipt(&request, &receipt);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::AdmissionReceiptMismatch]
    );
}

#[test]
fn admission_receipt_must_echo_request_digest() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.admission_request_digest = digest('f');

    let report = validate_admission_receipt(&request, &receipt);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::AdmissionReceiptMismatch]
    );
}

#[test]
fn admission_receipt_must_echo_policy_epoch() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.policy_epoch = "epoch-2026-06-25T03:00Z".to_owned();

    let report = validate_admission_receipt(&request, &receipt);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::AdmissionReceiptMismatch]
    );
}

#[test]
fn receipt_body_must_not_reference_its_signing_envelope() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.signing_envelope = Some(digest_locked("dsse.envelope/v1", 'f'));

    let report = validate_admission_receipt(&request, &receipt);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::ReceiptSignatureCycle]
    );
}

#[test]
fn accepted_receipt_cannot_carry_rejection_evidence() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.rejection = Some(digest_locked("continuum.admission-rejection/v1", 'f'));

    let report = validate_admission_receipt(&request, &receipt);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::AdmissionReceiptMismatch]
    );
}

#[test]
fn receipt_admitted_operations_must_be_requested() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.admitted_operations = vec!["hello.wave".to_owned()];

    let report = validate_admission_receipt(&request, &receipt);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::AdmissionReceiptMismatch]
    );
}

#[test]
fn receipt_admitted_capabilities_must_be_requested() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.admitted_capabilities = vec![digest_locked("continuum.capability.unrequested/v1", '5')];

    let report = validate_admission_receipt(&request, &receipt);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::AdmissionReceiptMismatch]
    );
}

#[test]
fn llm_authored_artifact_still_requires_admission_receipt() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut packet = invocation_packet(
        bundle,
        request,
        None,
        vec![CapabilityReceipt {
            kind: CapabilityReceiptKind::Invocation,
            issuer_bundle_subject: subject(BundleSubjectKind::Semantic, &digest('a')),
            participant: digest_locked("continuum.participant.echo-lab/v1", '3'),
            operation_coordinate: "hello.sayHello".to_owned(),
            scope: digest_locked("continuum.scope.echo-lab/v1", '4'),
            policy_epoch: "epoch-2026-06-25T02:00Z".to_owned(),
        }],
    );
    packet.authoring_provenance = AuthoringProvenance::LlmAgent;

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::MissingAdmissionReceipt]
    );
}

#[test]
fn registration_receipt_does_not_authorize_invocation() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let receipt = accepted_receipt_for(&request);
    let registration_receipt = CapabilityReceipt {
        kind: CapabilityReceiptKind::Registration,
        ..invocation_capability(&request)
    };
    let mut packet = invocation_packet(bundle, request, Some(receipt), vec![registration_receipt]);
    packet.authoring_provenance = AuthoringProvenance::Human;

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::RegistrationReceiptIsNotInvocationAuthority]
    );
}

#[test]
fn participant_policy_rejection_is_evidence_not_invocation_authority() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let mut receipt = accepted_receipt_for(&request);
    receipt.decision = AdmissionDecision::Rejected;
    receipt.rejection = Some(digest_locked("continuum.admission-rejection/v1", 'f'));
    let receipt_report = validate_admission_receipt(&request, &receipt);
    let packet = invocation_packet(
        bundle,
        request.clone(),
        Some(receipt),
        vec![invocation_capability(&request)],
    );

    let invocation_report = check_gate_c_invocation(&packet);

    assert_eq!(receipt_report.status, AdmissionValidationStatus::Valid);
    assert_eq!(
        receipt_report.failures,
        [] as [edict_syntax::AdmissionValidationFailure; 0]
    );
    assert_eq!(invocation_report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&invocation_report),
        vec![AdmissionValidationFailureKind::MissingAcceptedAdmissionReceipt]
    );
}

#[test]
fn invoked_operation_must_be_in_requested_operations() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let receipt = accepted_receipt_for(&request);
    let mut capability = invocation_capability(&request);
    capability.operation_coordinate = "hello.wave".to_owned();
    let mut packet = invocation_packet(bundle, request, Some(receipt), vec![capability]);
    packet.operation_coordinate = "hello.wave".to_owned();

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::OperationRequirementMismatch]
    );
}

#[test]
fn accepted_receipt_must_admit_invoked_operation() {
    let bundle = echo_bundle();
    let mut request = request_for(&bundle, BundleSubjectKind::Semantic);
    request.requested_operations.push(operation_with_coordinate(
        request.bundle_subject.clone(),
        "hello.wave",
    ));
    let receipt = accepted_receipt_for(&request);
    let mut capability = invocation_capability(&request);
    capability.operation_coordinate = "hello.wave".to_owned();
    let packet = invocation_packet(bundle, request, Some(receipt), vec![capability]);
    let mut packet = packet;
    packet.operation_coordinate = "hello.wave".to_owned();

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::MissingAcceptedAdmissionReceipt]
    );
}

#[test]
fn invocation_capability_must_match_receipt_participant() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let receipt = accepted_receipt_for(&request);
    let mut capability = invocation_capability(&request);
    capability.participant = digest_locked("continuum.participant.other-lab/v1", '5');
    let packet = invocation_packet(bundle, request, Some(receipt), vec![capability]);

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::MissingInvocationCapability]
    );
}

#[test]
fn invocation_capability_must_match_policy_epoch() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let receipt = accepted_receipt_for(&request);
    let mut capability = invocation_capability(&request);
    capability.policy_epoch = "epoch-2026-06-25T03:00Z".to_owned();
    let packet = invocation_packet(bundle, request, Some(receipt), vec![capability]);

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::MissingInvocationCapability]
    );
}

#[test]
fn invocation_capability_scope_must_be_admitted() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let receipt = accepted_receipt_for(&request);
    let mut capability = invocation_capability(&request);
    capability.scope = digest_locked("continuum.capability.unadmitted/v1", '5');
    let packet = invocation_packet(bundle, request, Some(receipt), vec![capability]);

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Invalid);
    assert_eq!(
        failure_kinds(&report),
        vec![AdmissionValidationFailureKind::MissingInvocationCapability]
    );
}

#[test]
fn accepted_receipt_and_invocation_capability_authorize_gate_c_invocation() {
    let bundle = echo_bundle();
    let request = request_for(&bundle, BundleSubjectKind::Semantic);
    let receipt = accepted_receipt_for(&request);
    let capability = invocation_capability(&request);
    let packet = invocation_packet(bundle, request, Some(receipt), vec![capability]);

    let report = check_gate_c_invocation(&packet);

    assert_eq!(report.status, AdmissionValidationStatus::Valid);
    assert_eq!(
        report.failures,
        [] as [edict_syntax::AdmissionValidationFailure; 0]
    );
}
