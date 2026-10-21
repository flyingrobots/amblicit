---
title: "REQUIREMENTS - Edict Fixture Constitution"
legend: "SPEC|TRANSMUTE|PLATFORM"
lane: "design"
packet: "0021-continuum-yolo-runtime-neutral-edict-sha-lock-assurance"
issue: "https://github.com/flyingrobots/wesley/issues/611"
status: "draft"
owners:
  - "@flyingrobots"
created: "2026-06-18"
updated: "2026-06-18"
---

<!-- markdownlint-disable MD025 MD013 -->

# REQUIREMENTS - Edict Fixture Constitution

<!-- markdownlint-enable MD025 -->

## Purpose

Every normative requirement gets a stable ID. Every ID gets a positive fixture,
a negative fixture, and (where it has hash impact) a golden artifact.

> A requirement without a fixture is advisory.
> A fixture without a requirement is folklore.

This registry is the join table between the prose specs and the
[`fixtures/`](../fixtures/) corpus (Phase 0 deliverable). It is also the input to
[`spec.lock.json`](../spec.lock.json): the documentation build fails when an
embedded snippet, schema, or fixture no longer matches the locked digests.

## ID Scheme

`<DOMAIN>-<TOPIC>-<NNN>`:

- `EDICT-LANG-*` — Edict Language v1 (syntax, types, evaluation).
- `EDICT-CORE-*` — Core IR identity and canonicalization.
- `EDICT-OPTIC-*` — optic contract and Observer Geometry preservation.
- `EDICT-TARGET-*` — Target Profile ABI.
- `EDICT-LAWPACK-*` — Lawpack ABI.
- `EDICT-ABI-*` — cross-ABI rules (no-duplication, display sidecars).
- `EDICT-ADMISSION-*` — Edict-owned admission-boundary artifact and operation
  semantics.
- `EDICT-CLI-*` — public CLI and application-build contracts.
- `EDICT-CONFORMANCE-*` — conformance/differential testing.
- `CONTINUUM-*` — contract bundle and admission.

`status`: `spec` (prose landed), `fixture` (fixture exists), `golden` (golden
artifact locked), `impl` (implementation passes), `deferred` (requirement stated
but owned by a follow-up issue; no fixtures until its dependency lands).

## Registry

| ID | Requirement | Owner spec | Positive fixture | Negative fixture | Status |
| --- | --- | --- | --- | --- | --- |
| EDICT-LANG-BOUNDS-001 | Naked unbounded `String`/`Bytes` rejected in checked lane; refined scalars bound every position | Language | `lang/bounds/bounded-hello` | `lang/bounds/naked-string` | spec |
| EDICT-LANG-LEN-001 | `len` units pinned (Unicode scalars vs bytes); canonicalize before measure; concat bound derivation | Language | `lang/len/unicode-vs-bytes` | `lang/len/concat-overflow` | spec |
| EDICT-LANG-BYTE-SLICE-001 | Half-open raw-byte slicing preserves the operand maximum and requires exact U64 range proof, independently validated by source compilation and Target lowering without runtime execution claims | Language/Core/Target | `byte_slice_preserves_operand_identity_and_weakens_length_bound`; `byte_slice_accepts_full_and_empty_ranges_without_assumptions` | `byte_slice_rejects_unproven_ranges`; `target_rejects_forged_byte_slice_signatures_and_proofs` | impl |
| EDICT-LANG-BYTE-UTF8-001 | Bounded byte UTF-8 predicate preserves exact operand identity and Bool result; compiler/Target signature, authority and scan-cost bounds are checked without runtime execution claims | Language/Core/Target | `byte_utf8_call_retains_operand_identity_and_boolean_projection`; `byte_utf8_source_function_budget_includes_bounded_scan` | `byte_utf8_rejects_wrong_operands_signatures_and_result_type`; `byte_utf8_target_refuses_forged_signature_or_operand_authority` | impl |
| EDICT-LANG-BYTE-COUNT-001 | Bounded single-byte count takes an exactly-one-byte literal needle, preserves operand identity and the counted byte, returns U64 bounded by the operand maximum for addition proofs, charges one complete scan, and is independently validated by Target without runtime execution claims | Language/Core/Target | `count_retains_operand_identity_needle_byte_and_u64_result`; `the_count_bound_proves_jims_line_count_addition`; `count_helper_scan_is_charged_once` | `count_rejects_wrong_operands_needles_signatures_and_result_type`; `count_target_refuses_forged_signature_needle_or_operand_authority`; `the_count_bound_is_the_operand_maximum_not_zero`; `target_independently_refuses_a_forged_overflowing_count_sum` | impl |
| EDICT-LANG-BYTE-CONCAT-001 | Ordered bounded raw-byte concatenation derives a checked conservative sum of maxima and independently validates exact operands in Target | Language/Core/Target | `bounded_byte_concat_preserves_order_coordinates_and_summed_maximum`; `byte_concat_order_and_nesting_are_explicit_in_core_identity` | `byte_concat_rejects_overflowing_static_sum_without_wrapping`; `target_rejects_forged_byte_concat_calls`; `target_rejects_byte_concat_overflow_and_nominal_unwrapping` | impl |
| EDICT-LANG-LIST-LIT-001 | Bounded list literals are typed against an expected `List<T, max=N>`, preserve order and duplicates, reject excess cardinality, wrong items and missing context, and are independently validated by Target | Language/Core/Target | `list_literal_preserves_order_and_lowers_to_core_list`; `list_literal_accepts_empty_partial_and_full_cardinality`; `list_literal_elements_are_checked_against_the_item_type`; `list_literals_flow_into_records_returns_and_loops`; `source_functions_build_list_literals` | `list_literal_rejects_excess_cardinality_wrong_items_and_missing_context`; `authenticated_effects_are_rejected_inside_list_literals`; `target_refuses_forged_list_literals` | impl |
| EDICT-LANG-INTLIT-001 | Integer width hash-significant; literal needs suffix or unambiguous type; bare ambiguous rejects | Language | `lang/intlit/suffix` | `lang/intlit/ambiguous-hash-arg` | spec |
| EDICT-LANG-WHERE-001 | `where` is input refinement → `EDICT-INPUT-CONSTRAINT`, not runtime precondition | Language | `lang/where/input-refine` | `lang/where/touches-runtime-state` | spec |
| EDICT-LANG-TOTAL-CHECK-001 | Runtime `require`/`guarantee` must carry `else`; faults are typed, never host exceptions | Language | `lang/require/with-else` | `lang/require/missing-else` | spec |
| EDICT-LANG-BOOL-001 | Boolean `and`/`or` operators short-circuit | Language | `lang/bool/short-circuit` | `lang/bool/effect-in-operand` | spec |
| EDICT-LANG-LOOP-001 | `for ... bounded N`: List only, list order, prove M<=N, never silently truncate | Language | `lang/loop/bounded-list` | `lang/loop/unprovable-bound` | spec |
| EDICT-LANG-OBSTRUCT-EXHAUST-001 | Obstruction mapping is exhaustive over domain-mappable classes, with typed payloads | Language | `lang/obstruct/exhaustive-payload` | `lang/obstruct/missing-class` | spec |
| EDICT-LANG-NOSHADOW-001 | Locals must not shadow import/package/type/prelude names | Language | `lang/shadow/distinct-names` | `lang/shadow/greeting-collision` | spec |
| EDICT-LANG-ENUMVARIANT-001 | Enum case `Type.CASE` vs variant constructor `Type::Case(payload)` | Language | `lang/enumvariant/both` | `lang/enumvariant/mixed-syntax` | spec |
| EDICT-LANG-FOOTPRINT-COST-001 | Footprint and cost are checked separately | Language | `lang/footprint/separate` | `lang/footprint/conflated` | spec |
| EDICT-LANG-BUDGET-SPLIT-001 | Core/target/admitted budget split; target dims not in Core | Language | `lang/budget/split` | `lang/budget/target-dim-in-core` | spec |
| EDICT-LANG-PROFILE-001 | `edict.language/v1` vs `edict.implementation/minimal-v1` capability flags | Language | `lang/profile/minimal` | `lang/profile/undeclared-capability` | spec |
| EDICT-LANG-CAPREF-001 | `CapabilityRef<T>` carries receipt digest only; inert until admitted | Language | `lang/capref/inert` | `lang/capref/ambient-authority` | spec |
| EDICT-LANG-EXTERNAL-REQUEST-001 | External boundary crossings are typed deterministic request values; exact operation capability closure is required and no ambient world authority enters Edict or the provider seam | Language/Core/Target | `fixtures/lang/external-actions/workspace-snapshot.edict`; `fixtures/core/canonical/workspace-snapshot.core.cbor`; `fixtures/core/canonical/workspace-snapshot.core.sha256`; `fixtures/target-ir/canonical/workspace-snapshot.target-ir.cbor`; `fixtures/target-ir/canonical/workspace-snapshot.target-ir.sha256`; `workspace_observation_request_compiles_as_non_callable_data` | `undeclared_or_floating_operation_families_fail_closed`; `ambient_operation_families_are_not_requestable`; `request_resource_coordinates_must_be_nonempty`; `duplicate_target_request_ids_reject_before_identity` | impl |
| EDICT-CLI-EXTERNAL-REQUEST-BUILD-001 | An explicit application build publishes exact compiler-owned Core and Target IR for a request-only capability closure without performing the action or granting callable target authority | CLI/Lawpack/Core/Target | `public_external_action_build_emits_exact_compiler_artifacts`; `request_only_profile_supplies_budget_without_callable_effect_authority`; `fixtures/lawpack/workspace-snapshot/` | `public_external_action_build_rejects_capability_substitution`; `external_action_build_requires_a_typed_request`; `external_action_build_rejects_mixed_callable_execution`; `failed_external_action_pair_publication_preserves_previous_core` | impl |
| EDICT-LAWPACK-AUTHORING-001 | Applications can submit one typed lawpack review document to Edict, which owns canonical construction, derived local identities, exact dependency corroboration, public-decoder validation, deterministic write/check output, and failure-atomic owned-tree replacement without provider or runtime participation | CLI/Lawpack | `minimal_authoring_emits_a_deterministic_valid_bundle`; `full_surface_round_trips_through_existing_decoders`; `external_application_authors_vendors_and_builds_its_own_lawpack` | `malformed_inputs_fail_with_stable_categories`; `exact_dependency_closure_is_required_and_corroborated`; `injected_pre_activation_failure_restores_the_previous_tree` | impl |
| EDICT-LANG-READONLY-001 | Read-only inferred; executionClass (proofOnly/runtime) orthogonal to writeClass; runtime read is allowed | Language/Lawpack | `lang/readonly/runtime-read` | `lang/readonly/hidden-append` | spec |
| EDICT-OPTIC-SOURCE-001 | Each optic field has one deterministic source (basis clause / profile template / coordinate / footprint) | Language | `optic/source/basis-clause` | `optic/source/freeform-support` | spec |
| EDICT-DIGEST-WIRE-001 | Canonical digest is typed `[algorithm, bytes]`, never a hex string; hex only in review JSON | Bundle/ABI | `abi/digest/typed-pair` | `abi/digest/hex-string` | spec |
| EDICT-ABI-AUTHORITY-FACTS-001 | `edict.authority-facts/v1` uses an Edict-owned CDDL root and canonical-CBOR value shape with coordinate-keyed fact maps, typed source digest bytes, normalized write-class sets, stable decode failures, and compatibility with the existing validated compiler-context path | Authority Facts | `fixtures/authority-facts/canonical/example-effectful.authority-facts.cbor`; `crates/edict-syntax/tests/authority_facts_cbor.rs` | `crates/edict-syntax/tests/authority_facts_cbor.rs`; `crates/edict-provider-schema/tests/authority_facts_abi.rs` | impl |
| EDICT-ABI-TARGET-PROFILE-RESOURCES-001 | Edict publishes exact canonical resources for the five Edict-owned target-profile slots; only a complete explicit set matching coordinate, canonical bytes, domain-framed digest, and repository-source provenance can bind those slots without discovery or provider component selection | Target Profile | `fixtures/target-profile/contract-resources/README.md`; `crates/edict-syntax/tests/target_profile_contract_resources.rs` | `crates/edict-syntax/tests/target_profile_contract_resources.rs` | impl |
| EDICT-ABI-PROVIDER-WIT-001 | External provider components use the distinct versioned target-provider WIT package with explicit protocol versions, digest-bound opaque inputs, authority-separated role-tagged digest-free outputs, optional logical paths, typed refusal, deterministic response bounds, and no callable ambient imports | Target Provider | `docs/abi/edict-target-provider.wit` | `fixtures/providers/wit/legacy-target-profile.wit` | impl |
| EDICT-ABI-PROVIDER-INVOCATION-001 | Provider invocation validation requires protocol `1.0.0`, host-authored input bindings, an explicitly injected owning-schema validator whose host contract is deterministic and side-effect-free, canonical domain-framed digests computed only after schema acceptance, exact role/output/path/diagnostic/limit contracts, limit-independent complete results, and sealed all-or-nothing host output manifests without component execution or ambient I/O in the generic validator | Target Provider | `crates/edict-syntax/tests/provider_invocation.rs`; `fixtures/core/canonical/bounded-hello.core.cbor`; `fixtures/target-ir/canonical/echo-effectful.target-ir.cbor` | `crates/edict-syntax/tests/provider_invocation.rs` | impl |
| EDICT-ABI-PROVIDER-AUTHORITY-001 | Hostable provider manifests bind the exact provider ABI, component identities, generated schema provenance, and every artifact domain to one digest-locked schema and root; the immutable complete CDDL registry verifies explicit schema bytes and real canonical instances before invocation | Target Provider | `fixtures/providers/echo-generated/provider-manifest.json`; `crates/edict-provider-schema/tests/registry.rs` | `crates/edict-syntax/tests/provider.rs`; `crates/edict-provider-schema/tests/registry.rs` | impl |
| EDICT-ABI-PROVIDER-HOST-001 | The provider host verifies exact component digest and contract identity, supplies no callable ambient or WASI capability, enforces explicit work/resource/transport limits with stable host failures, invokes typed lowerer and verifier exports, and exposes only responses admitted by the pure validator | Target Provider | `fixtures/providers/components/lowerer.component.wasm`; `fixtures/providers/components/verifier.component.wasm`; `crates/edict-provider-host-wasmtime/tests/invocation.rs` | `crates/edict-provider-host-wasmtime/tests/preflight.rs`; `fixtures/providers/components/malformed-lowerer.component.wasm`; `fixtures/providers/components/instantiation-failure-lowerer.component.wasm` | impl |
| EDICT-ABI-PROVIDER-REPLAY-001 | Provider replay executes identical authority and limits through distinct fresh stores; exact sealed outcomes or stable host-failure identities must match, while mismatches, ambient capability attempts, guest failures, concurrent calls, and cross-provider recovery remain structured and isolated | Target Provider | `crates/edict-provider-host-wasmtime/tests/invocation.rs`; `fixtures/target-ir/canonical/echo-effectful.target-ir.cbor` | `crates/edict-provider-host-wasmtime/src/replay.rs`; `crates/edict-provider-host-wasmtime/tests/preflight.rs` | impl |
| EDICT-ABI-PROVIDER-CONTRACT-PACK-001 | Edict generates the deterministic Rust-neutral source-functions-v1 provider contract pack while retaining v1 as exact frozen CDDL and manifest bytes; the current self-contained CDDL, contract/domain roots, Apache-2.0 license, and canonical target-profile resource bytes, digests, and provenance are bound by a checked manifest and validated without discovery | Target Provider | `fixtures/provider-contracts/source-functions-v1/edict-provider-contracts.cddl`; `fixtures/provider-contracts/v1/edict-provider-contracts.cddl`; `fixtures/provider-contracts/v1/manifest.json` | `crates/edict-provider-schema/tests/provider_contract_pack.rs` (`source_function_contract_preserves_old_modules_and_requires_new_schema_selection`); `xtask/src/tests.rs` | impl |
| EDICT-ABI-FAILURE-NAMED-001 | Effects declare named, typed low-level failures; obstruction map keyed by failure coordinate | Target/Lawpack | `abi/failure/named-payload` | `abi/failure/undeclared-arm` | spec |
| CONTINUUM-BUNDLE-SUBJECT-001 | Requests/receipts/explanations carry `bundleSubject {kind,digest}`; receipt echoes it; Moriarty tracks both | Bundle/Admission | `bundle/subject/echo` | `bundle/subject/mismatch` | spec |
| EDICT-ADMISSION-BOUNDARY-001 | Edict owns bundle subject, digest selection, operation coordinates, basis, variables, requirements, and failure taxonomy; Continuum owns participant policy and authority | Admission | `admission/boundary/edict-owned-fields` | `admission/boundary/participant-policy-in-edict` | impl |
| EDICT-ADMISSION-OPREQ-001 | Operation requirements bind bundle subject, operation coordinate, basis, canonical variables digest, and instantiated requirements digest | Admission | `admission/opreq/bound` | `admission/opreq/floating-subject` | impl |
| EDICT-ADMISSION-HIDDEN-INPUT-001 | Hidden prompt, DOM, filesystem, network, host callback, or scheduler state cannot influence execution unless materialized as canonical input, witnessed evidence, admitted basis, or capability presentation | Admission | `admission/input/canonical` | `admission/input/hidden-host` | impl |
| EDICT-ADMISSION-AUTHORITY-001 | Authorship provenance and registration evidence do not grant invocation authority; invocation requires accepted admission plus matching invocation capability evidence | Admission | `admission/authority/invocation-capability` | `admission/authority/registration-only` | impl |
| EDICT-TARGET-NEUTRAL-LOWERING-001 | Lowerer compares cost/footprint vs declared target ceiling, never an admitted participant budget | Target | `target/lowering/declared-ceiling` | `target/lowering/admitted-budget` | spec |
| EDICT-CORE-GUARD-PAYLOAD-001 | CoreGuard/ObstructionMap carry a typed obstruction payload construct (coordinate + binder + payload expr) | Language | `core/guard/payload-roundtrip` | `core/guard/coordinate-only` | spec |
| EDICT-CORE-WHERE-HASH-001 | Core carries typed `where` predicate trees in `inputConstraints`, not a validator coordinate | Language | `core/where/predicate-tree` | `core/where/coordinate-only` | spec |
| EDICT-ABI-LAWPACK-ADAPTER-001 | `acceptedLawpackAdapterAbi` is absent/empty or exactly `["edict.lawpack-adapter/v1"]`; unknown and duplicate ABI claims reject | Target | `abi/adapter/direct-v1` | `abi/adapter/unknown-or-duplicate` | impl |
| CONTINUUM-SEMANTIC-OPTIONS-001 | Only semantic compile options enter the semantic digest; diagnostic options excluded | Bundle | `bundle/options/semantic-only` | `bundle/options/diagnostic-in-semantic` | spec |
| EDICT-LANG-ENCODEMAX-001 | `CanonicalEncodedMax<T>` is the compiler-derived max canonical-CBOR size of T, composed structurally; rejected for unbounded T | Language | `lang/encodemax/bounded` | `lang/encodemax/unbounded-type` | spec |
| EDICT-LANG-BUDGET-UNITS-001 | Core budget units pinned (steps/peak live bytes/output octets); targetBudget = costAlgebra+resolved ceiling | Language | `lang/budget/units` | `lang/budget/undefined-units` | spec |
| EDICT-LANG-INTLIT-002 | Suffix disagreeing with contextual integer type rejects; propagation contexts enumerated | Language | `lang/intlit/context-propagate` | `lang/intlit/suffix-context-mismatch` | spec |
| EDICT-LANG-PRELUDE-001 | Minimal-v1 prelude op set is closed; unlisted prelude ops do not exist | Language | `lang/prelude/closed` | `lang/prelude/unlisted-op` | spec |
| EDICT-LANG-BASIS-PURE-001 | `basis` is pure/effect-free over inputs, constants, capability refs, pure fns; runtime reads validate not define | Language | `lang/basis/pure` | `lang/basis/runtime-read` | spec |
| EDICT-LANG-HELPER-BOUNDS-001 | Pure-helper bounds enforced at import validation + call-site instantiation | Language/Lawpack | `lang/helper/bounded` | `lang/helper/unbounded` | spec |
| EDICT-LANG-REQUIRE-ELSE-001 | Every `require` carries `else`; input-only require allowed for domain obstructions; `where`=input refinement; `assert`=proof | Language | `lang/require/input-only-else` | `lang/require/no-else` | spec |
| EDICT-CORE-EXPR-CDDL-001 | Core expression/predicate CDDL and canonical encoding are normative before reviewed Core goldens are accepted | Language/Core | `core/expr/cddl-and-goldens` | `core/expr/golden-before-cddl` | impl |
| EDICT-LANG-OPTION-REFINE-001 | Only Option refinement (isSome/unwrap), lexical and variable-specific; no general narrowing | Language | `lang/refine/isSome` | `lang/refine/flows-through-helper` | spec |
| EDICT-LANG-BOUND-VIOLATION-001 | Runtime value violating a proven static bound is integrity/internal fault, never silent truncation or resourceFault | Language | `lang/bound/integrity-fault` | `lang/bound/silent-truncate` | spec |
| EDICT-CORE-GUARD-CATEGORY-001 | CoreGuard is targetAtomic + always carries obstruction; verifier proofs are CoreProofObligation nodes | Language | `core/guard/atomic-obstruction` | `core/guard/obstructionless` | spec |
| EDICT-CORE-PREIMAGE-LIST-001 | Canonicalization rules carry a positive exhaustive Core-intent preimage inclusion list | Language | `core/preimage/inclusion-list` | `core/preimage/scavenger-hunt` | spec |
| EDICT-OPTIC-TEMPLATE-OWNER-001 | operation-profile optic template has a canonical shape in edict-common.cddl, exported via ABIs | Target/Lawpack | `optic/template/owned` | `optic/template/undefined` | spec |
| EDICT-OPTIC-APERTURE-REF-001 | `apertureRequirement` is a typed reference (footprintCeiling/abstractFootprintObligation), not a string | Language | `optic/aperture/typed-ref` | `optic/aperture/string` | spec |
| EDICT-LANG-CAPABILITIES-SPLIT-001 | requiredSourceCapabilities (compiler) vs requiredCoreCapabilities (hash-significant Core field) | Language/Bundle | `lang/caps/split` | `lang/caps/conflated` | spec |
| EDICT-LANG-FIXED-NUMERIC-001 | Bunny 0.6.0 owns the checked Q32.32 numeric profile; raw arithmetic results and explicit failures are shared compiler/runtime obligations, without implying current source or artifact support | Language | `raw_values_preserve_bits_and_order`; `multiplication_uses_signed_ties_to_even`; `division_uses_signed_ties_to_even` | `checked_products_and_quotients_refuse_invalid_results`; `checked_linear_operations_preserve_exact_boundaries` | spec |
| EDICT-LANG-INT-SAFETY-001 | Integer arithmetic overflow-safe and total; checked forms or static proof; no wrap/saturate/trap | Language | `lang/int/checked` | `lang/int/unproven-overflow` | spec |
| EDICT-TARGET-POSTCOND-001 | Target declares `postconditionSupport`; precommit `guarantee` requires it or rejects | Target/Language | `target/postcond/supported` | `target/postcond/unsupported-guarantee` | spec |
| EDICT-LOWERABILITY-PARTIAL-001 | Lowering is a partial semantics-preserving relation: native/adapted/composite/unsupported; unsupported is a compiler error | Language/Target | `lowering/partial/classified` | `lowering/partial/silent-approx` | spec |
| EDICT-LAWPACK-ADAPTER-DIRECT-001 | v1 direct adaptation only; exactly one adapter per (effect,target); no chained legalization/fixed-point | Lawpack | `lawpack/adapter/direct-one` | `lawpack/adapter/chained` | spec |
| EDICT-LANG-TARGETBUDGET-HASH-001 | targetBudget costAlgebra ref + ceiling both hash-significant; ceiling meaningless without its algebra | Language | `lang/targetbudget/both-hashed` | `lang/targetbudget/algebra-unhashed` | spec |
| EDICT-LANG-OBSTRUCT-EMPTY-001 | Bare obstruction-target normalizes to ObstructionConstruct with empty `{}` payload | Language | `lang/obstruct/bare-empty` | `lang/obstruct/bare-undefined` | spec |
| EDICT-LANG-INTENT-CLAUSES-001 | Intent clause requiredness (at least one of profile/implements — both allowed; budget; basis-unless-template); omission parseable but semantically rejected | Language | `lang/clauses/required-present` | `lang/clauses/missing-budget` | spec |
| EDICT-OPMODE-AUTHORITY-001 | Operation-mode predicates defined authoritatively in Target Profile ABI; language spec mirror must not diverge | Target/Language | `opmode/mirror-matches` | `opmode/mirror-diverges` | spec |
| EDICT-ABI-OPPROFILE-SLOT-001 | Target profiles + lawpacks publish operation-profile records via a hash-locked ABI slot | Target/Lawpack | `abi/opprofile/published` | `abi/opprofile/no-slot` | spec |
| EDICT-LAWPACK-PURE-IMPL-001 | Exported pure helper needs a hash-bound implementation (edict body or component+sandbox+fuel) | Lawpack | `lawpack/pure/has-impl` | `lawpack/pure/signature-only` | spec |
| EDICT-LAWPACK-ADAPTER-TARGETIR-001 | Target adapter digest-locks its accepted target profile + Target IR; resolution can't bind a republished IR | Lawpack | `lawpack/adapter/targetir-locked` | `lawpack/adapter/targetir-unlocked` | spec |
| EDICT-ABI-FAILURE-UNIQUE-001 | An effect's `effectFailures` coordinates must be unique (obstruction map keyed by coordinate) | Target/Lawpack | `abi/failure/unique-coords` | `abi/failure/dup-coord` | spec |
| EDICT-ABI-FAILURE-IDENT-001 | Failure coordinates must be bare Edict `ident`s (source obstruction map LHS only accepts ident) | Target/Lawpack | `abi/failure/ident-coord` | `abi/failure/hyphen-coord` | spec |
| EDICT-ABI-INTRINSIC-UNIQUE-001 | Intrinsic coordinates unique within the corpus (schema-enforced via coordinate-keyed map) | Target | `abi/intrinsic/unique-coords` | `abi/intrinsic/dup-coord` | spec |
| EDICT-ABI-OPPROFILE-UNIQUE-001 | Operation-profile coordinates unique (coordinate-keyed map) so resolution is deterministic | Target/Lawpack | `abi/opprofile/unique-coords` | `abi/opprofile/dup-coord` | spec |
| EDICT-LANG-RECORD-SHORTHAND-001 | Bare `ident` record entry `{ x }` is shorthand for `{ x: x }` | Language | `lang/record/shorthand` | `lang/record/shorthand-unbound` | spec |
| EDICT-LANG-BYTES-NOCANON-001 | `Bytes` refinement is max-only; `canonical=` on `Bytes` is rejected | Language | `lang/bytes/max-only` | `lang/bytes/canonical-rejected` | spec |
| EDICT-ABI-INTRINSICS-DOC-001 | The `intrinsics` resource has a fixed corpus-document shape | Target | `abi/intrinsics/document` | `abi/intrinsics/freeform` | spec |
| EDICT-ABI-VERIFIER-BOUND-001 | An executable verifier requires sandbox + fuel; declarative verifier is classified | Lawpack | `lawpack/verifier/executable-bounded` | `lawpack/verifier/unbounded` | spec |
| EDICT-CORE-SELFHASH-001 | No Core/manifest self-hash; digest is external descriptor | Language/Target | `core/hash/no-selfhash` | `core/hash/embedded-selfhash` | spec |
| EDICT-CORE-NOPACKAGING-001 | Lowerer/verifier digests are bundle fields, not Core | Language/Bundle | `core/hash/lowerer-swap-stable` | `core/hash/lowerer-in-preimage` | spec |
| EDICT-CORE-VERIFIED-EXTERNAL-001 | Core states `requiredOperationProfile`; `verifiedOperationMode` is verifier report | Language | `core/verified/external` | `core/verified/in-core` | spec |
| EDICT-CORE-NODUP-PREPOST-001 | `preconditions`/`postconditions` are derived indices, excluded from preimage | Language | `core/prepost/derived` | `core/prepost/double-hashed` | spec |
| EDICT-CORE-NODIAG-001 | `diagnosticPolicy` is compile option/sidecar, not hashed | Language | `core/diag/sidecar` | `core/diag/in-preimage` | spec |
| EDICT-OPTIC-PRESERVE-001 | Optic contract preserved; support loss/degeneracy/witness debt recorded or rejected | Language | `optic/preserve/affect` | `optic/preserve/silent-loss` | spec |
| EDICT-TARGET-INTRINSIC-CLASS-001 | Intrinsic `pure` vs `effect`, enforced as a CDDL union (pure constructors carry no effect kind/failures) | Target | `target/intrinsic/pure-ctor` | `target/intrinsic/pure-with-effectkind` | spec |
| EDICT-LAWPACK-PURE-001 | Lawpack pure helpers are authority-free, observe no runtime state | Lawpack | `lawpack/pure/clean` | `lawpack/pure/side-door` | spec |
| EDICT-LAWPACK-PURE-002 | Pure helpers bounded (no unbounded alloc/scan, no naked return) | Lawpack | `lawpack/pure/bounded` | `lawpack/pure/unbounded` | spec |
| EDICT-LAWPACK-DAG-001 | Lawpack dependency graph acyclic and digest-locked | Lawpack | `lawpack/dag/acyclic` | `lawpack/dag/cycle` | spec |
| EDICT-LAWPACK-ADAPTER-001 | Missing target adapter is a hard error, never silent fallback | Lawpack | `lawpack/adapter/present` | `lawpack/adapter/missing` | spec |
| EDICT-ABI-NODUP-001 | Normative manifest defined once (schema); no duplicate prose JSON | Target/Lawpack | `abi/nodup/generated` | `abi/nodup/drifted-copy` | spec |
| EDICT-ABI-DISPLAY-001 | Display metadata/codenames live in sidecars, never in manifests | Target/Lawpack/Bundle | `abi/display/sidecar` | `abi/display/codename-in-manifest` | spec |
| EDICT-CONFORMANCE-DIFFERENTIAL-001 | Two independent lowerers/verifiers must byte-match the corpus | Conformance | `conformance/two-lowerer/agree` | `conformance/two-lowerer/diverge` | spec |
| CONTINUUM-BUNDLE-DAG-001 | Artifact graph acyclic; no subject references an attestation over itself | Bundle | `bundle/dag/acyclic` | `bundle/dag/cycle` | spec |
| CONTINUUM-BUNDLE-DAG-MORIARTY-001 | Moriarty must catch any introduced cycle | Bundle/Assurance | `(negative-only)` | `bundle/dag/moriarty-injected-cycle` | spec |
| CONTINUUM-RECEIPT-ACYCLIC-001 | Receipt body never references its signing envelope; DSSE signs body digest | Bundle/Admission | `admission/receipt/body-then-sign` | `admission/receipt/self-signature` | spec |
| CONTINUUM-SOURCEPATH-001 | Source paths are logical package-relative URIs, never machine-local | Bundle | `bundle/sourcepath/logical` | `bundle/sourcepath/absolute` | spec |

## Backlog (IDs to allocate)

Existing acceptance criteria in the Language spec (e.g. FIDLAR rejection,
determinism, atomic application, codename coordinates, content-addressed
duplicate create) will be assigned `EDICT-*` IDs and rows here as their fixtures
are written, so the registry becomes the authoritative checklist for grammar and
Core-schema freeze.

**Deferred entries** (status `deferred`) intentionally have no positive fixture
until their dependent schema lands; their positive-fixture cell records the
blocking dependency (e.g. `(deferred → #3)`). A row whose only fixture is a
negative one (e.g. a Moriarty cycle-injection) marks its positive cell
`(negative-only)`. These markers keep the table's "every ID gets fixtures"
contract honest rather than papering over the gap with an em-dash.
