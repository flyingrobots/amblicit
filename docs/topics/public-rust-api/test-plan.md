# Public Rust API Test Plan

## Scope

In scope:

- a curated Rust facade with the library name `edict`;
- source checking and stable diagnostic-kind access;
- canonical Core, Target IR, and result-projection identity access;
- package inventory and clean external-consumer checks;
- an explicit non-publication boundary.

Out of scope:

- crates.io publication or crate-name reservation;
- a stable 1.0 API;
- exposing the implementation crate's full module tree;
- replacing the CLI application-build boundary;
- splitting every compiler subsystem into its final crate.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| PUBRUST-REQ-001 | implemented | One curated package exposes Edict source checking, stable diagnostic kinds, and canonical artifact identity operations without re-exporting the implementation module tree. | issue #189 |
| PUBRUST-REQ-002 | planned | The facade's package inventory is explicit, reproducible, and remains non-publishing until a separately approved publication policy exists. | issue #204 |
| PUBRUST-REQ-003 | planned | A clean external consumer can compile against the facade without an undocumented repository-relative dependency. | issue #204 |
| PUBRUST-REQ-004 | implemented | Release preparation advances the facade package version and exact implementation dependency together. | xtask/src/release_prep.rs |
| PUBRUST-REQ-005 | implemented | The facade exposes the checked numeric foundation without leaking Bunny saturating operators or implying source/IR support. | docs/SPEC_edict-language-v1.md |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PUBRUST-TP-001 | implemented | Public API | PUBRUST-REQ-001 | Source checking accepts valid input and reports stable parse and semantic failure kinds; consumer artifact values encode, decode, digest, and verify. | curated_facade_checks_source_and_reports_stable_failures, facade_consumer_constructs_and_verifies_artifacts | crates/edict/tests/public_surface.rs, crates/edict/tests/artifact_models.rs | Operations execute on valid and invalid inputs; no representation checks. |
| PUBRUST-TP-002 | implemented | Negative compile | PUBRUST-REQ-001 | A facade-only consumer compiles supported imports, then receives Rust E0432 for the implementation parser module. | implementation_modules_are_unavailable_to_consumers | crates/edict/src/lib.rs, crates/edict/tests/public_surface.rs | The independent consumer checks structured compiler diagnostics; the compile-fail doctest remains an additional workspace witness. |
| PUBRUST-TP-003 | planned | Package boundary | PUBRUST-REQ-002 | Packaging succeeds with the reviewed inventory without publishing or mutating registry state. | release-engineering package check | crates/edict/Cargo.toml | The current package inventory dry run succeeds; the complete registry dependency closure remains unpublished. |
| PUBRUST-TP-004 | planned | External consumer | PUBRUST-REQ-003 | The project compiles and runs without a sibling Edict checkout. | release-engineering external-consumer check | - | Requires packaged implementation dependencies or a sealed local registry before publication. |
| PUBRUST-TP-005 | implemented | Release preparation | PUBRUST-REQ-004 | Cargo resolves the requested facade and implementation versions with the prepared lockfile. | release_prep_keeps_facade_exact_dependency_resolvable | xtask/src/tests.rs | Offline temporary workspace; no registry publication. |
| PUBRUST-TP-006 | implemented | Consumer model closure | PUBRUST-REQ-001 | A consumer using only facade imports constructs Core, Target IR, and projection values, names decoded values and verified projections, and reads diagnostic spans. | facade_consumer_constructs_and_verifies_artifacts, facade_consumer_names_diagnostic_spans | crates/edict/tests/artifact_models.rs | The integration test is a separate consumer crate; it uses no implementation imports. |
| PUBRUST-TP-007 | implemented | Numeric consumer | PUBRUST-REQ-005 | A facade-only consumer evaluates literal Q32.32 vectors and distinguishes overflow from division by zero. | raw_values_preserve_bits_and_order, checked_linear_operations_preserve_exact_boundaries, multiplication_uses_signed_ties_to_even, division_uses_signed_ties_to_even, checked_products_and_quotients_refuse_invalid_results | crates/edict/tests/numeric_foundation.rs | No direct Bunny import or fixed-point source/artifact claim. |

## Known Gaps

- The implementation dependency still needs a permanent registry package name
  and a completed dependency-closure dry run before publication can be
  considered.
- Registry names, ownership, credentials, and publication automation remain
  deliberately unconfigured.
