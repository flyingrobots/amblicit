# Typed Function Failures Test Plan

Status: verification ledger for declared failures on source functions,
propagation with `expect`, intent-level mapping with `else`, and their Core
and Target IR representation.

## Scope

In scope:

- parsing of `failure` declarations, `fails` clauses, and `fail` and `expect`
  expressions as contextual syntax;
- the source compiler's failure-set, propagation and mapping judgments;
- the Core representation and its independent integrity judgment;
- Target IR lowering of mapped calls and its execution-model boundary;
- the provider contract pack that publishes the new schema.

Out of scope:

- Echo's provider lowerer, verifier and runtime, which must adopt
  `core.failure/v1` before a failing program can run;
- failure payloads, mapping inside function bodies, and other features the
  Jedit rope drafts mark as proposed (function `where`, `decreases`, tuple
  patterns, wildcard arms).

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| TFAIL-REQ-001 | implemented | `failure Name;` declares a payload-free, module-owned failure whose coordinate is the declaring module coordinate plus its name, also under source linking; `fails` names resolve to declared failures with no repeats and no ambiguous bare names. | EDICT-LANG-FAIL-DECL-001 |
| TFAIL-REQ-002 | implemented | A function's declared set equals exactly the failures its body produces with `fail` and `expect`; `fail` takes its context type and is legal only inside a function that declares it. | EDICT-LANG-FAIL-SET-001 |
| TFAIL-REQ-003 | implemented | `expect` propagates a direct callee's whole failure set only inside a function that declares that set; every other failing call refuses with `UnhandledFailure`. | EDICT-LANG-FAIL-EXPECT-001 |
| TFAIL-REQ-004 | implemented | An intent `let` whose whole value is a failing source call maps each callee failure exactly once to a terminal obstruction; one-failure shorthand only; missing, unknown, repeated and binding arms refuse. | EDICT-LANG-FAIL-MAP-001 |
| TFAIL-REQ-005 | implemented | Core carries sorted function `failures`, `fail` expressions and `attempt` nodes and requires `core.failure/v1`. It omits them all when unused, so existing artifacts stay byte-identical, and its integrity judgment independently refuses forged sets, placements and maps. | EDICT-CORE-FAIL-001 |
| TFAIL-REQ-006 | implemented | Target lowers an `attempt` to a pure binding with `failureArms` under the ordered and structured models; the pre-step-guard model refuses `failure_mapped_binding`. | EDICT-TARGET-FAIL-001 |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TFAIL-TP-001 | implemented | Parser | TFAIL-REQ-001 | `failure`, `fails`, `fail` and `expect` parse into their AST forms in the positions the grammar gives them. | failure_syntax_parses_into_declarations_clauses_and_expressions | crates/amblicit-syntax/tests/typed_failures.rs | Grammar in the language spec. |
| TFAIL-TP-002 | implemented | Contextual syntax | TFAIL-REQ-001 | The four words stay ordinary identifiers (locals, fields, effect failure names) everywhere else. | failure_words_remain_ordinary_identifiers_elsewhere | crates/amblicit-syntax/tests/typed_failures.rs | Existing programs keep compiling. |
| TFAIL-TP-003 | implemented | Declarations | TFAIL-REQ-001, TFAIL-REQ-005 | Declared sets compile to sorted module-owned coordinates and add `core.failure/v1`. | failure_declarations_compile_to_sorted_module_coordinates | crates/amblicit-syntax/tests/typed_failures.rs | Spec example coordinates. |
| TFAIL-TP-004 | implemented | Declarations | TFAIL-REQ-001 | An unknown name reports `UnknownFailure` at its span; a repeated declaration reports the existing `SurfaceValidation` duplicate-name refusal; a repeated or ambiguous `fails` entry reports `UnsupportedSourceShape` at the function. | unknown_repeated_or_ambiguous_failure_names_refuse, linked_modules_keep_original_failure_coordinates | crates/amblicit-syntax/tests/typed_failures.rs | Map keys must identify one failure. |
| TFAIL-TP-005 | implemented | Exact sets | TFAIL-REQ-002 | A body that produces exactly its declared failures through `fail` in returns, conditionals and matches compiles. | declared_failure_set_is_exactly_what_the_body_produces | crates/amblicit-syntax/tests/typed_failures.rs | `fail` takes its context type. |
| TFAIL-TP-006 | implemented | Exact sets | TFAIL-REQ-002 | Undeclared, unraised and context-free `fail` report `UndeclaredFailure`, `UnraisedFailure` and `TypeMismatch`. | undeclared_and_unraised_failures_refuse | crates/amblicit-syntax/tests/typed_failures.rs | The judgment is syntactic. |
| TFAIL-TP-007 | implemented | Propagation | TFAIL-REQ-003 | `expect` in arguments, record fields and `let` values propagates the callee set. | expect_propagates_callee_failures_through_nested_calls | crates/amblicit-syntax/tests/typed_failures.rs | Calls keep their ordinary Core form. |
| TFAIL-TP-008 | implemented | Propagation | TFAIL-REQ-003 | A failing call without `expect`, `expect` in an intent, `expect` on a failure-free call, a widening `expect`, and a failing `fold` step refuse with their spec kinds. | failing_calls_without_expect_or_mapping_refuse | crates/amblicit-syntax/tests/typed_failures.rs | Precise diagnostic kinds. |
| TFAIL-TP-009 | implemented | Mapping | TFAIL-REQ-004 | A mapped intent `let` becomes an `attempt` node whose map has one terminal reason per callee failure. | intent_maps_every_failure_to_a_terminal_obstruction, one_failure_shorthand_maps_to_a_single_arm | crates/amblicit-syntax/tests/typed_failures.rs | Payloads may read earlier bindings. |
| TFAIL-TP-010 | implemented | Mapping | TFAIL-REQ-004 | Incomplete, unknown, repeated, binding and shorthand-misuse maps refuse with their spec kinds. | incomplete_unknown_repeated_and_binding_failure_maps_refuse | crates/amblicit-syntax/tests/typed_failures.rs | Same arm syntax as effects. |
| TFAIL-TP-011 | implemented | Linking | TFAIL-REQ-001, TFAIL-REQ-004 | A failure declared in an imported source module keeps its original coordinate and is mapped by its bare name. | linked_modules_keep_original_failure_coordinates | crates/amblicit-syntax/tests/typed_failures.rs | Hygienic link names never reach Core. |
| TFAIL-TP-012 | implemented | Core encoding | TFAIL-REQ-005 | Canonical Core carries `failures`, `fail` and `attempt` as specified, and a module without failures encodes as before. | core_failure_constructs_round_trip_canonically, failure_free_modules_carry_no_failure_fields_or_capability, reviewed_core_golden_bytes_match_executable_encoder, core_goldens_match_executable_encoder | crates/amblicit-syntax/tests/typed_failure_lowering.rs, crates/amblicit-syntax/tests/core_golden_fixtures.rs | Reviewed Core goldens stay byte-identical. |
| TFAIL-TP-013 | implemented | Core integrity | TFAIL-REQ-005 | Forged Core (unsorted or inexact sets, misplaced `fail`, unmapped failing call, wrong map keys, failing fold step, missing capability) refuses `InvalidDefinition` at the exact path. | core_integrity_refuses_forged_function_failure_sets, core_integrity_refuses_forged_intent_failure_constructs | crates/amblicit-syntax/tests/typed_failure_lowering.rs | Independent of the source compiler. |
| TFAIL-TP-014 | implemented | Target lowering | TFAIL-REQ-006 | Ordered and structured Target IR carry the mapped call as a pure binding with `failureArms`; existing Target goldens stay byte-identical. | attempt_lowers_to_failure_armed_pure_binding, target_ir_goldens_match_executable_encoder | crates/amblicit-syntax/tests/typed_failure_lowering.rs, xtask/src/tests.rs | Provider execution is out of scope. |
| TFAIL-TP-015 | implemented | Target lowering | TFAIL-REQ-006 | The pre-step-guard model refuses `UnsupportedCoreNode` with detail `failure_mapped_binding`. | pre_step_guard_target_refuses_failure_mapped_bindings | crates/amblicit-syntax/tests/typed_failure_lowering.rs | No silent ordering. |
| TFAIL-TP-016 | implemented | Consumer reproducer | TFAIL-REQ-003, TFAIL-REQ-004 | The reconciled Jedit `pure-typed-failure` reproducer compiles to Core with one `attempt`. | reconciled_jedit_typed_failure_reproducer_compiles | crates/amblicit-syntax/tests/typed_failures.rs, fixtures/lang/functions/pure-typed-failure.amblicit | Jedit's draft needs the edits listed in the README. |
| TFAIL-TP-017 | implemented | Provider schema | TFAIL-REQ-005, TFAIL-REQ-006 | The new contract pack accepts failure-bearing Core and Target IR; the earlier pack refuses them and still reproduces from frozen schemas. | typed_failure_contract_requires_new_schema_selection | crates/amblicit-provider-schema/tests/provider_contract_pack.rs, fixtures/provider-contracts/typed-failures-v1/edict-provider-contracts.cddl, fixtures/provider-contracts/typed-failures-v1/manifest.json, fixtures/core/pre-typed-failure-core.cddl, fixtures/target-ir/pre-typed-failure-target-ir.cddl | Schema acceptance is not runtime support. |
| TFAIL-TP-018 | gap | Runtime | TFAIL-REQ-005, TFAIL-REQ-006 | Echo executes a failing program and obstructs with the mapped reason. | - | - | Needs Echo support for `core.failure/v1`. |
