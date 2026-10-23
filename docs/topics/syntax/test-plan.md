# Syntax Test Plan

Status: current verification design for the Phase 1 syntax front end.

This plan is the verification side of the syntax topic shelf. It records how the
current contract is proven, what fixtures are authoritative, and which gaps must
be closed before later phases claim stronger coverage.

## Scope

In scope:

- lexical tokenization exposed through `amblicit_syntax::lex`;
- module parsing exposed through `amblicit_syntax::parse_module`;
- source AST shape for the minimal-v1 syntax subset;
- stable parser error identities through `ParseErrorKind`;
- deterministic contract metadata linking this plan to tests and fixtures.

Out of scope:

- semantic validation beyond parser-owned syntax shape;
- compiler-spine lowering and canonical encodings;
- type resolution across imports;
- target/lawpack ABI validation.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| SYNTAX-REQ-017 | implemented | Source-module imports preserve an exact package, mandatory digest and lexical alias. Standalone compilation refuses unresolved source modules, including unused imports. | docs/SPEC_edict-language-v1.md |

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SYNTAX-TP-027 | implemented | Source-module import boundary | SYNTAX-REQ-017 | Exact package/digest/alias and full import span survive parsing; missing digest fails with ExpectedKeyword; a standalone compiler cannot discard an unresolved dependency. | source_module_import_highlights_the_contextual_keyword, source_module_import_preserves_exact_package_digest_and_alias, source_module_import_requires_a_digest_clause, unresolved_source_import_cannot_silently_compile_even_when_unused | crates/amblicit-syntax/tests/source_modules.rs | Closure authentication and linking belong to the compiler-spine plan. |

Byte-literal verification for #249 is recorded below. Source-to-Core literal
and bound checks belong to the compiler-spine plan.

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| SYNTAX-REQ-016 | implemented | Parse ASCII-spelled byte literals as one token with raw octets; support quote, backslash, newline, tab, carriage-return, NUL and two-digit hexadecimal escapes; reject malformed escapes and non-ASCII byte source; preserve Unicode strings as valid UTF-8. | issue #249 |

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SYNTAX-TP-026 | implemented | Byte literals and escapes | SYNTAX-REQ-016 | Complete prefix span, literal binary octets, lexical refusals and Unicode string preservation are checked independently. | bytes_prefix_is_one_token_with_the_complete_source_span, source_bytes_literals_lower_to_exact_raw_octets, invalid_byte_spellings_fail_at_the_lexical_boundary | crates/amblicit-syntax/tests/bytes_literals.rs | Inline literal vectors include empty bytes, CR/LF/TAB/NUL, 0x80 and 0xFF. |

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| SYNTAX-REQ-001 | implemented | `parse_module` returns source AST only; later phases own semantic validation and lowering. | crates/amblicit-syntax/src/lib.rs |
| SYNTAX-REQ-002 | implemented | Module/package/import coordinates parse and preserve source-significant version spelling. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-003 | implemented | Imports parse supported kinds, including digest-bound external-operation `capability` references, and validate digest literals. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-004 | implemented | Type declarations parse Phase 1 type surface and reject empty enum declarations. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-005 | implemented | Integer suffixes remain source-significant in expression literals and static bounds. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-006 | implemented | Intent declaration syntax parses parameters, return type, clauses, and blocks; semantic requiredness is deferred. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-007 | implemented | Statement syntax includes external-action requests, effect call positions, guards, control flow, and bounded loops. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-008 | implemented | Expression syntax includes precedence, records, literals, conditionals, variants, and match. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-009 | implemented | Reserved keywords reject in bare-name positions while remaining legal after `.`. | docs/SPEC_edict-language-v1.md |
| SYNTAX-REQ-010 | implemented | Negative tests assert stable error kinds, not diagnostic prose or incidental output. | crates/amblicit-syntax/src/parser.rs |
| SYNTAX-REQ-011 | implemented | Source/surface validation rejects context-free semantic errors while deferring resolution, contextual typing, bound proof, and target/lawpack-dependent checks. | issue #10 |
| SYNTAX-REQ-012 | planned | Full source lowering emits canonical Core IR with byte-stable golden artifacts. | issue #21, issue #22 |
| SYNTAX-REQ-013 | implemented | Obstruction-strand source spelling has first-class parser syntax and does not parse as an ordinary obstruction-target function call. | issue #118, docs/topics/obstruction-strands/test-plan.md |
| SYNTAX-REQ-014 | implemented | Byte refinements distinguish maximum length from exact length; `Bytes<exact=N>` preserves one structural exact bound in the AST without adding a byte canonicalization policy. | issue #200, docs/SPEC_edict-language-v1.md |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| fixtures/lang/bounds/bounded-hello.amblicit | Positive bounded scalar and record fixture. | Parsed AST structural equality in `bounded_hello_parses`. |
| fixtures/lang/effects/read-greeting.amblicit | Positive imports, effect calls, obstruction map, and record result fixture. | Parsed AST structural equality in `read_greeting_parses`. |
| fixtures/lang/effects/conditional-blob.amblicit | Positive branch-yield conditional effect fixture. | Parsed AST structural equality in `conditional_blob_fixture_parses`. |
| fixtures/lang/types/color-match.amblicit | Positive enum, variant, variant literal, and match fixture. | Parsed AST structural equality in `palette_fixture_parses`. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SYNTAX-TP-001 | implemented | Golden path | SYNTAX-REQ-001 | Public API returns a `Module` with expected package/declaration shape. | bounded_hello_parses | fixtures/lang/bounds/bounded-hello.amblicit | Source AST, not Core IR. |
| SYNTAX-TP-002 | implemented | Golden path | SYNTAX-REQ-002 | Exact version strings match source spelling. | multi_part_package_version, package_versions_preserve_underscores, import_versions_preserve_underscore_labels | - | Covers `_beta` labels. |
| SYNTAX-TP-003 | implemented | Import boundary | SYNTAX-REQ-003 | Invalid digests reject with a stable error kind, while a digest-bound capability import preserves its distinct AST kind and digest. | import_digest_literals_are_validated, capability_imports_parse_as_external_operation_references | - | Capability import is request authority, not performance authority. |
| SYNTAX-TP-004 | implemented | Golden path | SYNTAX-REQ-004, SYNTAX-REQ-014 | Type AST nodes match expected records, maximum and exact bytes, variants, enums, and bounds. | bounded_hello_parses, bytes_accept_coordinate_bounds, bytes_accept_exact_bounds, enum_decl_parses, variant_type_with_and_without_payloads_parses | fixtures/lang/bounds/bounded-hello.amblicit | Structural AST equality. |
| SYNTAX-TP-005 | implemented | Error handling | SYNTAX-REQ-004 | Empty enums reject with `ParseErrorKind::EmptyEnum`. | empty_enum_and_empty_obstruction_maps_reject | - | Parser-level syntactic emptiness. |
| SYNTAX-TP-006 | implemented | Edge case | SYNTAX-REQ-005 | Integer suffix is preserved in `Expr::Int` and `BoundRef::Int`. | typed_integer_suffix, bound_integer_suffixes_are_preserved | - | Source-significant suffix oracle. |
| SYNTAX-TP-007 | implemented | Golden path | SYNTAX-REQ-006 | Intent clauses and bodies parse into expected AST. | bounded_hello_parses, read_greeting_parses, require_statement_parses_terminal_obstruction_source_shape | fixtures/lang/bounds/bounded-hello.amblicit, fixtures/lang/effects/read-greeting.amblicit | Requiredness and `require` lowering semantics are deferred. |
| SYNTAX-TP-008 | implemented | Golden path | SYNTAX-REQ-007 | Bounded loops parse literal and coordinate bounds. | for_with_integer_bound_parses, for_with_coordinate_bound_parses | - | Loop semantic proof deferred. |
| SYNTAX-TP-009 | implemented | Known failure | SYNTAX-REQ-007 | Missing `bounded` rejects. | for_without_bounded_is_rejected | - | Syntactic mandatory bound. |
| SYNTAX-TP-010 | implemented | Error handling | SYNTAX-REQ-007 | Effect positions reject non-call expressions. | effect_positions_must_be_calls | - | Stable `NonCallEffect` kind. |
| SYNTAX-TP-011 | implemented | Golden path | SYNTAX-REQ-008 | Conditional expression and branch-yield AST nodes match expected structure. | pure_ternary_parses_as_let_value, ternary_is_usable_in_nested_expression_position, branch_yield_parses_only_as_let_rhs, conditional_blob_fixture_parses | fixtures/lang/effects/conditional-blob.amblicit | Branch-yield only in `let` RHS. |
| SYNTAX-TP-012 | implemented | Known failure | SYNTAX-REQ-008 | Branch-yield without `yield` rejects. | branch_yield_without_yield_is_rejected | - | Stable parser rejection. |
| SYNTAX-TP-013 | implemented | Golden path | SYNTAX-REQ-008 | Record shorthand, explicit fields, and spread parse. | bounded_hello_parses, read_greeting_parses | fixtures/lang/bounds/bounded-hello.amblicit, fixtures/lang/effects/read-greeting.amblicit | Shorthand semantics checked as AST entry. |
| SYNTAX-TP-014 | implemented | Golden path | SYNTAX-REQ-008 | Variants and match parse with payload binders and empty match rejects. | variant_literal_with_payload_parses, variant_literal_without_payload_parses, match_expr_with_binders_parses, empty_match_is_rejected, palette_fixture_parses | fixtures/lang/types/color-match.amblicit | Empty match has stable error kind. |
| SYNTAX-TP-015 | implemented | Edge case | SYNTAX-REQ-008 | Type-call parsing requires adjacency and does not swallow comparisons. | generic_call_vs_comparison_disambiguation, type_call_suffix_requires_adjacency_to_call_paren | - | Oracle is AST operator/call shape. |
| SYNTAX-TP-016 | implemented | Golden path | SYNTAX-REQ-008 | Boolean and digest value literals parse as dedicated AST nodes. | bool_and_digest_literals_are_real_literals | - | Digest oracle is fixed string payload. |
| SYNTAX-TP-017 | implemented | Error handling | SYNTAX-REQ-009 | Bare reserved keywords reject in value and binder positions. | keywords_are_rejected_as_bare_values, keywords_are_rejected_as_let_binder_names, reserved_words_are_rejected_in_all_binder_positions | - | Includes booleans as unusable binder names. |
| SYNTAX-TP-018 | implemented | Error handling | SYNTAX-REQ-009 | Import aliases, coordinate roots, and record shorthand reject reserved keywords. | reserved_keywords_are_rejected_as_import_aliases, reserved_words_are_rejected_as_coordinate_roots, reserved_words_are_rejected_as_record_shorthand | - | Dotted members remain legal. |
| SYNTAX-TP-019 | implemented | Golden path | SYNTAX-REQ-009 | Keywords after `.` and prelude constructors remain legal. | keywords_are_legal_as_member_names, prelude_constructors_are_not_reserved | - | Contextual keyword oracle. |
| SYNTAX-TP-020 | implemented | Error handling | SYNTAX-REQ-010 | Negative cases assert stable `ParseErrorKind` identities. | reserved_future_decls_are_rejected_at_top_level, missing_package_is_rejected, bytes_rejects_canonical_policy, unterminated_string_is_rejected | - | No stdout/stderr scraping. |
| SYNTAX-TP-021 | implemented | Semantic validation | SYNTAX-REQ-011 | The source/surface validator returns stable diagnostic kinds for context-free source errors and accepts unresolved downstream facts. | validate_module_remains_surface_stage_compatibility_alias, surface_validation_defers_import_and_name_resolution, surface_validation_defers_contextual_typing_and_loop_bound_proof, surface_validation_defers_obstruction_exhaustiveness | - | Owned by the semantic-validation shelf. |
| SYNTAX-TP-022 | planned | Golden artifact | SYNTAX-REQ-012 | Full source lowering emits byte-stable canonical artifacts. | - | - | Initial Core golden artifacts exist in the Core IR shelf; full source-language coverage remains planned. |
| SYNTAX-TP-023 | implemented | Syntax guard | SYNTAX-REQ-013 | `else continue obstructed { reason: ... }` parses as a distinct require-else arm, while `else continueInObstructedStrand(...)` remains a terminal obstruction target. | continue_obstructed_source_arm_parses, helper_shaped_continue_in_obstructed_strand_is_terminal, stale_basis_obstruction_strand_fixture_parses | crates/amblicit-syntax/tests/parse_review_regressions.rs, fixtures/obstruction-strands/v0/stale-basis/source.amblicit | Prevents hidden control-flow semantics in ordinary obstruction constructors. |
| SYNTAX-TP-024 | implemented | Golden path | SYNTAX-REQ-003, SYNTAX-REQ-007 | A request statement parses its typed binding, single operation call, schemas, authority scope, basis, two budgets, and reconciliation resource before source-to-Core compilation. A non-call request operation rejects with `NonCallExternalActionOperation`, independently of semantic-effect call diagnostics. | workspace_observation_request_compiles_as_non_callable_data, non_call_request_operation_has_a_request_specific_parse_kind | crates/amblicit-syntax/tests/external_action_requests.rs | Execution and settlement remain outside the parser. |

## Determinism Obligations

- Tests use checked-in source strings and fixtures only.
- Negative tests assert structured `ParseErrorKind` values.
- Parser tests inspect returned AST state; they do not inspect stdout, stderr,
  logs, or human-readable diagnostic prose.
- Fixture paths are repository-relative and checked by the contract checker.
- Topic metadata is checked by `cargo xtask contract-check` and by the `xtask`
  test target in `cargo test --workspace`.

## Fuzz And Stress Plan

No fuzz target exists yet. The first useful fuzz target should generate bounded
token streams for the lexer/parser and minimize failures into permanent
deterministic regression cases in `crates/amblicit-syntax/tests/`.

## Open Gaps

- `SYNTAX-REQ-011`: semantic validation is implemented by the
  [semantic-validation](../semantic-validation/) shelf.
- `SYNTAX-REQ-012`: initial in-memory Core lowering is implemented by the
  [compiler-spine](../compiler-spine/) shelf, and initial canonical Core golden
  artifacts are implemented by the [core-ir](../core-ir/) shelf. Full
  source-language Core artifact coverage remains planned.
- `SYNTAX-REQ-013`: first-class obstruction-strand source syntax is implemented
  by the [obstruction-strands](../obstruction-strands/) shelf. Core lowering
  for the currently lowerable `require` subset is implemented by the
  compiler-spine shelf; Target IR and runtime obstruction behavior remain
  deferred.
- Fixture coverage is not exhaustive across every `EDICT-LANG-*` row in
  `docs/REQUIREMENTS.md`; this shelf covers the landed Phase 1 syntax parser
  only.

## Source functions

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| SYNTAX-REQ-015 | implemented | Parse typed nongeneric first-order function declarations with ordered statement bodies; preserve spans for definition and call diagnostics. | issue #226 |

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SYNTAX-TP-025 | implemented | Source functions | SYNTAX-REQ-015 | Identity and Jim byte-assembly declarations reach actual compiler checking; declaration-only acceptance is insufficient. | source_function_identity_is_not_an_imported_fact, jim_range_assembly_calls_an_authored_function_through_core_and_target | fixtures/lang/functions/range-assembly.amblicit | crates/amblicit-syntax/tests/source_functions.rs; Guarded source-function and regression suites pass. |
