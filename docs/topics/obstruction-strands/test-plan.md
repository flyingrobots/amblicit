# Obstruction Strands Test Plan

Status: current verification ledger for `require` obstruction source syntax and
planned verification design for future Target IR and runtime obstruction
semantics.

## Scope

In scope:

- current parser support for `require ... else` obstruction source shape;
- current Core lowering for `Stmt::Require` obstruction arms;
- current Echo Target IR lowering for Core `require` requirements;
- first-class source syntax for preserved repairable obstruction attempts;
- negative guard against hidden control flow in ordinary obstruction-target
  constructors.

Out of scope:

- Echo or Jim runtime execution;
- Continuum support-ledger verification;
- XYPH Quest settlement policy.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| OBSTRAND-REQ-001 | implemented | Current `require ... else <obstruction>` parses as `Stmt::Require` source syntax and does not imply hidden continuation semantics. | docs/topics/syntax/test-plan.md |
| OBSTRAND-REQ-002 | implemented | The `require ... else continue obstructed { reason: ... }` form parses as first-class source syntax, not an ordinary helper call hidden inside `else`. | issue #118, docs/design/obstruction-strands-v0.md |
| OBSTRAND-REQ-003 | implemented | The `continue obstructed { ... }` form is contextual to `require ... else`, requires exactly one `reason` field, and rejects duplicate `reason` fields before Core digesting. | issue #118, docs/design/obstruction-strands-v0.md |
| OBSTRAND-REQ-004 | implemented | Helper-shaped obstruction constructors such as `continueInObstructedStrand(...)` do not acquire hidden continuation semantics. | issue #118, docs/design/obstruction-strands-v0.md |
| OBSTRAND-REQ-005 | implemented | Echo Target IR exposes a distinct preserved-obstruction disposition. | issue #131, docs/design/obstruction-strands-v0.md |
| OBSTRAND-REQ-006 | implemented | Preserved obstruction Target IR records no success-path write and carries repair metadata only as causal support. | issue #131 |
| OBSTRAND-REQ-007 | implemented | Core lowering preserves terminal require obstruction and preserved obstruction continuation as distinct require-failure arms. | issue #129, docs/design/obstruction-strands-v0.md |
| OBSTRAND-REQ-008 | implemented | Core digest behavior binds require-failure disposition, reason kind, and canonical reason payload values while ignoring non-semantic formatting. | issue #129, docs/design/obstruction-strands-v0.md |
| OBSTRAND-REQ-009 | implemented | The cross-project taxonomy distinguishes not-admitted scheduler counterfactuals, admitted obstructed strands, and hard rejections without assigning another layer's authority to Edict, Graft, or jedit. | issue #133, docs/design/obstruction-strands-v0.md |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| docs/topics/obstruction-strands/README.md | Current boundary shelf. | Human review confirms HEAD claims parser and Core support only for the currently lowerable `require` obstruction subset. |
| docs/design/obstruction-strands-v0.md | Future design note and cross-project taxonomy. | Human review confirms planned syntax/lowering material is outside the topic README contract, separates not-admitted scheduler counterfactuals, admitted obstructed strands, and hard rejections, and does not claim runtime/editor behavior in Edict. |
| docs/topics/syntax/test-plan.md | Syntax ledger for future parser coverage. | `SYNTAX-REQ-013` and `SYNTAX-TP-023` preserve the planned parser guard. |
| docs/topics/target-ir/README.md | Target IR current-boundary note. | Target IR explicitly claims Echo requirement disposition support while leaving Echo receipts and runtime execution deferred. |
| fixtures/obstruction-strands/v0/stale-basis/README.md | Golden corridor manifest. | Human review confirms future artifacts appear only after the owning layer lands. |
| fixtures/obstruction-strands/v0/stale-basis/source.amblicit | Stale-basis source corridor fixture. | Parsed by `stale_basis_obstruction_strand_fixture_parses`. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| OBSTRAND-TP-001 | implemented | Current parser contract | OBSTRAND-REQ-001 | Current `require ... else` parses as `Stmt::Require` with a predicate and obstruction target. | require_statement_parses_terminal_obstruction_source_shape | crates/amblicit-syntax/tests/parse_review_regressions.rs | Parser evidence only; compiler lowering for `Stmt::Require` remains outside the landed boundary. |
| OBSTRAND-TP-002 | implemented | Syntax guard | OBSTRAND-REQ-002 | Parser introduces a distinct source AST arm for first-class preserved obstruction syntax. | continue_obstructed_source_arm_parses, stale_basis_obstruction_strand_fixture_parses | crates/amblicit-syntax/tests/parse_review_regressions.rs, fixtures/obstruction-strands/v0/stale-basis/source.amblicit | Parser evidence only. |
| OBSTRAND-TP-003 | implemented | Source validation | OBSTRAND-REQ-003 | `continue obstructed { ... }` requires a `reason` field and rejects duplicate `reason` fields. | continue_obstructed_requires_reason_field, continue_obstructed_rejects_duplicate_reason_field | crates/amblicit-syntax/tests/parse_review_regressions.rs | Rejection happens before Core digesting. |
| OBSTRAND-TP-004 | implemented | Contextual syntax guard | OBSTRAND-REQ-003 | `continue obstructed { ... }` is not a normal expression outside a require-else arm. | continue_obstructed_is_contextual_to_require_else | crates/amblicit-syntax/tests/parse_review_regressions.rs | Prevents function-like or value-like use. |
| OBSTRAND-TP-005 | implemented | Negative guard | OBSTRAND-REQ-004 | `else continueInObstructedStrand(...)` is not interpreted as hidden control flow. | helper_shaped_continue_in_obstructed_strand_is_terminal | crates/amblicit-syntax/tests/parse_review_regressions.rs | It remains an ordinary terminal obstruction target. |
| OBSTRAND-TP-006 | implemented | Target IR guard | OBSTRAND-REQ-005, OBSTRAND-REQ-006 | Lowering emits a distinct preserved-obstruction disposition with no success-path write. | echo_target_ir_contains_obstruction_requirement_payload, terminal_and_preserved_requirements_are_target_ir_distinct | crates/amblicit-syntax/tests/target_ir.rs | Must remain separate from terminal obstruction and success. |
| OBSTRAND-TP-007 | implemented | Core lowering | OBSTRAND-REQ-007 | Terminal `require ... else <obstruction>` lowers to a terminal require-failure arm. | terminal_require_obstruction_lowers_to_core_failure_arm | crates/amblicit-syntax/tests/compiler_spine.rs | Core evidence only; no Target IR or Echo claim. |
| OBSTRAND-TP-008 | implemented | Core lowering | OBSTRAND-REQ-007 | `require ... else continue obstructed { ... }` lowers to a preserved-obstruction require-failure arm. | continue_obstructed_require_lowers_to_core_failure_arm | crates/amblicit-syntax/tests/compiler_spine.rs | Core evidence only; no Target IR or Echo claim. |
| OBSTRAND-TP-009 | implemented | Core distinction | OBSTRAND-REQ-007, OBSTRAND-REQ-008 | Terminal obstruction and `continue obstructed` with the same reason are Core-distinct and digest-distinct. | terminal_and_continue_obstructed_require_arms_are_core_distinct | crates/amblicit-syntax/tests/compiler_spine.rs | Prevents semantic collapse into one failure bucket. |
| OBSTRAND-TP-010 | implemented | Core digest mutation | OBSTRAND-REQ-008 | Reason kind and reason payload value mutations move the Core digest, while payload field order and non-semantic formatting do not. | obstruction_reason_mutations_move_core_digest | crates/amblicit-syntax/tests/compiler_spine.rs | Payload fields are canonicalized before Core digesting. |
| OBSTRAND-TP-011 | implemented | Core validation | OBSTRAND-REQ-008 | Duplicate reason payload fields reject before Core digesting. | duplicate_obstruction_reason_payload_fields_reject_before_core_digest | crates/amblicit-syntax/tests/compiler_spine.rs | Stable structured compiler error, not digest drift. |
| OBSTRAND-TP-012 | implemented | Taxonomy boundary | OBSTRAND-REQ-009 | Taxonomy names the owner of not-admitted scheduler counterfactuals, admitted obstructed strands, and hard rejections, while preserving Edict/Graft/jedit non-authority over runtime semantics. | obstruction_taxonomy_boundary_uses_layer_authority_terms | docs/design/obstruction-strands-v0.md | Documentation boundary only; no runtime behavior is implemented by this case. |

## Determinism Obligations

- Future tests must assert structured AST/Core/Target IR values, not diagnostic
  prose.
- Future fixtures must be checked in under `fixtures/` or explicit topic test
  sources.
- No future test may depend on wall-clock time, runtime execution, network
  state, or live Continuum/XYPH services.

## Open Gaps

- Edict does not own Echo runtime receipts, scheduler counterfactual
  exploration, Graft projection, jedit display, Continuum settlement, or XYPH
  settlement.
