# Fixtures Test Plan

Status: current verification design for shared fixtures and reviewed artifacts.

## Scope

In scope:

- checked-in source fixtures consumed by executable parser, validator, compiler,
  highlighter, grammar, and canonicalization behavior;
- reviewed Core golden bytes and exact digest artifacts;
- reviewed authority-facts canonical bytes and exact digest artifact;
- reviewed Edict-owned target-profile contract-resource bytes and digests;
- reviewed lawpack manifest/export canonical bytes, exact digests, and
  digest-pinned source;
- contract-graph validation of fixture references in topic test plans.

Out of scope:

- runtime-owned target-profile, contract-bundle, admission, and conformance
  fixture families that do not have executable owning behavior yet;
- tests that assert documentation prose about fixtures.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| FIXTURES-REQ-001 | implemented | Source fixtures under `fixtures/lang/` are executable behavior inputs, not illustrative prose. | fixtures/README.md |
| FIXTURES-REQ-002 | implemented | Reviewed Core golden artifacts are generated from the executable compiler and canonical encoder, then checked for exact bytes and digest stability. | fixtures/core/canonical/README.md, xtask/src/goldens.rs |
| FIXTURES-REQ-003 | implemented | Topic-shelf fixture references resolve to checked-in artifacts through the local contract graph. | xtask/src/contract_check.rs |
| FIXTURES-REQ-004 | gap | Runtime-owned target-profile, contract-bundle, admission, and conformance fixture families remain unpopulated until owning behavior lands. | fixtures/README.md, ROADMAP.md |
| FIXTURES-REQ-005 | implemented | Reviewed authority-facts golden artifacts are generated from the validated JSON review input and executable canonical codec, then checked for exact bytes and digest stability. | fixtures/authority-facts/canonical/README.md, xtask/src/goldens.rs |
| FIXTURES-REQ-006 | implemented | Reviewed Edict-owned target-profile contract resources are generated from the executable semantic model, then checked for exact canonical bytes and coordinate-framed digest stability. | fixtures/target-profile/contract-resources/README.md, xtask/src/goldens.rs |
| FIXTURES-REQ-007 | implemented | The reviewed provider contract pack and manifest are generated from Edict-owned ABI schemas and canonical target-profile resources, then checked for exact bytes, root closure, and raw digest stability. | issue #161, docs/topics/providers/test-plan.md |
| FIXTURES-REQ-008 | implemented | The reviewed Hello Echo lawpack manifest and export bytes, exact digests, and digest-pinned source are generated or checked through the executable lawpack loader. | issue #169, docs/topics/lawpacks/test-plan.md |
| FIXTURES-REQ-009 | implemented | The portable causal-cell lawpack closure is generator-owned and checked as exact canonical manifest, exports, adapter, target-configuration, and digest artifacts after an executable compiler witness. | docs/topics/lawpacks/test-plan.md |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| fixtures/README.md | Fixture constitution. | Defines executable source, negative, and golden fixture roles plus placeholder digest handling. |
| fixtures/lang/bounds/bounded-hello.edict | Bounded source fixture. | Parses, validates, compiles to initial Core, and produces reviewed Core artifacts. |
| fixtures/lang/effects/conditional-blob.edict | Control-flow source fixture. | Parses and validates for current surface checks. |
| fixtures/lang/effects/read-greeting.edict | Imported effect source fixture. | Parses and validates with shape, lawpack, target, obstruction, and effect-call syntax. |
| fixtures/lang/types/color-match.edict | Variant/match source fixture. | Parses variant and match syntax. |
| fixtures/lang/tooling/highlight-smoke.edict | Developer-tooling source fixture. | Emits public editor highlight roles for comments, keywords, identifiers, strings, numbers, operators, punctuation, and type identifiers. |
| fixtures/core/canonical/bounded-hello.core.cbor | Reviewed canonical Core byte fixture. | Matches executable `edict.canonical-cbor/v1` output for `bounded-hello`. |
| fixtures/core/canonical/bounded-hello.core.sha256 | Reviewed Core digest fixture. | Matches executable `edict.core.module/v1` digest output for `bounded-hello`. |
| fixtures/authority-facts/canonical/example-effectful.authority-facts.json | Validated authority-facts review/input fixture. | Loads through the existing strict JSON loader before golden generation. |
| fixtures/authority-facts/canonical/example-effectful.authority-facts.cbor | Reviewed canonical authority-facts byte fixture. | Matches executable `edict.canonical-cbor/v1` output. |
| fixtures/authority-facts/canonical/example-effectful.authority-facts.sha256 | Reviewed authority-facts digest fixture. | Matches the domain-framed `edict.authority-facts/v1` digest. |
| fixtures/target-profile/contract-resources/README.md | Five reviewed Edict-owned target-profile contract resources and digest fixtures. | Every file matches the executable model and its coordinate-framed identity. |
| fixtures/provider-contracts/structured-control-v1/edict-provider-contracts.cddl | Reviewed self-contained provider CDDL pack. | Exact bytes match deterministic assembly from authoritative ABI sources. |
| fixtures/provider-contracts/structured-control-v1/manifest.json | Reviewed provider contract-pack manifest. | Exact bytes and all embedded schema/resource identities match deterministic assembly. |
| fixtures/lawpack/hello-echo/README.md | First reviewed canonical lawpack fixture and digest-pinned source. | `cargo xtask lawpack-goldens --check` reproduces exact bytes and digests; the public loader corroborates the source import digest. |
| fixtures/lawpack/causal-cell/README.md | Portable capability closure for external Edict applications. | `cargo xtask lawpack-goldens --check` validates, compiles through, and reproduces the exact closure. |

## Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| FIXTURES-TP-001 | implemented | Source behavior | FIXTURES-REQ-001 | Source fixtures are consumed through public APIs and produce stable parser, validator, compiler, and highlighter behavior. | bounded_hello_parses, read_greeting_parses, conditional_blob_fixture_parses, palette_fixture_parses, phase1_fixtures_validate_semantically, bounded_hello_compiles_to_initial_core, highlight_source_emits_editor_roles_for_fixture | fixtures/lang/bounds/bounded-hello.edict, fixtures/lang/effects/conditional-blob.edict, fixtures/lang/effects/read-greeting.edict, fixtures/lang/types/color-match.edict, fixtures/lang/tooling/highlight-smoke.edict | Fixtures are behavior inputs, not prose anchors. |
| FIXTURES-TP-002 | implemented | Golden artifact | FIXTURES-REQ-002 | Reviewed Core byte and digest fixtures exactly match executable compiler and encoder output. | reviewed_core_golden_bytes_match_executable_encoder, reviewed_core_digest_matches_exact_fixture | xtask/src/goldens.rs, fixtures/lang/bounds/bounded-hello.edict, fixtures/core/canonical/bounded-hello.core.cbor, fixtures/core/canonical/bounded-hello.core.sha256 | `cargo xtask core-goldens --check` covers the same artifact contract. |
| FIXTURES-TP-003 | implemented | Contract graph | FIXTURES-REQ-003 | Topic-shelf test plans cannot cite missing fixture paths. | contract_graph_is_valid | xtask/src/contract_check.rs, xtask/src/tests.rs, fixtures/README.md | The checker validates referenced artifacts rather than prose. |
| FIXTURES-TP-004 | gap | Future corpus | FIXTURES-REQ-004 | No runtime-owned target-profile, contract-bundle, admission, or conformance fixture corpus is claimed before owning behavior lands. | - | - | Add these families with the implementation slice that first consumes them. |
| FIXTURES-TP-005 | implemented | Golden artifact | FIXTURES-REQ-005 | Reviewed authority-facts byte and digest fixtures exactly match validated JSON input plus executable canonical codec output. | authority_facts_goldens_match_executable_codec | xtask/src/goldens.rs, fixtures/authority-facts/canonical/example-effectful.authority-facts.json, fixtures/authority-facts/canonical/example-effectful.authority-facts.cbor, fixtures/authority-facts/canonical/example-effectful.authority-facts.sha256 | `cargo xtask authority-facts-goldens --check` covers the same artifact contract. |
| FIXTURES-TP-006 | implemented | Golden artifact | FIXTURES-REQ-006 | All five reviewed target-profile contract-resource bytes and digests exactly match the executable semantic model. | target_profile_resource_goldens_match_executable_contract | xtask/src/goldens.rs, fixtures/target-profile/contract-resources/README.md | `cargo xtask target-profile-resource-goldens --check` covers the same artifact contract. |
| FIXTURES-TP-007 | implemented | Golden artifact | FIXTURES-REQ-007 | Provider contract-pack CDDL and manifest fixtures exactly match executable assembly, while check mode detects drift without modifying either file. | provider_contract_pack_goldens_match_executable_contract, provider_contract_pack_check_rejects_drift_without_rewriting | xtask/src/provider_contract_pack.rs, xtask/src/tests.rs, fixtures/provider-contracts/structured-control-v1/edict-provider-contracts.cddl, fixtures/provider-contracts/structured-control-v1/manifest.json | `cargo xtask provider-contract-pack --check` is the focused drift gate. |
| FIXTURES-TP-008 | implemented | Golden artifact | FIXTURES-REQ-008 | The Hello Echo lawpack loader accepts the exact checked-in manifest/export bytes, reproduces their digests, and proves the `.edict` import pins the manifest identity. | hello_echo_lawpack_bundle_loads_from_exact_canonical_resources | fixtures/lawpack/hello-echo/README.md, crates/edict-syntax/tests/lawpack.rs, xtask/src/lawpack_goldens.rs | `cargo xtask lawpack-goldens --check` is the focused drift gate. |
| FIXTURES-TP-009 | implemented | Golden artifact | FIXTURES-REQ-009 | The causal-cell generator validates the bundle and adapter, compiles and lowers a digest-pinned Edict witness, and reproduces the exact closure bytes and digest sidecars. | lawpack_goldens_match_executable_codec | fixtures/lawpack/causal-cell/README.md, xtask/src/lawpack_goldens.rs, xtask/src/tests.rs | The fixture is a portable capability, not an exact application provider. |

## Determinism Obligations

- Source fixture tests must assert stable software behavior, such as AST shape,
  error kind, validation status, compiler output, highlighter role, or generated
  artifact equality.
- Golden tests compare behavior-derived bytes and digests against reviewed
  artifacts.
- Fixture coverage must not pass merely because a file exists or prose contains
  a phrase.

## Open Gaps

- Runtime-owned target-profile, contract-bundle, admission, and conformance
  fixture families remain future work.
- Additional reviewed Core golden fixtures should be added as lowerable Core
  language coverage expands.
