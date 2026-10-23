# Assurance Test Plan

Status: current verification design for assurance evidence boundaries.

## Scope

In scope:

- optional assurance evidence references in typed contract-bundle manifests;
- subject-digest binding for semantic or release bundle subjects;
- target-profile and target-IR digest binding;
- exclusion of admission artifacts from participant-neutral bundles.

Out of scope:

- HOLMES, Watson, and Moriarty executable tools;
- transparency logs;
- profile diff;
- hash-ladder explainers;
- admission explanations.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| ASSURANCE-REQ-001 | implemented | Contract-bundle assurance evidence is optional, role-tagged, digest-locked, and bound to the selected bundle subject when present. | crates/amblicit-syntax/src/contract_bundle.rs, docs/GUIDE_edict-assurance-transparency.md |
| ASSURANCE-REQ-002 | implemented | Assurance evidence must match the bundle target-profile digest and target-IR digest. | crates/amblicit-syntax/src/contract_bundle.rs |
| ASSURANCE-REQ-003 | implemented | Admission artifacts remain outside participant-neutral contract bundles. | crates/amblicit-syntax/src/contract_bundle.rs, docs/SPEC_continuum-admission-v1.md |
| ASSURANCE-REQ-004 | gap | HOLMES, Watson, Moriarty, transparency, profile-diff, and hash-ladder tooling are not implemented in this repository. | docs/GUIDE_edict-assurance-transparency.md, ROADMAP.md |

## Fixtures

No checked-in fixture files are required for the current assurance boundary.
The implemented tests construct typed bundle values directly and assert public
validation status and stable failure kinds.

## Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ASSURANCE-TP-001 | implemented | Contract bundle | ASSURANCE-REQ-001 | Bundles validate with role-tagged evidence, reject mismatched bundle-subject evidence, and remain valid when external assurance evidence is absent. | echo_and_kv_bundles_validate_with_the_same_runtime_neutral_contract, assurance_evidence_must_match_bundle_subject, external_assurance_evidence_is_optional | - | Tests assert validation status and stable failure kinds. |
| ASSURANCE-TP-002 | implemented | Contract bundle | ASSURANCE-REQ-002 | Assurance evidence whose target-profile or target-IR digest differs from the bundle rejects. | assurance_evidence_must_match_target_profile_digest, assurance_evidence_must_match_target_ir_digest | - | Prevents transplanting evidence between target artifacts. |
| ASSURANCE-TP-003 | implemented | Boundary guard | ASSURANCE-REQ-003 | Admission artifacts reject from participant-neutral contract bundle manifests. | admission_artifacts_are_rejected_from_contract_bundles | - | Admission evidence belongs outside the bundle. |
| ASSURANCE-TP-004 | gap | Future tooling | ASSURANCE-REQ-004 | No executable HOLMES, Watson, Moriarty, transparency, profile-diff, or hash-ladder tool is claimed. | - | - | Add with the owning platform/tooling slice. |

## Determinism Obligations

- Assurance tests must assert typed validation behavior and stable failure
  kinds.
- They must not assert guide prose, marketing names, or repository layout.
- Future assurance tools should add deterministic fixtures or generated
  artifacts before being marked implemented here.

## Open Gaps

- No executable assurance engines or explainers exist in this repository.
- No assurance fixture corpus exists beyond typed bundle values constructed in
  tests.
