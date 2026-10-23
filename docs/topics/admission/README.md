# Admission Topic

Status: current HEAD contract.

This chapter describes the typed Gate C admission-boundary checks that exist
today. Edict validates the artifact and operation semantics that Continuum
admission artifacts carry. It does not authenticate participants, evaluate
participant policy, decide revocation, run target lowerers, or execute admitted
operations.

## Public Surface

The `amblicit_syntax` crate exposes:

- `validate_admission_request`, which checks Edict-owned fields of a Continuum
  admission request against a typed `ContractBundleManifest`;
- `digest_admission_request`, which computes the domain-separated digest for a
  typed admission request;
- `validate_admission_receipt`, which checks that a receipt body echoes its
  request digest, request fields, admitted operations, and admitted
  capabilities, and remains separate from its signing envelope;
- `check_gate_c_invocation`, which checks that invocation evidence names a
  requested operation and contains an accepted admission receipt plus a matching
  invocation capability receipt;
- typed admission structures including `AdmissionRequest`,
  `AdmissionReceiptBody`, `OperationRequirementRef`, `ExecutionInputRef`,
  `CapabilityReceipt`, and `GateCInvocation`;
- stable `AdmissionValidationFailureKind` categories. [ADMISSION-REQ-001]

## Current Contract

- Admission requests reference a `bundleSubject` whose `kind` selects either the
  semantic or release bundle digest from the contract bundle manifest. A stale
  release subject is rejected even when the semantic subject still matches.
  [ADMISSION-REQ-001]
- Operation requirements bind the same bundle subject as the admission request,
  a concrete operation coordinate, a digest-locked basis artifact, a canonical
  variables digest, and a requirements digest. [ADMISSION-REQ-002]
- Execution inputs below the determinism boundary must be materialized as
  canonical input, witnessed evidence, admitted basis, or capability
  presentation. Hidden host inputs are rejected. [ADMISSION-REQ-003]
- Admission receipt bodies must echo the request's bundle subject and policy
  epoch. They must also carry the domain-separated digest of the typed admission
  request. Receipt admitted operations and admitted capabilities must be subsets
  of the requested operations and capabilities. Accepted receipt bodies must not
  carry rejection evidence, and receipt bodies must not reference their own
  signing envelope. [ADMISSION-REQ-004]
- Gate C invocation names the operation being invoked. That operation must
  appear in the requested operation requirements, be admitted by an accepted
  admission receipt, and carry an invocation capability receipt matching the
  bundle subject, operation coordinate, participant, admitted capability scope,
  and policy epoch.
  Registration evidence alone never grants invocation authority, and authoring
  provenance never bypasses admission. [ADMISSION-REQ-005]
- Edict owns artifact and operation semantics: bundle subjects, semantic versus
  release digest selection, operation coordinates, basis/variables/requirements
  binding, hidden execution input rejection, and compiler/lowering failure
  classes. Continuum owns participant identity, participant policy, capability
  delegation, revocation, policy epochs, and effective-authority evaluation.
  [ADMISSION-REQ-006]

## Deferred

The following are not implemented by this admission-boundary slice:

- file-backed admission request or receipt loading;
- canonical-CBOR encoding for admission artifacts;
- participant authentication or host attestation;
- participant policy evaluation;
- capability delegation or revocation evaluation;
- target lowerer execution or Target IR generation;
- admission explanation generation;
- distribution-envelope signature verification.

The verification matrix is tracked in [test-plan.md](./test-plan.md).
