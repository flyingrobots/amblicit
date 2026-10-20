# Result Projections Test Plan

## Scope

This shelf owns the compiler-authored, runtime-neutral representation of an
application operation result. It covers derivation from exact Core and Target
IR, canonical bytes and digest identity, source authority, boundedness, and
independent reverse reconstruction. Runtime evaluation and durable result
binding belong to Echo.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| RESULT-PROJ-REQ-001 | implemented | The compiler derives one operation result projection from exact Core and the matching Target IR result without host-authored application semantics. | issue #173 |
| RESULT-PROJ-REQ-002 | implemented | Projection leaves may name only the declared application input or a declared capability-step result, with explicit bounded field paths. | issue #173 |
| RESULT-PROJ-REQ-003 | implemented | `edict.result-projection/v1` has canonical CBOR bytes, a published CDDL root, and a domain-framed `edict.result-projection.artifact/v1` identity. | issue #173 |
| RESULT-PROJ-REQ-004 | implemented | Projection node count, path depth, text size, artifact bytes, and output bytes are bounded before the artifact can be admitted. | issue #173 |
| RESULT-PROJ-REQ-005 | implemented | Independent verification decodes the claimed bytes, recomputes their identity, reconstructs the authored Core result from declared sources, and requires exact Core and Target IR agreement. | issue #173 |
| RESULT-PROJ-REQ-006 | implemented | Mutated, incomplete, unsupported, undeclared-source, digest-substituted, malformed, and over-bound projections fail with stable error kinds. | issue #173 |
| RESULT-PROJ-REQ-007 | implemented | The representation remains target-neutral and contains no runtime callback, external effect, model call, or application-specific production behavior. | issue #173 |
| RESULT-PROJ-REQ-008 | implemented | Projection source validation fails closed when structured Core control flow cannot be represented by the supplied Target IR, including effects nested under loops or branches. | issue #192 |
| RESULT-PROJ-REQ-009 | implemented | A projection may cite one exact compiler-owned pure binding retained in matching Target IR; emission and independent verification reject missing, substituted, duplicated, or reordered binding authority. | issue #200 |
| RESULT-PROJ-REQ-010 | implemented | Byte-type projection compatibility treats Core bounds as intervals: a max-only source cannot be represented as an exact-byte output without proof of the minimum. | issue #200, docs/abi/edict-core.cddl |
| RESULT-PROJ-REQ-011 | implemented | Public result-projection emission and verification require the same whole-module Core type-integrity judgment used by canonical encoding and Target lowering; malformed or unresolved raw Core therefore cannot mint or verify projection authority. | issue #201, docs/topics/core-ir/test-plan.md |
| RESULT-PROJ-REQ-012 | implemented | Projection emission and verification reject Core whose named types exceed the fully expanded semantic depth limit even when the same definition was previously validated shallowly, before minting or accepting projection authority. | issue #201, docs/topics/core-ir/test-plan.md |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| `fixtures/lawpack/hello-echo/create-greeting.edict` | Real application source with one application-input field and one capability-result field in its result. | Compilation and Target IR lowering derive the reviewed projection without native Hello Echo code. |
| `fixtures/lawpack/hello-echo/create-greeting.result-projection.cbor` | Reviewed canonical projection bytes. | `cargo xtask lawpack-goldens --check` reproduces exact bytes through the compiler-owned emitter. |
| `fixtures/lawpack/hello-echo/create-greeting.result-projection.sha256` | Reviewed domain-framed projection identity. | The emitter and independent verifier reproduce the same identity. |
| `docs/abi/edict-result-projection.cddl` | Edict-owned canonical schema. | The provider contract pack validates a reference projection and rejects structural mutation. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| RESULT-PROJ-TP-001 | implemented | Golden path | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-002 | Exact Hello Echo Core and Target IR emit the expected typed record projection. | `exact_core_and_target_ir_emit_the_typed_hello_echo_projection` | fixtures/lawpack/hello-echo/create-greeting.edict, fixtures/lawpack/hello-echo/create-greeting.result-projection.cbor | The test enters through real source and lawpack compilation. |
| RESULT-PROJ-TP-002 | implemented | Independent verification | RESULT-PROJ-REQ-005 | The verifier reconstructs the exact authored Core result from projection sources and accepts the emitted identity. | `independent_verifier_reconstructs_the_authored_core_result` | crates/edict-syntax/tests/result_projection.rs | Verification runs in the reverse direction from projection to Core expression. |
| RESULT-PROJ-TP-003 | implemented | Mutation rejection | RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006 | A Target IR result that differs from its source Core result fails closed. | `mutated_target_result_fails_closed` | crates/edict-syntax/tests/result_projection.rs | Prevents target-owned result authorship. |
| RESULT-PROJ-TP-004 | implemented | Authority rejection | RESULT-PROJ-REQ-002, RESULT-PROJ-REQ-006, RESULT-PROJ-REQ-007 | Undeclared locals and call expressions fail with stable error kinds. | `undeclared_locals_and_unsupported_calls_fail_closed` | crates/edict-syntax/tests/result_projection.rs | No callback or general expression VM enters the artifact. |
| RESULT-PROJ-TP-005 | implemented | Shape and bound rejection | RESULT-PROJ-REQ-004, RESULT-PROJ-REQ-006 | Incomplete output records and a zero output-byte ceiling fail closed. | `incomplete_output_and_zero_output_bound_fail_closed` | crates/edict-syntax/tests/result_projection.rs | Output shape is checked against declared Core types. |
| RESULT-PROJ-TP-006 | implemented | Canonical identity | RESULT-PROJ-REQ-003, RESULT-PROJ-REQ-006 | Trailing bytes and a substituted digest fail independently. | `canonical_bytes_and_digest_are_independently_enforced` | fixtures/lawpack/hello-echo/create-greeting.result-projection.cbor, fixtures/lawpack/hello-echo/create-greeting.result-projection.sha256 | Canonicality and identity are separate gates. |
| RESULT-PROJ-TP-007 | implemented | Boundary | RESULT-PROJ-REQ-004 | The exact node limit encodes and the next node rejects. | `projection_node_limit_accepts_the_boundary_and_rejects_the_next_node` | crates/edict-syntax/tests/result_projection.rs | The root record counts as one node. |
| RESULT-PROJ-TP-008 | implemented | Property | RESULT-PROJ-REQ-003 | Sixty-four fixed-seed permutations of one canonical map produce identical canonical bytes. | `canonical_maps_are_insertion_order_independent_for_fixed_seed_cases` | crates/edict-syntax/tests/result_projection.rs | Fixed seed: `0x17305eedcafebabe`; the test preserves insertion order until it reaches the canonical encoder. |
| RESULT-PROJ-TP-009 | implemented | Stress | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-005 | 128 bounded emit/verify repetitions reproduce identical artifacts. | `repeated_emit_and_verify_is_stable_under_bounded_stress` | crates/edict-syntax/tests/result_projection.rs | Bounded deterministic stress, not a benchmark. |
| RESULT-PROJ-TP-010 | implemented | Schema fidelity | RESULT-PROJ-REQ-003 | The provider contract pack publishes the result-projection root and domain. | `result_projection_root_matches_reference_encoder` | docs/abi/edict-result-projection.cddl, fixtures/provider-contracts/source-functions-v1/edict-provider-contracts.cddl | Echo #698 can consume only this admitted representation. |
| RESULT-PROJ-TP-011 | implemented | Closure binding | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006 | Substituting the Target IR semantic-closure Core identity rejects before projection emission. | `mutated_target_core_closure_fails_closed` | crates/edict-syntax/tests/result_projection.rs | Matching result syntax cannot detach the projection from the exact compiler input. |
| RESULT-PROJ-TP-012 | implemented | Bound matrix | RESULT-PROJ-REQ-004, RESULT-PROJ-REQ-006 | One-over-limit path depth, UTF-8 text, artifact bytes, and structurally incompatible output types reject with stable kinds. | `path_text_artifact_and_structure_bounds_fail_closed` | crates/edict-syntax/tests/result_projection.rs | Complements the exact node-limit boundary. |
| RESULT-PROJ-TP-013 | implemented | Target emission | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-005 | Echo target lowering returns the compiler-produced projection and the independent verifier reconstructs that exact output. | `echo_target_lowering_emits_the_verified_result_projection` | crates/edict-syntax/tests/result_projection.rs | Prevents callers from reauthoring or dropping the projection between Core and the provider boundary. |
| RESULT-PROJ-TP-014 | implemented | Provider closure | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-003 | The application build binds the compiler-emitted canonical projection and digest into the provider semantic-input closure. | `compiler_result_projection_is_bound_into_the_provider_closure` | crates/edict-cli/src/application_build.rs | Echo #698 consumes this Edict-owned representation rather than inventing one inside the runtime. |
| RESULT-PROJ-TP-015 | implemented | Target refusal | RESULT-PROJ-REQ-006, RESULT-PROJ-REQ-007 | A result outside the closed projection language produces no projection claim and records the stable per-intent projection failure while preserving general Target IR. | `target_lowering_exposes_an_unsupported_result_projection_without_claiming_one` | crates/edict-syntax/tests/result_projection.rs | The application build treats any recorded projection failure as terminal; non-application Target IR consumers remain compatible. |
| RESULT-PROJ-TP-016 | implemented | Closure binding | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006 | Substituting a lawpack digest in the Target IR semantic closure rejects even when the source Core identity and result expression are unchanged. | `mutated_target_lawpack_closure_fails_closed` | crates/edict-syntax/tests/result_projection.rs | Independent verification reconstructs the complete Core-derived semantic closure, not only its source-Core member. |
| RESULT-PROJ-TP-017 | implemented | Schema parity | RESULT-PROJ-REQ-003, RESULT-PROJ-REQ-004, RESULT-PROJ-REQ-006 | The provider contract root accepts exactly 32 projection path segments and rejects the 33rd. | `result_projection_root_enforces_decoder_path_limit` | crates/edict-provider-schema/tests/provider_contract_pack.rs | Provider admission must not accept an artifact that the authoritative Rust decoder rejects. |
| RESULT-PROJ-TP-018 | implemented | Application cardinality | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-006 | The public application build rejects zero projections, multiple projections, and any projection-failure entry alongside an otherwise valid artifact. | `application_build_requires_one_projection_and_no_projection_failures` | crates/edict-cli/src/application_build.rs | The singleton provider route does not guess which compiler result contract to bind. |
| RESULT-PROJ-TP-019 | implemented | Hostile decoding | RESULT-PROJ-REQ-004, RESULT-PROJ-REQ-006 | Empty required text and a decoded zero output-byte ceiling fail with stable categories before semantic admission. | `hostile_decoded_values_fail_closed_before_semantic_admission` | crates/edict-syntax/tests/result_projection.rs | Covers hostile canonical values independently of the compiler emitter. |
| RESULT-PROJ-TP-020 | implemented | Source resolution | RESULT-PROJ-REQ-002, RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006 | Unknown capability steps plus missing or duplicate application-input bindings fail closed. | `unknown_steps_and_invalid_application_input_bindings_fail_closed` | crates/edict-syntax/tests/result_projection.rs | The verifier and emitter accept only the exact compiler-owned local/step closure. |
| RESULT-PROJ-TP-021 | implemented | Schema bounds | RESULT-PROJ-REQ-003, RESULT-PROJ-REQ-004, RESULT-PROJ-REQ-006 | The provider root rejects a zero output ceiling and a flat record whose root plus fields exceeds the 256-node decoder limit. | `result_projection_root_enforces_output_and_record_node_limits` | crates/edict-provider-schema/tests/provider_contract_pack.rs | Aggregate recursive node and encoded-byte limits remain authoritative decoder checks because CDDL cannot express those totals. |
| RESULT-PROJ-TP-022 | implemented | Parser recursion | RESULT-PROJ-REQ-004, RESULT-PROJ-REQ-006 | The projection parser accepts the exact expression-node recursion boundary and rejects the next recursive node before descending into it. | `expression_parser_refuses_recursion_beyond_the_node_budget` | crates/edict-syntax/src/result_projection.rs | The parser carries an explicit remaining-node budget independently of canonical-CBOR nesting limits. |
| RESULT-PROJ-TP-023 | implemented | Fail-closed control flow | RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006, RESULT-PROJ-REQ-008 | A Core effect nested under a branch inside a bounded loop cannot disappear when the supplied Target IR has no corresponding step. | `structured_core_effects_cannot_disappear_from_projection_validation` | crates/edict-syntax/tests/result_projection.rs | One witness exercises closure recursion through both loop and branch blocks; the projection seam must not accept semantics that general Target IR lowering refuses. |
| RESULT-PROJ-TP-024 | implemented | Pure binding source | RESULT-PROJ-REQ-001, RESULT-PROJ-REQ-002, RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-009 | Emission names the exact retained pure binding used by the authored Core result, and independent verification reconstructs the same local reference from matching Core and Target IR while missing, substituted, reordered, or duplicate binding authority rejects through both emission and verification. | `pure_binding_result_projection_round_trips_through_independent_verification`, `pure_binding_projection_rejects_missing_substituted_reordered_and_duplicate_target_authority` | crates/edict-syntax/tests/result_projection.rs | The projection cites compiler data; it does not contain a callback or re-authored expression evaluator. |
| RESULT-PROJ-TP-025 | implemented | Type interval compatibility | RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006, RESULT-PROJ-REQ-010 | An exact-byte application-input field projects into an exact-byte output, while changing only that source field to max-only rejects with `OutputShapeMismatch`. | `exact_byte_projection_refuses_a_max_only_source` | crates/edict-syntax/tests/result_projection.rs | Projection verification cannot upgrade an upper bound into proof of exact length. |
| RESULT-PROJ-TP-026 | implemented | Raw-Core integrity | RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006, RESULT-PROJ-REQ-011 | Mutating otherwise matching Core with an unused malformed type definition makes both public projection emission and independent verification reject with the stable Core-invalid/mismatch category before a projection artifact or verified witness exists. | `malformed_raw_core_cannot_emit_or_verify_a_result_projection` | crates/edict-syntax/tests/result_projection.rs | Projection authority cannot bypass the shared Core border merely because the malformed definition is not result-reachable. |
| RESULT-PROJ-TP-027 | implemented | Expanded-depth refusal | RESULT-PROJ-REQ-005, RESULT-PROJ-REQ-006, RESULT-PROJ-REQ-011, RESULT-PROJ-REQ-012 | Mutating matching Core with a shallow-cached named definition used one level beyond the semantic depth limit makes both public emission and independent verification reject before a projection artifact or witness exists. | `over_depth_named_expansion_cannot_emit_or_verify_a_result_projection` | crates/edict-syntax/tests/result_projection.rs | Projection consumers share the Core expansion-height judgment rather than rechecking a convenient subset. |

## Determinism Obligations

- Projection derivation performs no I/O and consumes only supplied Core and
  Target IR values.
- Record fields are canonicalized by key.
- Step references use Target IR step identities, not source variable names.
- Verification never trusts a claimed digest or a host-authored result.
- Exact replay of the same Core and Target IR produces identical bytes and
  identity.
- The lowerer and verifier receive byte-identical projection inputs bound to
  the compiler-authored identity.

## Non-Goals

- Runtime projection evaluation.
- Echo package or verification-report changes.
- General expression evaluation.
- External effects or model execution.
- Application-specific callbacks.
