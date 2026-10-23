# Admission Test Plan

Status: current verification design for typed Gate C admission-boundary checks.

## Scope

In scope:

- typed `AdmissionRequest`, `AdmissionReceiptBody`, `CapabilityReceipt`, and
  `GateCInvocation` values;
- `validate_admission_request`, `validate_admission_receipt`, and
  `check_gate_c_invocation` validation behavior;
- `digest_admission_request` request identity behavior;
- bundle-subject binding to semantic versus release bundle digests;
- operation requirement binding to bundle subject, operation coordinate, basis,
  canonical variables, and requirements digest;
- hidden execution input rejection below the determinism boundary;
- receipt echoing and receipt/signature acyclicity;
- receipt admitted-operation and admitted-capability subset checks and
  accepted/rejected evidence separation;
- registration evidence versus invocation authority;
- selected invoked operation membership in the request and accepted receipt;
- participant matching between admission receipt and invocation capability;
- authoring provenance being insufficient to bypass admission.

Out of scope:

- file-backed admission artifact loading;
- canonical-CBOR encode/decode helpers for admission artifacts;
- participant authentication;
- participant policy evaluation;
- capability delegation or revocation evaluation;
- target lowerer execution;
- admission explanation generation;
- distribution-envelope signature verification.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| ADMISSION-REQ-001 | implemented | Admission requests carry a `bundleSubject` that selects and must match either the semantic or release digest from the contract bundle manifest. | issue #6, issue #11, docs/SPEC_continuum-admission-v1.md |
| ADMISSION-REQ-002 | implemented | Operation requirements bind the request bundle subject, operation coordinate, basis artifact, canonical variables digest, and requirements digest. | issue #6, docs/SPEC_edict-language-v1.md |
| ADMISSION-REQ-003 | implemented | Hidden host inputs are rejected below the determinism boundary; execution inputs must be explicit canonical input, witnessed evidence, admitted basis, or capability presentation. | issue #6, docs/SPEC_edict-language-v1.md |
| ADMISSION-REQ-004 | implemented | Admission receipt bodies echo the request digest, bundle subject, and policy epoch; admitted operations and capabilities stay within requested operations and capabilities, accepted receipts do not carry rejection evidence, and receipt bodies do not reference their external signing envelope. | issue #11, docs/SPEC_continuum-admission-v1.md |
| ADMISSION-REQ-005 | implemented | Gate C invocation requires an accepted admission receipt and matching invocation capability receipt for the invoked operation, bundle subject, admitted capability scope, participant, and policy epoch; registration evidence and authoring provenance do not grant invocation authority. | issue #11, docs/SPEC_continuum-admission-v1.md |
| ADMISSION-REQ-006 | implemented | Edict validates artifact and operation semantics while participant policy, identity, revocation, delegation, and effective authority remain Continuum-owned. | issue #6 |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| crates/amblicit-syntax/tests/admission.rs | In-memory typed contract bundle, admission request, receipt, capability, and Gate C invocation values plus negative mutations. | The same checker accepts valid Edict-owned admission bindings and rejects only the mutated obligations with stable failure kinds. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ADMISSION-TP-001 | implemented | Boundary guard | ADMISSION-REQ-001 | A semantic-subject request validates while a release-subject request with a stale release digest returns `BundleSubjectMismatch`. | semantic_and_release_subjects_are_checked_independently | crates/amblicit-syntax/tests/admission.rs | Proves semantic and release subjects are selected independently. |
| ADMISSION-TP-002 | implemented | Boundary guard | ADMISSION-REQ-002 | Mutating an operation requirement bundle subject away from the request subject returns `OperationRequirementMismatch`. | operation_requirements_bind_subject_basis_and_canonical_variables | crates/amblicit-syntax/tests/admission.rs | The requirement binds the operation to the exact admitted subject. |
| ADMISSION-TP-003 | implemented | Boundary guard | ADMISSION-REQ-003 | Adding a hidden host execution input returns `HiddenExecutionInput`. | hidden_host_inputs_are_rejected_below_the_determinism_boundary | crates/amblicit-syntax/tests/admission.rs | Covers prompt context, DOM, filesystem, network, or other hidden host state by kind. |
| ADMISSION-TP-004 | implemented | Boundary guard | ADMISSION-REQ-004 | A receipt whose bundle subject differs from the request returns `AdmissionReceiptMismatch`. | admission_receipt_must_echo_request_subject | crates/amblicit-syntax/tests/admission.rs | Receipt echoing is checked as structured data. |
| ADMISSION-TP-005 | implemented | Boundary guard | ADMISSION-REQ-004 | A receipt whose request digest differs from the typed admission request digest returns `AdmissionReceiptMismatch`. | admission_receipt_must_echo_request_digest | crates/amblicit-syntax/tests/admission.rs | Prevents receipt substitution across request identities. |
| ADMISSION-TP-006 | implemented | Boundary guard | ADMISSION-REQ-004 | A receipt whose policy epoch differs from the request returns `AdmissionReceiptMismatch`. | admission_receipt_must_echo_policy_epoch | crates/amblicit-syntax/tests/admission.rs | Receipt policy epoch echoing is checked as structured data. |
| ADMISSION-TP-007 | implemented | Boundary guard | ADMISSION-REQ-004 | A receipt body carrying a signing-envelope reference returns `ReceiptSignatureCycle`. | receipt_body_must_not_reference_its_signing_envelope | crates/amblicit-syntax/tests/admission.rs | Prevents self-referential signed bodies. |
| ADMISSION-TP-008 | implemented | Boundary guard | ADMISSION-REQ-004 | An accepted receipt carrying rejection evidence returns `AdmissionReceiptMismatch`. | accepted_receipt_cannot_carry_rejection_evidence | crates/amblicit-syntax/tests/admission.rs | Accepted and rejected receipt evidence remain mutually exclusive. |
| ADMISSION-TP-009 | implemented | Boundary guard | ADMISSION-REQ-004 | A receipt admitting an operation absent from the request returns `AdmissionReceiptMismatch`. | receipt_admitted_operations_must_be_requested | crates/amblicit-syntax/tests/admission.rs | Public receipt validation checks admitted operations against the request. |
| ADMISSION-TP-010 | implemented | Boundary guard | ADMISSION-REQ-004 | A receipt admitting a capability absent from the request returns `AdmissionReceiptMismatch`. | receipt_admitted_capabilities_must_be_requested | crates/amblicit-syntax/tests/admission.rs | Public receipt validation checks admitted capabilities against the request. |
| ADMISSION-TP-011 | implemented | Boundary guard | ADMISSION-REQ-005 | An LLM-authored artifact with no admission receipt returns `MissingAdmissionReceipt`. | llm_authored_artifact_still_requires_admission_receipt | crates/amblicit-syntax/tests/admission.rs | Authorship provenance does not grant admission. |
| ADMISSION-TP-012 | implemented | Boundary guard | ADMISSION-REQ-005 | Registration evidence without an invocation capability returns `RegistrationReceiptIsNotInvocationAuthority`. | registration_receipt_does_not_authorize_invocation | crates/amblicit-syntax/tests/admission.rs | Registration and invocation authority remain distinct. |
| ADMISSION-TP-013 | implemented | Boundary guard | ADMISSION-REQ-006 | A participant policy rejection validates as receipt evidence but returns `MissingAcceptedAdmissionReceipt` for Gate C invocation. | participant_policy_rejection_is_evidence_not_invocation_authority | crates/amblicit-syntax/tests/admission.rs | Edict checks structure without turning policy rejection into invocation authority. |
| ADMISSION-TP-014 | implemented | Boundary guard | ADMISSION-REQ-005 | Naming an invoked operation absent from the request returns `OperationRequirementMismatch`. | invoked_operation_must_be_in_requested_operations | crates/amblicit-syntax/tests/admission.rs | The invocation operation is explicit evidence, not an implicit list position. |
| ADMISSION-TP-015 | implemented | Boundary guard | ADMISSION-REQ-005 | An accepted receipt that does not admit the invoked operation returns `MissingAcceptedAdmissionReceipt`. | accepted_receipt_must_admit_invoked_operation | crates/amblicit-syntax/tests/admission.rs | The accepted receipt must cover the operation being invoked. |
| ADMISSION-TP-016 | implemented | Boundary guard | ADMISSION-REQ-005 | An invocation capability for a different participant returns `MissingInvocationCapability`. | invocation_capability_must_match_receipt_participant | crates/amblicit-syntax/tests/admission.rs | Participant matching is part of invocation evidence binding. |
| ADMISSION-TP-017 | implemented | Boundary guard | ADMISSION-REQ-005 | An invocation capability for a different policy epoch returns `MissingInvocationCapability`. | invocation_capability_must_match_policy_epoch | crates/amblicit-syntax/tests/admission.rs | Capability policy epoch matching is part of invocation evidence binding. |
| ADMISSION-TP-018 | implemented | Boundary guard | ADMISSION-REQ-005 | An invocation capability for an unadmitted scope returns `MissingInvocationCapability`. | invocation_capability_scope_must_be_admitted | crates/amblicit-syntax/tests/admission.rs | Capability scope matching is part of invocation evidence binding. |
| ADMISSION-TP-019 | implemented | Golden path | ADMISSION-REQ-005 | A matching accepted receipt and invocation capability returns `AdmissionValidationStatus::Valid` with no failures. | accepted_receipt_and_invocation_capability_authorize_gate_c_invocation | crates/amblicit-syntax/tests/admission.rs | Proves the positive Gate C invocation evidence shape. |

## Determinism Obligations

- Fixtures are in-memory typed values.
- Assertions use structured statuses and stable failure kinds.
- No test inspects stdout, stderr, diagnostic prose, serialized bytes,
  documentation text, repository structure, network state, or wall-clock time.
- The contract graph is checked by `cargo xtask contract-check`.

## Open Gaps

- File-backed admission artifact loading.
- Canonical-CBOR encode/decode helpers for admission artifacts.
- Participant authentication and host attestation.
- Participant policy evaluation.
- Capability delegation and revocation evaluation.
- Target lowerer execution and Target IR generation.
- Distribution-envelope signature verification.
