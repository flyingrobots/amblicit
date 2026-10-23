# Authority Facts Test Plan

Status: current verification design for JSON and canonical authority facts.

## Scope

In scope:

- JSON authority-facts documents with explicit source identity;
- canonical-CBOR authority-facts documents constrained by Edict-owned CDDL;
- file-backed loading into `CompilerContext`;
- profile, budget, profile write-class, and effect write-class facts;
- deterministic merging and structured load failures.

Out of scope:

- full lawpack manifest validation;
- full target-profile manifest loading;
- registry resolution or directory discovery;
- author/reviewer governance;
- Continuum participant trust policy.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| AUTHFACTS-REQ-001 | implemented | Authority-facts documents load from explicit file paths with digest-locked source identity and no registry, directory discovery, or environment fallback. | ROADMAP.md |
| AUTHFACTS-REQ-002 | implemented | Loaded operation-profile and budget facts can resolve compiler-spine source coordinates into `CompilerContext`. | ROADMAP.md, docs/topics/compiler-spine/test-plan.md |
| AUTHFACTS-REQ-003 | implemented | Loaded profile write-class allowances and effect write classes participate in compiler profile/effect compatibility checks. | issue #54 |
| AUTHFACTS-REQ-004 | implemented | Malformed, non-digest-locked, invalid, or conflicting authority facts reject with stable `AuthorityFactsLoadFailureKind` categories before a compiler context is returned. | ROADMAP.md |
| AUTHFACTS-REQ-005 | gap | Trusted lawpack and target-profile authorship governance is not implemented by this loader. | docs/design/authority-fact-governance.md |
| AUTHFACTS-REQ-006 | implemented | `edict.authority-facts/v1` has an Edict-owned CDDL root and `edict.canonical-cbor/v1` byte representation with typed SHA-256 source digests. | EDICT-ABI-AUTHORITY-FACTS-001, issue #157 |
| AUTHFACTS-REQ-007 | implemented | Canonical authority-facts bytes decode into the existing validated `AuthorityFactsDocument` and deterministic `CompilerContext` path. | EDICT-ABI-AUTHORITY-FACTS-001, issue #157 |
| AUTHFACTS-REQ-008 | implemented | Fact maps and write-class sets have one canonical representation independent of producer insertion order, and duplicate fact coordinates reject in JSON, direct-document, and canonical-encoding paths before context construction. | EDICT-ABI-AUTHORITY-FACTS-001, issue #157 |
| AUTHFACTS-REQ-009 | implemented | Non-canonical CBOR, invalid CDDL shape, invalid typed digests, and invalid semantic fact values reject with stable structured failure kinds. | EDICT-ABI-AUTHORITY-FACTS-001, issue #157 |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| crates/amblicit-syntax/tests/authority_facts.rs | Runtime-created authority-facts JSON files for compiler and rejection paths. | Tests pass explicit file paths and assert compiler behavior or stable failure kinds. |
| crates/amblicit-syntax/tests/authority_facts_cbor.rs | Canonical authority-facts byte, ordering, decoding, and compiler-compatibility cases. | Tests assert canonical byte equality, decoded typed meaning, compiler behavior, and stable rejection kinds. |
| fixtures/authority-facts/canonical/example-effectful.authority-facts.json | JSON review/input form used to generate the reviewed canonical artifact. | The existing JSON loader validates it before canonical projection. |
| fixtures/authority-facts/canonical/example-effectful.authority-facts.cbor | Reviewed canonical authority-facts bytes for a runtime-neutral effectful context. | Golden check regenerates bytes from the checked review document and requires an exact match. |
| fixtures/authority-facts/canonical/example-effectful.authority-facts.sha256 | Reviewed domain-framed authority-facts artifact digest. | Golden check recomputes the digest from the canonical value under `edict.authority-facts/v1`. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| AUTHFACTS-TP-001 | implemented | Golden path | AUTHFACTS-REQ-001, AUTHFACTS-REQ-002 | A target-profile fact file plus lawpack fact file produce a `CompilerContext` that compiles `bounded-hello` with the expected profile and budget. | file_backed_authority_facts_compile_bounded_hello | crates/amblicit-syntax/tests/authority_facts.rs | Uses runtime-created files and asserts software behavior. |
| AUTHFACTS-TP-002 | implemented | Boundary guard | AUTHFACTS-REQ-001, AUTHFACTS-REQ-003 | Loaded read-only profile and replace-effect facts cause an effectful source body to reject with `ProfileEffectMismatch`. | file_backed_authority_facts_reject_write_effect_profile_mismatch | crates/amblicit-syntax/tests/authority_facts.rs | Proves the compiler consumes loaded facts, not hand-built context. |
| AUTHFACTS-TP-003 | implemented | Error handling | AUTHFACTS-REQ-004 | Malformed JSON rejects with `InvalidJson`. | malformed_authority_facts_file_rejects_with_stable_kind | crates/amblicit-syntax/tests/authority_facts.rs | Stable failure kind, not parser prose. |
| AUTHFACTS-TP-004 | implemented | Error handling | AUTHFACTS-REQ-004 | A source without a SHA-256 digest rejects with `NonDigestLockedSource`. | nondigest_authority_fact_source_rejects_with_stable_kind | crates/amblicit-syntax/tests/authority_facts.rs | Source identity must be digest-bound. |
| AUTHFACTS-TP-005 | implemented | Error handling | AUTHFACTS-REQ-004 | An omitted source coordinate rejects with `MissingCoordinate`. | omitted_authority_fact_source_coordinate_rejects_with_stable_kind | crates/amblicit-syntax/tests/authority_facts.rs | Valid JSON with a missing required coordinate stays on the structured remediation path. |
| AUTHFACTS-TP-006 | implemented | Error handling | AUTHFACTS-REQ-004 | Conflicting repeated facts reject with `ConflictingFact`. | conflicting_file_backed_authority_facts_reject_before_context | crates/amblicit-syntax/tests/authority_facts.rs | Prevents ambiguous compiler facts. |
| AUTHFACTS-TP-007 | implemented | Error handling | AUTHFACTS-REQ-004 | Repeated authority sources with the same kind and coordinate but different digests reject with `ConflictingFact`. | mixed_authority_source_digests_reject_before_context | crates/amblicit-syntax/tests/authority_facts.rs | Prevents merging facts from multiple reviewed source revisions. |
| AUTHFACTS-TP-008 | implemented | Error handling | AUTHFACTS-REQ-004 | Malformed loaded profile coordinates reject with `InvalidCoordinate`. | invalid_loaded_profile_coordinates_reject_with_stable_kind | crates/amblicit-syntax/tests/authority_facts.rs | Prevents invalid profile identifiers from entering Core. |
| AUTHFACTS-TP-009 | implemented | Error handling | AUTHFACTS-REQ-004 | ABI `custom` write classes load, while non-ABI prefixed custom spellings reject with `InvalidWriteClass`. | abi_custom_write_class_loads_and_prefixed_custom_rejects | crates/amblicit-syntax/tests/authority_facts.rs | Aligns authority-facts loading with target/lawpack write-class vocabulary. |
| AUTHFACTS-TP-010 | implemented | Error handling | AUTHFACTS-REQ-004 | Non-ABI mixed-case write-class spellings reject with `InvalidWriteClass`. | non_abi_write_class_casing_rejects_with_stable_kind | crates/amblicit-syntax/tests/authority_facts.rs | The authority-facts loader accepts exact ABI spellings only. |
| AUTHFACTS-TP-011 | implemented | Error handling | AUTHFACTS-REQ-004 | Directly constructed authority-facts documents reject invalid source, profile, effect, write-class, and budget facts before context merge. | direct_authority_fact_documents_validate_before_context_merge | crates/amblicit-syntax/tests/authority_facts.rs | Public typed APIs must preserve loader invariants. |
| AUTHFACTS-TP-012 | implemented | Error handling | AUTHFACTS-REQ-004 | Explicit file loading reports stable failure kinds for unreadable paths, invalid API versions, and unsupported source kinds. | loader_failure_kinds_cover_public_branches | crates/amblicit-syntax/tests/authority_facts.rs | Covers remaining public loader error branches. |
| AUTHFACTS-TP-013 | gap | Governance | AUTHFACTS-REQ-005 | No author/reviewer trust workflow is claimed by this loader. | - | docs/design/authority-fact-governance.md | Planned for `v0.13.0-alpha.1`. |
| AUTHFACTS-TP-014 | implemented | Golden path | AUTHFACTS-REQ-006, AUTHFACTS-REQ-007 | Canonical authority-facts bytes decode to the existing document model and supply profile, write-class, effect, and budget facts to a successful compiler invocation. | canonical_authority_facts_decode_into_existing_compiler_context | crates/amblicit-syntax/tests/authority_facts_cbor.rs | Proves the byte ABI is not a parallel unused model. |
| AUTHFACTS-TP-015 | implemented | Determinism | AUTHFACTS-REQ-008 | Reordering fact declarations and allowed write classes does not move canonical bytes or the domain-framed digest. | canonical_authority_facts_encoding_is_insertion_order_independent | crates/amblicit-syntax/tests/authority_facts_cbor.rs | Fact coordinates are map keys and write classes normalize as a set. |
| AUTHFACTS-TP-016 | implemented | Error handling | AUTHFACTS-REQ-008, AUTHFACTS-REQ-009 | Non-canonical bytes, malformed roots, unknown fields, invalid typed digests, unsupported source kinds, array-shaped write-class sets, duplicate coordinates, and invalid semantic values reject with exact stable kinds. | canonical_authority_facts_rejections_have_stable_kinds | crates/amblicit-syntax/tests/authority_facts_cbor.rs | Separates byte, shape, duplicate, typed-digest, canonical-set-shape, and semantic failures. |
| AUTHFACTS-TP-017 | implemented | Golden path | AUTHFACTS-REQ-006, AUTHFACTS-REQ-009 | The checked CDDL root accepts both target-profile and lawpack authority sources and rejects shape-incompatible or unsupported-source canonical values. | authority_facts_cddl_accepts_only_the_frozen_root | crates/amblicit-provider-schema/tests/authority_facts_abi.rs | Uses the same CDDL engine as the provider schema registry and covers both source variants required by generated providers. |
| AUTHFACTS-TP-018 | implemented | Determinism | AUTHFACTS-REQ-008 | Uppercase and lowercase digest review hex do not split one typed source identity during context merge. | digest_review_hex_case_does_not_create_a_source_conflict | crates/amblicit-syntax/tests/authority_facts_cbor.rs | Typed digest bytes remove review-case ambiguity. |
| AUTHFACTS-TP-019 | implemented | Error handling | AUTHFACTS-REQ-008 | A JSON authority-facts document cannot repeat one fact coordinate even when the values are identical. | duplicate_fact_coordinates_in_one_json_document_reject_before_context | crates/amblicit-syntax/tests/authority_facts.rs | JSON review input preserves canonical map uniqueness before context construction. |
| AUTHFACTS-TP-020 | implemented | Error handling | AUTHFACTS-REQ-008 | A directly constructed authority-facts document cannot repeat one fact coordinate even when the values are identical. | duplicate_fact_coordinates_in_direct_document_reject_before_context | crates/amblicit-syntax/tests/authority_facts.rs | Public typed construction preserves canonical map uniqueness before context construction. |

## Determinism Obligations

- Tests create explicit temporary files and pass exact paths to the loader.
- Tests do not inspect stdout, stderr, diagnostic prose, filesystem ordering,
  network state, registry state, environment configuration, random values, or
  wall-clock time.
- Loader tests assert compiler behavior and stable failure kinds.
- Canonical fact-map construction and write-class normalization do not depend on
  declaration insertion order.
- Golden checks compare exact canonical bytes and the domain-framed digest
  against executable codec output.
- The loader must not fetch packages, discover directories, or mutate
  dependency state.

## Open Gaps

- No trusted lawpack or target-profile authorship workflow exists.
- Full lawpack and target-profile manifest instance loading remains future
  work.
- Intrinsic, obstruction, obligation, adapter, footprint, and cost corpora are
  not loaded by this first authority-facts slice.
