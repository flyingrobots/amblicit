# Semantic Validation Test Plan

Status: current verification design for source-AST semantic validation.

## Scope

In scope:

- `amblicit_syntax::validate_surface`;
- `amblicit_syntax::validate_module` as a compatibility alias for the same stage;
- stable `SemanticErrorKind` identities;
- source-AST checks independent of import resolution, resolved typing,
  target/lawpack facts, and Core IR;
- deterministic fixture and oracle mapping.

Out of scope for this first slice:

- resolved type checking;
- import/lawpack/target profile validation;
- full Core IR lowering and relapse-zoo golden artifacts;
- participant/runtime admission checks.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| SEMVAL-REQ-001 | implemented | Validation returns structured semantic errors with stable kinds and source span payloads. | crates/amblicit-syntax/src/semantic.rs |
| SEMVAL-REQ-002 | implemented | Runtime `String` and `Bytes` type references must be explicitly bounded. | EDICT-LANG-BOUNDS-001 |
| SEMVAL-REQ-003 | implemented | Each intent must declare at least one operation mode: `profile` or `implements`. | EDICT-LANG-INTENT-CLAUSES-001 |
| SEMVAL-REQ-004 | implemented | Each intent must declare a `budget` clause. | EDICT-LANG-INTENT-CLAUSES-001 |
| SEMVAL-REQ-005 | implemented | Each intent must declare a `basis` clause until template resolution exists. | EDICT-LANG-INTENT-CLAUSES-001 |
| SEMVAL-REQ-006 | implemented | Singleton intent clauses reject duplicates. | docs/SPEC_edict-language-v1.md |
| SEMVAL-REQ-007 | implemented | Source binders must not shadow visible module/prelude names, parameters, or earlier locals; module-scope import/declaration names cannot collide. | EDICT-LANG-NOSHADOW-001 |
| SEMVAL-REQ-008 | implemented | The source/surface validator is an explicit `validate_surface` compiler stage; `validate_module` remains a compatibility alias. | issue #10 |
| SEMVAL-REQ-009 | implemented | Surface validation does not require import resolution, named type resolution, callee resolution, or target/lawpack facts. | issue #10 |
| SEMVAL-REQ-010 | implemented | Contextual integer typing, loop-bound proof, and obstruction exhaustiveness are deferred to later compiler-spine stages. | issue #10 |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| fixtures/lang/bounds/bounded-hello.amblicit | Positive bounded scalar and required-clause fixture. | `validate_surface` returns `Ok(())`. |
| fixtures/lang/effects/read-greeting.amblicit | Positive effect body and obstruction fixture. | `validate_surface` returns `Ok(())`. |
| fixtures/lang/effects/conditional-blob.amblicit | Positive branch-yield fixture. | `validate_surface` returns `Ok(())`. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SEMVAL-TP-001 | implemented | Golden path | SEMVAL-REQ-001 | Valid Phase 1 fixtures return `Ok(())`. | phase1_fixtures_validate_semantically | fixtures/lang/bounds/bounded-hello.amblicit, fixtures/lang/effects/read-greeting.amblicit, fixtures/lang/effects/conditional-blob.amblicit | Source-AST validator only. |
| SEMVAL-TP-002 | implemented | Error handling | SEMVAL-REQ-002 | Nested unbounded scalars produce `UnboundedScalar` for each occurrence. | unbounded_runtime_scalars_are_rejected_recursively, unbounded_runtime_scalars_are_rejected_in_declaration_type_surfaces, unbounded_runtime_scalars_are_rejected_in_intent_and_expression_surfaces | - | Covers `Option`, `List`, `Map`, `CapabilityRef`, variant payloads, intent params/returns, typed `let`, and expression type args. |
| SEMVAL-TP-003 | implemented | Error handling | SEMVAL-REQ-003, SEMVAL-REQ-004, SEMVAL-REQ-005 | Missing required intent clauses produce `MissingOperationMode`, `MissingBudget`, and `MissingBasis`. | intent_required_clauses_are_validated | - | Does not require import resolution. |
| SEMVAL-TP-004 | implemented | Golden path | SEMVAL-REQ-003 | Either `profile` or `implements` satisfies operation mode. | profile_or_implements_satisfies_operation_mode | - | Both remain legal. |
| SEMVAL-TP-005 | implemented | Error handling | SEMVAL-REQ-006 | Duplicate singleton clauses produce `DuplicateIntentClause`. | duplicate_singleton_intent_clauses_are_rejected, duplicate_implements_and_footprint_clauses_are_rejected | - | Covers `profile`, `implements`, `basis`, `footprint`, and `budget` duplicates. |
| SEMVAL-TP-006 | implemented | Error handling | SEMVAL-REQ-007 | Module-scope declaration and import aliases cannot collide in the same namespace. | module_namespace_collisions_are_rejected | - | Source-AST environment only. |
| SEMVAL-TP-007 | implemented | Error handling | SEMVAL-REQ-007 | Intent parameters and local binders cannot shadow module names, parameters, or earlier locals. | local_binders_cannot_shadow_visible_names | - | Source-AST environment only. |
| SEMVAL-TP-008 | implemented | Edge/scope | SEMVAL-REQ-007 | Branch, loop, match, obstruction-map, and branch-yield binders are scoped deterministically and do not leak into sibling or outer scopes. | branch_and_loop_binders_are_scoped, branch_yield_binders_are_scoped, clause_expression_binders_see_parameters, expression_binders_cannot_shadow_visible_names, obstruction_map_binders_cannot_shadow_visible_names | - | Source-AST environment only. |
| SEMVAL-TP-009 | implemented | Stage boundary | SEMVAL-REQ-008 | `validate_surface` accepts known-good fixtures and `validate_module` remains equivalent for compatibility. | validate_module_remains_surface_stage_compatibility_alias | fixtures/lang/bounds/bounded-hello.amblicit | Keeps the stage name explicit without breaking existing callers. |
| SEMVAL-TP-010 | implemented | Boundary guard | SEMVAL-REQ-009 | Unresolved imports, named types, callees, and field paths do not fail surface validation when no source/surface rule is violated. | surface_validation_defers_import_and_name_resolution | - | Resolution belongs to the resolver stage. |
| SEMVAL-TP-011 | implemented | Boundary guard | SEMVAL-REQ-010 | Contextual integer mismatch and unresolved loop cardinality do not fail surface validation. | surface_validation_defers_contextual_typing_and_loop_bound_proof | - | Type checking and cardinality proof are downstream. |
| SEMVAL-TP-012 | implemented | Boundary guard | SEMVAL-REQ-010 | A partial-looking obstruction map does not fail surface validation without target/lawpack failure facts. | surface_validation_defers_obstruction_exhaustiveness | - | Exhaustiveness belongs after target/lawpack facts exist. |

## Determinism Obligations

- Semantic tests inspect returned `SemanticErrorKind` values and AST validation
  state.
- Exact error ordering is not a Phase 2 contract; tests may normalize errors when
  a case is about identity rather than traversal order.
- Tests do not inspect stdout, stderr, logs, or diagnostic prose.
- Source inputs are inline strings or checked-in fixtures.
- The contract graph is checked by `cargo xtask contract-check`.

## Open Gaps

- Contextual typing for integer suffix checks belongs to `type_check`.
- Cardinality proof machinery for loop bounds belongs after resolution.
- Target/lawpack facts for obstruction exhaustiveness belong after import and
  target/lawpack resolution.
- Core/assurance relapse-zoo fixtures belong to downstream Core/admission topic
  shelves after full Core lowering and canonical artifacts exist.
- Clause-level diagnostic spans; duplicate singleton diagnostics currently report
  at the enclosing intent span because intent clauses do not retain spans.


## Variant case identity

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| SEMVAL-REQ-011 | implemented | Duplicate variant case labels report DuplicateName at the repeated case span before Core construction. | crates/amblicit-syntax/src/semantic.rs |

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SEMVAL-TP-013 | implemented | Variant names | SEMVAL-REQ-011 | Repeated labels fail validate_surface and compile_to_core with stable structured error kinds. | variant_empty_and_duplicate_case_sets_refuse | crates/amblicit-syntax/tests/source_variants.rs | Payload bound checking remains part of surface traversal. |
