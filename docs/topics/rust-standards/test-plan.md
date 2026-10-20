# Rust Standards Test Plan

Status: current verification design for Rust engineering policy.

## Scope

In scope:

- workspace lint baseline;
- workspace minimum-supported-Rust-version and CI alignment;
- deterministic compiler/library policy;
- structured public failure policy;
- RED/GREEN and behavior-first testing policy;
- documentation-impact and claim-integrity policy;
- dependency supply-chain enforcement; and
- planned lint and fuzzing ratchets.

Out of scope:

- immediate global denial of `expect`, `panic`, or stdout/stderr in tests and
  `xtask`;
- crates.io publication policy;
- cargo-audit or fuzz harnesses.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| RUST-REQ-001 | implemented | Workspace lint configuration forbids unsafe code, denies missing `Debug` implementations, and denies Clippy `all` and `pedantic` lint groups. | Cargo.toml |
| RUST-REQ-002 | policy | Library and compiler paths must avoid hidden I/O and nondeterminism unless the public function explicitly defines that boundary. | docs/topics/rust-standards/README.md |
| RUST-REQ-003 | policy | Public validation APIs should return structured failures with stable kind enums and machine-usable fields. | docs/topics/rust-standards/README.md |
| RUST-REQ-004 | policy | Tests assert software behavior and stable contract artifacts, not implementation details, documentation details, repository structure, diagnostic prose, incidental output, or live service state. | AGENTS.md, docs/topics/rust-standards/README.md |
| RUST-REQ-005 | policy | Contract-bearing changes update affected documentation, verify docs unchanged, or state `docs-impact: none`; generated and golden artifacts require executable checks. | AGENTS.md, docs/topics/rust-standards/README.md |
| RUST-REQ-006 | planned | Library-code footgun lints for `unwrap`, `expect`, `panic`, `todo`, `unimplemented`, debug macros, and direct stdout/stderr should become deny-level after scoped test and `xtask` allowances exist. | docs/topics/rust-standards/README.md |
| RUST-REQ-007 | implemented | CI pins cargo-deny and checks the root plus provider fixture guest lockfiles for advisories, yanked crates, reviewed licenses, dependency bans, and allowed sources. | deny.toml, .github/workflows/ci.yml |
| RUST-REQ-008 | planned | Parser, lexer, decoder, and authority-facts fuzz targets should be added as the language surface grows. | docs/topics/rust-standards/README.md |
| RUST-REQ-009 | implemented | The workspace declares Rust `1.96` as its MSRV, and CI runs the complete formatting, lint, and test matrix on exact Rust `1.96.0` plus stable. | Cargo.toml, .github/workflows/ci.yml |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| Cargo.toml | Workspace lint baseline. | The xtask regression checks configured lint levels. |
| AGENTS.md | Agent-facing testing and documentation rules. | Policy rows cite the contributor contract. |
| deny.toml | Cargo dependency policy. | The xtask regression checks the enforced policy sections and critical deny settings. |
| .github/workflows/ci.yml | Rust and supply-chain CI contract. | The xtask regressions check exact MSRV and cargo-deny commands. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| RUST-TP-001 | implemented | Lint guard | RUST-REQ-001 | Workspace lint configuration contains the required deny/forbid baseline. | rust_workspace_lints_define_safety_baseline | Cargo.toml | Static configuration contract, not prose. |
| RUST-TP-002 | policy | Determinism policy | RUST-REQ-002 | Human review rejects hidden I/O or nondeterminism in library/compiler paths unless the API explicitly owns that boundary. | - | - | Policy row; future behavior changes still require executable tests. |
| RUST-TP-003 | policy | Error policy | RUST-REQ-003 | Human review requires structured failures and stable kind enums for public validators. | - | - | Policy row; individual validators require behavior tests. |
| RUST-TP-004 | policy | Test policy | RUST-REQ-004 | Human review rejects tests that pass by asserting prose, repo shape, incidental output, or implementation detail. | - | - | Mirrors the repo testing rule. |
| RUST-TP-005 | policy | Documentation policy | RUST-REQ-005 | Human review requires docs updates, docs-unchanged verification, or `docs-impact: none` for contract-bearing changes. | - | - | Topic README files remain current-truth only. |
| RUST-TP-006 | planned | Lint ratchet | RUST-REQ-006 | Add scoped lint allowances and then deny library-code footgun lints. | - | - | Planned cleanup slice. |
| RUST-TP-007 | implemented | Dependency gate | RUST-REQ-007 | CI installs pinned cargo-deny and checks both root and provider fixture guest lockfiles under the reviewed advisory, license, ban, and source policy. | cargo_deny_supply_chain_gate_covers_root_and_fixture_guest | deny.toml, .github/workflows/ci.yml, Cargo.lock, fixtures/providers/components/guests/Cargo.lock | This gate does not publish crates. |
| RUST-TP-008 | planned | Fuzzing | RUST-REQ-008 | Add fuzz targets for parser/decoder surfaces. | - | - | Planned hardening slice. |
| RUST-TP-009 | implemented | MSRV guard | RUST-REQ-009 | Cargo metadata reports Rust 1.96 for every workspace package, and the exact CI toolchain label agrees. | workspace_msrv_matches_the_ci_toolchain | Cargo.toml, crates/edict-cli/Cargo.toml, crates/edict-provider-host-wasmtime/Cargo.toml, crates/edict-provider-schema/Cargo.toml, crates/edict-syntax/Cargo.toml, xtask/Cargo.toml, .github/workflows/ci.yml | Required by the pinned Bunny 0.6.0 numeric dependency. |

## Determinism Obligations

- The implemented lint test reads checked-in configuration, not live tool output.
- Policy rows define human review obligations; they do not replace behavior tests
  for software behavior.
- Planned ratchets stay planned until executable checks or CI jobs exist.

## Open Gaps

- Library-code footgun lints are not globally denied yet.
- Fuzz targets are not implemented yet.
