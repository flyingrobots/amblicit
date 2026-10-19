# External-Action Requests Test Plan

Status: current verification design for issue #172.

## Scope

In scope:

- digest-locked `capability` imports that name requestable external operation
  families;
- source-authored external-action request declarations;
- deterministic Core and Target IR representation of request data;
- package-closure binding for operation, schema, and reconciliation resources;
- explicit awaiting-settlement and schema-admission posture;
- compile-time denial of undeclared or directly callable operation families;
- omission of filesystem, process, network, Git, GitHub, model, and shell
  authority from compiler and provider interfaces.

Out of scope:

- performing external actions;
- adapter selection or authorization;
- live path, ref, basis, budget, or settlement admission;
- stack suspension, continuations, or `async` host frames;
- general-purpose effects or ambient I/O.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| EXTREQ-REQ-001 | implemented | Source syntax binds each requestable operation through one digest-locked `capability` import and carries exact input-schema, settlement-schema, authority-scope, basis, budget, input, and reconciliation-law values. | issue #172 |
| EXTREQ-REQ-002 | implemented | Core represents external-action requests as data with compiler-owned binding identity, typed input and settlement coordinates, explicit `awaitingSettlement` state, and required schema admission. | issue #172 |
| EXTREQ-REQ-003 | implemented | Target IR preserves external-action requests separately from callable target steps and target intrinsics. | issue #172 |
| EXTREQ-REQ-004 | implemented | Core and Target IR semantic closure bind the exact capability resource; undeclared operations, floating capability imports, empty request resource coordinates, and duplicate Target request identities fail closed. | issue #172 |
| EXTREQ-REQ-005 | implemented | Equivalent admitted source and compiler facts produce byte-identical Core and Target IR; every request authority or meaning mutation moves the corresponding digest. | issue #172 |
| EXTREQ-REQ-006 | implemented | Runtime-valued authority scope, basis, and budget expressions survive compilation for Echo admission; Edict performs no external action while compiling or lowering them. | issue #172 |
| EXTREQ-REQ-007 | implemented | The request-family allowlist contains only the domain-specific `workspace` root; raw filesystem, process, network, Git, GitHub, model, shell, case-variant, abbreviation, and unregistered roots are outside the requestable capability vocabulary. | issue #172 |
| EXTREQ-REQ-008 | implemented | Request construction remains bounded under a fixed-seed mutation corpus and a 64-request stress module. | issue #172 |
| EXTREQ-REQ-009 | implemented | The public application build explicitly selects request-only publication, validates the exact source/root-reachable-lawpack/adapter/target-profile closure, rejects zero requests, callable-step mixtures, disconnected lawpacks, profile-budget mismatches, and substituted capability manifests, and atomically publishes exact canonical Core and Target IR bytes without invoking a provider component. | issue #176 |
| EXTREQ-REQ-010 | implemented | A real `workspace.patch.applyValidated@1` closure binds canonical patch input, exact workspace basis, writable-path policy authority, request budgets, settlement schema, and reconciliation law as non-callable request data; compiler-owned Core and Target IR remain independently derivable without granting write authority. | issue #178 |
| EXTREQ-REQ-011 | implemented | A public external-action build resolves every request input schema, settlement schema, and reconciliation law to one canonical compiler-owned resource artifact whose exact domain-framed digest matches Core and Target IR; missing, substituted, duplicate, disconnected, non-canonical, or sentinel identities fail before publication. | issue #180 |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| In-test `workspace.snapshot.observe@1` source | First bounded read-only external request. | Public parser, compiler, canonical encoders, and Target IR lowerer preserve the exact request contract. |
| Fixed seed `0x4558_5452_4551_0001` | Determinism and mutation corpus. | Repeated compilation is byte-identical and distinct capability identities produce distinct Core identities. |
| CLI publication seed `0x5eed_1a77_c105_0a11` | Deterministic paired-publication corpus. | Sixteen fixed-seed Core/Target IR pairs publish byte-exactly, and 64 bounded pairs publish without growth. |
| 64-request generated module | Bounded stress case. | All 64 requests survive Core and Target IR without becoming callable steps. |
| `fixtures/lawpack/workspace-snapshot/` | Exact public-build capability closure. | The owning generator reproduces manifest, exports, request-only adapter, target configuration, source, Core, and Target IR; the public build reproduces the checked compiler bytes. |
| `fixtures/lawpack/workspace-patch/` | Basis-bound validated patch request closure. | The owning generator reproduces manifest, exports, request-only adapter, target configuration, source, Core, and Target IR with one request and zero callable steps. |
| Generator-owned external-action resource artifacts | Exact request schema and reconciliation closure. | Each canonical artifact carries its coordinate, resource kind, and complete definition; its domain-framed digest is pinned by the source request and independently recomputed by the public build. |

## Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| EXTREQ-TP-001 | implemented | Golden path | EXTREQ-REQ-001, EXTREQ-REQ-002, EXTREQ-REQ-003 | One workspace observation request parses, compiles, and lowers with exact structured fields, awaiting posture, and zero callable target steps. | workspace_observation_request_compiles_as_non_callable_data | crates/edict-syntax/tests/external_action_requests.rs | The target owns later admission and execution. |
| EXTREQ-TP-002 | implemented | Closure guard | EXTREQ-REQ-004 | Missing operation imports and floating capability digests reject during compilation; removing the exact capability from an already constructed Core or Target artifact rejects during canonical encoding. | undeclared_or_floating_operation_families_fail_closed, request_operation_must_remain_in_core_and_target_capability_closure | crates/edict-syntax/tests/external_action_requests.rs | A source alias is not authority by itself. |
| EXTREQ-TP-003 | implemented | Authority guard | EXTREQ-REQ-003, EXTREQ-REQ-007 | Direct invocation or obstruction-position use of a capability import fails; case variants, abbreviations, unregistered roots, and raw ambient operation families reject with stable compiler kinds. | capability_import_cannot_be_called_as_an_effect, capability_import_cannot_be_used_as_an_obstruction_coordinate, ambient_operation_families_are_not_requestable | crates/edict-syntax/tests/external_action_requests.rs | Request authority and performance authority remain distinct. |
| EXTREQ-TP-004 | implemented | Dynamic boundary | EXTREQ-REQ-006 | Scope, basis, maximum settlement bytes, and attempt count remain Core expressions sourced from admitted input. | dynamic_admission_values_survive_without_compile_time_execution | crates/edict-syntax/tests/external_action_requests.rs | Echo owns value-instance admission. |
| EXTREQ-TP-005 | implemented | Determinism | EXTREQ-REQ-005 | Repeated compilation and lowering produce identical canonical bytes and digests. | request_artifacts_are_reproducible | crates/edict-syntax/tests/external_action_requests.rs | No clock, randomness, filesystem, or network input enters the compiler. |
| EXTREQ-TP-006 | implemented | Mutation sensitivity | EXTREQ-REQ-005 | Operation, input schema, settlement schema, authority scope, basis, both budget fields, input, and reconciliation mutations move both Core and Target IR identity. | every_request_authority_field_moves_core_and_target_identity | crates/edict-syntax/tests/external_action_requests.rs | Each field is request-authoritative. |
| EXTREQ-TP-007 | implemented | Property | EXTREQ-REQ-005, EXTREQ-REQ-008 | A fixed-seed 32-case capability-digest corpus is reproducible and collision-free within the corpus. | fixed_seed_request_identity_corpus_is_deterministic | crates/edict-syntax/tests/external_action_requests.rs | Seed is recorded in the fixture table. |
| EXTREQ-TP-008 | implemented | Stress | EXTREQ-REQ-008 | A generated module with 64 request declarations emits 64 Core requests and 64 Target IR requests with zero callable steps. | sixty_four_requests_remain_bounded_non_callable_data | crates/edict-syntax/tests/external_action_requests.rs | Bound is fixed for CI. |
| EXTREQ-TP-009 | implemented | Closure guard | EXTREQ-REQ-004 | Canonical Core and Target IR encoding reject request operations removed from their exact capability closure; a request with atomic operands passes Core integrity at graph depth 128 and reaches closure validation, while depth 129 fails Core integrity with `DepthExceeded` and encoding with `UnsupportedValue`. | request_operation_must_remain_in_core_and_target_capability_closure, nested_request_validation_distinguishes_graph_depth_and_closure_failures | crates/edict-syntax/tests/external_action_requests.rs | Manual artifact construction cannot bypass compiler-owned closure, and the graph-depth rejection precedes the closure prepass. |
| EXTREQ-TP-010 | implemented | Bundle boundary | EXTREQ-REQ-004 | Contract-bundle assembly rejects a digest-locked Target IR capability absent from the supplied Core closure. | assembly_from_target_ir_rejects_artifact_capability_substitution | crates/edict-syntax/tests/contract_bundle.rs | A self-consistent target artifact cannot substitute source-owned request authority. |
| EXTREQ-TP-011 | implemented | Public projection | EXTREQ-REQ-002, EXTREQ-REQ-003, EXTREQ-REQ-004 | The public CLI review projection carries the complete Core and Target IR request, exact capability closure, awaiting-settlement posture, and no callable target step. | project_exposes_external_requests_as_non_callable_review_data | crates/edict-cli/tests/jsonl_cli.rs | Projection is review data, not execution authority. |
| EXTREQ-TP-012 | implemented | Canonical identity guard | EXTREQ-REQ-004 | Canonical Core encoding rejects empty request schema or reconciliation coordinates, and canonical Target IR encoding rejects duplicate request ids within one intent. | request_resource_coordinates_must_be_nonempty, duplicate_target_request_ids_reject_before_identity | crates/edict-syntax/tests/external_action_requests.rs | Waiting and settlement identity cannot be ambiguous or anonymous. |
| EXTREQ-TP-013 | implemented | Tooling guard | EXTREQ-REQ-001 | A non-call request operation has its own stable parser kind, and `request` is highlighted as a keyword. | non_call_request_operation_has_a_request_specific_parse_kind, request_statement_introducer_is_highlighted_as_a_keyword | crates/edict-syntax/tests/external_action_requests.rs, crates/edict-syntax/tests/highlighting.rs | Request syntax remains distinct from semantic effect syntax. |
| EXTREQ-TP-014 | implemented | Golden artifact | EXTREQ-REQ-002, EXTREQ-REQ-003, EXTREQ-REQ-004, EXTREQ-REQ-005 | The checked workspace-snapshot source reproduces exact compiler-owned Core and Target IR canonical bytes and domain-framed digests. | core_goldens_match_executable_encoder, target_ir_goldens_match_executable_encoder | fixtures/lang/external-actions/workspace-snapshot.edict, fixtures/core/canonical/workspace-snapshot.core.cbor, fixtures/core/canonical/workspace-snapshot.core.sha256, fixtures/target-ir/canonical/workspace-snapshot.target-ir.cbor, fixtures/target-ir/canonical/workspace-snapshot.target-ir.sha256 | Generated only through the owning xtask commands. |
| EXTREQ-TP-015 | implemented | Public build | EXTREQ-REQ-009 | A real `edict.application/v1` request loads the generated workspace closure and exact Echo target profile, publishes checked canonical Core and Target IR bytes, removes stale executable outputs, and reruns byte-identically. | public_external_action_build_emits_exact_compiler_artifacts | crates/edict-cli/src/application_build.rs, fixtures/lawpack/workspace-snapshot/README.md, fixtures/providers/echo-target-profile/README.md | Provider components are outside the request-only route. |
| EXTREQ-TP-016 | implemented | Closure refusal | EXTREQ-REQ-004, EXTREQ-REQ-009 | A request operation whose digest no longer equals its owning supplied capability manifest, or a supplied lawpack unreachable from the ordered root, is rejected before output publication; an independently versioned operation coordinate remains valid when its authority digest is exact. | external_action_build_rejects_a_substituted_capability_manifest, public_external_action_build_rejects_capability_substitution, public_external_action_build_rejects_a_disconnected_lawpack, external_action_build_binds_operation_authority_by_manifest_digest | crates/edict-cli/src/application_build.rs | Internal Core closure and a graph-valid disconnected manifest are each insufficient for public application authority; no undeclared coordinate derivation convention is inferred. |
| EXTREQ-TP-017 | implemented | Execution-class refusal | EXTREQ-REQ-003, EXTREQ-REQ-009 | The request-only build rejects zero requests and any artifact mixing external requests with callable Target IR steps. | external_action_build_requires_a_typed_request, external_action_build_rejects_mixed_callable_execution | crates/edict-cli/src/application_build.rs | The first host route has one execution class. |
| EXTREQ-TP-018 | implemented | Publication transaction | EXTREQ-REQ-009 | Paired request artifacts are deterministic under the recorded CLI publication seed and bounded stress corpus; stale executable outputs are removed; publication failure preserves the prior request pair. | external_action_pair_publication_is_deterministic_for_a_fixed_seed_corpus, external_action_pair_publication_remains_bounded_under_stress, external_action_publication_removes_stale_executable_outputs, failed_external_action_pair_publication_preserves_previous_core | crates/edict-cli/src/application_build.rs | Output ownership is symmetric across build kinds. |
| EXTREQ-TP-019 | implemented | Validated patch request | EXTREQ-REQ-010 | The patch operation, input schema, settlement schema, basis, authority scope, budgets, and reconciliation law survive as one non-callable Core and Target request. | validated_patch_request_compiles_as_non_callable_data | crates/edict-syntax/tests/external_action_requests.rs | Dynamic patch, basis, and path-policy instances remain Echo admission obligations. |
| EXTREQ-TP-020 | implemented | Golden artifact | EXTREQ-REQ-005, EXTREQ-REQ-010 | The owning generator reproduces the complete validated-patch lawpack and exact compiler-owned Core and Target IR bytes. | lawpack_goldens_match_executable_codec | fixtures/lawpack/workspace-patch/README.md, xtask/src/lawpack_goldens.rs | Generated artifacts are never hand-edited. |
| EXTREQ-TP-021 | implemented | Authority mutation | EXTREQ-REQ-005, EXTREQ-REQ-010 | Patch, basis, authority, budget, schema, operation, and reconciliation mutations move both Core and Target identity. | every_request_authority_field_moves_core_and_target_identity | crates/edict-syntax/tests/external_action_requests.rs | The generic mutation oracle applies to both domain-specific request families. |
| EXTREQ-TP-022 | implemented | Property | EXTREQ-REQ-008, EXTREQ-REQ-010 | The fixed-seed request identity corpus remains deterministic and collision-free for request authority changes. | fixed_seed_request_identity_corpus_is_deterministic | crates/edict-syntax/tests/external_action_requests.rs | Seed `0x4558_5452_4551_0001` remains authoritative. |
| EXTREQ-TP-023 | implemented | Stress | EXTREQ-REQ-008, EXTREQ-REQ-010 | Sixty-four request declarations remain bounded and non-callable. | sixty_four_requests_remain_bounded_non_callable_data | crates/edict-syntax/tests/external_action_requests.rs | Fixed CI bound; no adapter execution occurs. |
| EXTREQ-TP-024 | implemented | Resource golden path | EXTREQ-REQ-011 | The public build loads the exact generated input-schema, settlement-schema, and reconciliation-law artifacts referenced by every request and publishes only after their coordinates and digests reproduce the compiler-owned request closure. | public_external_action_build_emits_exact_compiler_artifacts | crates/edict-cli/src/application_build.rs, fixtures/lawpack/workspace-snapshot/README.md, fixtures/lawpack/workspace-patch/README.md | Resolution grants no execution authority. |
| EXTREQ-TP-025 | implemented | Resource refusal | EXTREQ-REQ-011 | A missing artifact, substituted artifact bytes, duplicate coordinate, disconnected artifact, invalid shape, opaque definition, or non-canonical encoding rejects before output publication. | public_external_action_build_rejects_invalid_request_resource_closure, public_external_action_build_rejects_opaque_resource_definitions | crates/edict-cli/src/application_build.rs | Configuration and artifact bytes must corroborate one complete closure. |
| EXTREQ-TP-026 | implemented | Sentinel refusal | EXTREQ-REQ-011 | Repeated-byte sentinel digests and other unresolved identities reject even when they are syntactically valid SHA-256 strings. | public_external_action_build_rejects_sentinel_resource_identities | crates/edict-cli/src/application_build.rs | Syntactic digest validity is not artifact identity. |
| EXTREQ-TP-027 | implemented | Property | EXTREQ-REQ-005, EXTREQ-REQ-011 | A fixed-seed 16-case mutation corpus changes one canonical resource artifact at a time and every stale request digest rejects. | fixed_seed_request_resource_mutations_fail_closed | crates/edict-cli/src/application_build.rs | Seed `0x4558_5452_4551_0180` is authoritative. |
| EXTREQ-TP-028 | implemented | Stress | EXTREQ-REQ-008, EXTREQ-REQ-011 | A 64-request resource closure resolves within the fixed CI bound, with no unreferenced artifact accepted, and application configuration rejects a 193rd artifact above the declared 192-resource ceiling. | sixty_four_request_resources_resolve_as_one_bounded_closure, external_action_resource_configuration_has_a_fixed_boundary | crates/edict-cli/src/application_build.rs | Cardinalities are fixed at 64 requests and 192 configured artifacts. |

## Determinism Obligations

- Tests use no filesystem discovery, network access, clock, environment, or
  randomness.
- The property and CLI publication corpora use their recorded fixed seeds and
  local deterministic generators.
- Canonical comparisons assert decoded structured fields and exact bytes, not
  diagnostics or log text.
- Stress cardinality is fixed at 64.

## Open Gaps

- Echo admission of compiler-emitted request values.
- A concrete workspace-observation adapter and settlement witness.
- Echo execution of the compiler-owned basis-bound validated patch request.
