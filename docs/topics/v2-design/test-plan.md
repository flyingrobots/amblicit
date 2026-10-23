# v2 Design Test Plan

Status: current verification plan for future adapter-composition behavior.

This plan records the design contract for issue #4. It intentionally does not
claim executable v2 behavior in HEAD.

## Scope

In scope:

- lowering-obligation terminology;
- future adapter `consumes` / `provides` / `requires` declarations;
- digest-locked adapter coordinates;
- fixed-point obligation-closure resolution;
- deterministic candidate ordering and ambiguity policy;
- version and conflict solving boundary;
- cycle rejection;
- closure evidence hashing requirements;
- structured diagnostics for unresolved or ambiguous obligations;
- the v1 direct-adapter boundary.

Out of scope:

- implementing v2 adapter declarations in `amblicit_syntax`;
- accepting lawpack adapter ABI declarations in target profiles;
- changing v1 lowerability behavior;
- Target IR generation;
- registry, package-manager, or network resolution behavior;
- admission, capability receipt, or participant-policy behavior.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| V2DESIGN-REQ-001 | planned | v2 composition uses lowering-obligation terminology; capability remains reserved for participant authority and admission concepts. | issue #4 |
| V2DESIGN-REQ-002 | planned | v2 adapters declare digest-locked coordinates plus `consumes`, `provides`, and `requires` lowering-obligation sets. | issue #4, docs/design/v2-obligation-closure.md |
| V2DESIGN-REQ-003 | planned | v2 resolution computes a monotonic fixed point over a closed adapter universe, selects only adapters that consume current unresolved obligations, and succeeds only when all root obligations are discharged. | issue #4, docs/design/v2-obligation-closure.md |
| V2DESIGN-REQ-004 | planned | candidate ordering, version solving, conflict handling, and ambiguity rejection are deterministic and input-order independent. | issue #4 |
| V2DESIGN-REQ-005 | planned | adapter dependency cycles, unsatisfied transitive obligations, ABI incompatibilities, version conflicts, and non-digest-locked candidates produce structured diagnostics. | issue #4, docs/design/v2-obligation-closure.md |
| V2DESIGN-REQ-006 | planned | selected adapter sets, consumed obligations, and closure evidence are canonicalized and hash-bound so equivalent input ordering is stable and semantic changes alter evidence digests. | issue #4, docs/design/v2-obligation-closure.md |
| V2DESIGN-REQ-007 | implemented | HEAD remains v1 direct-adapter only; chained or composite adapter claims stay unsupported until future v2 behavior lands. | docs/topics/lowerability/test-plan.md |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| crates/amblicit-syntax/tests/lowerability.rs | Existing v1 boundary fixtures. | Chained, floating, and ambiguous direct adapters remain unsupported. |
| planned v2 adapter-closure fixture corpus | Planned positive and negative v2 closure cases. | Must prove consumes-gated fixed-point resolution, ambiguity rejection, cycle rejection, and stable evidence hashing. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| V2DESIGN-TP-001 | planned | Terminology guard | V2DESIGN-REQ-001 | Future public APIs and diagnostics distinguish lowering obligations from participant capabilities. | - | - | Do not encode prose-string tests. |
| V2DESIGN-TP-002 | planned | Adapter schema | V2DESIGN-REQ-002 | Future typed adapter declarations reject missing digest locks and malformed obligation sets. | - | - | Assert structured validation errors. |
| V2DESIGN-TP-003 | planned | Closure success | V2DESIGN-REQ-003, V2DESIGN-REQ-006 | A closed acyclic adapter universe selects only adapters that consume current unresolved obligations and discharges root and transitive obligations with stable closure evidence. | - | - | Assert selected adapter coordinates, consumed obligations, and evidence digest behavior. |
| V2DESIGN-TP-004 | planned | Ordering and ambiguity | V2DESIGN-REQ-004 | Reordered equivalent candidates produce the same result, while semantically distinct candidate sets reject as ambiguous. | - | - | Assert stable ambiguity kinds, not diagnostic prose. |
| V2DESIGN-TP-005 | planned | Failure diagnostics | V2DESIGN-REQ-005 | Unsatisfied transitive obligations, adapter dependency cycles, ABI conflicts, version conflicts, and floating candidates reject with stable diagnostic kinds. | - | - | Diagnostics are structured values. |
| V2DESIGN-TP-006 | implemented | v1 boundary | V2DESIGN-REQ-007 | Existing v1 lowerability rejects chained adapter claims with a stable failure kind. | v1_rejects_chained_adapter_claims | crates/amblicit-syntax/tests/lowerability.rs | Existing behavior guard; no v2 implementation claim. |

## Determinism Obligations

- Future closure fixtures must be closed local inputs; no test may fetch
  registries, packages, network resources, or wall-clock data.
- Future resolver tests must assert structured outcomes, selected adapter
  coordinates, stable failure kinds, and evidence digest behavior.
- Tests must not assert prose wording, file layout, or implementation branch
  choices.
- The existing v1 boundary remains guarded by lowerability behavior tests.

## Open Gaps

- No typed v2 adapter declaration exists.
- No adapter-closure resolver exists.
- No closure evidence artifact exists.
- No closure evidence canonicalization or digest exists.
- No v2 diagnostic enum exists.
- No positive or negative v2 adapter-closure fixture corpus exists.
