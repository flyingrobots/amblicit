# Assurance Topic

Status: current HEAD contract.

This shelf describes the assurance boundary that exists today. Edict currently
models assurance evidence as optional, hash-bound references inside
participant-neutral contract bundle manifests. It does not ship HOLMES, Watson,
Moriarty, profile-diff, transparency-log, or hash-ladder tools.

## Public Surface

The `amblicit_syntax` crate exposes contract-bundle types for assurance evidence:

- `AssuranceRole`, covering HOLMES, Watson, and Moriarty;
- `AssuranceEvidenceRef`, binding an evidence artifact to a bundle subject,
  target profile digest, and target IR digest;
- `validate_contract_bundle_manifest`, which validates those references when
  present. [ASSURANCE-REQ-001] [ASSURANCE-REQ-002]

Assurance design guidance lives in
[`docs/GUIDE_edict-assurance-transparency.md`](../../GUIDE_edict-assurance-transparency.md).
The guide names future platform artifacts. The executable contract in this repo
is currently the typed bundle validation boundary.

## Current Contract

- Assurance evidence is optional in a contract bundle. A bundle can be valid
  with no external assurance evidence. [ASSURANCE-REQ-001]
- When assurance evidence is present, its artifact reference must be
  digest-locked and its subject digest must match the selected semantic or
  release bundle subject. [ASSURANCE-REQ-001]
- Assurance evidence must bind to the same target-profile digest and target-IR
  digest as the bundle it is attached to. [ASSURANCE-REQ-002]
- Admission artifacts are not participant-neutral assurance evidence and are
  rejected from the contract bundle manifest. [ASSURANCE-REQ-003]

## Deferred

The following are not implemented:

- HOLMES lawfulness certificate generation;
- Watson explanations or repair guidance;
- Moriarty mutation matrices or relapse fuzzing;
- profile diff;
- transparency-log publication;
- `amblicit explain --hash-ladder`;
- admission explanations.

Those are platform/tooling milestones outside the current Edict validation
surface. [ASSURANCE-REQ-004]

The verification matrix is tracked in [test-plan.md](./test-plan.md).
