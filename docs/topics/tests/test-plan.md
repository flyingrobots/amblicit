# Testing Workflow Test Plan

Status: current verification design for the repo testing workflow.

This plan verifies that the RED/GREEN workflow is discoverable, test-plan
driven, and connected to executable contract checks.

## Scope

In scope:

- contributor and agent entry-point references to this testing workflow shelf;
- RED/GREEN sequencing for nontrivial contract changes;
- topic test-plan rows as the planning ledger for requirements, cases, oracles,
  evidence, and fixtures;
- stable structured assertions for tests;
- fixture reuse across parser, validation, compiler-spine, canonicalization, and
  golden-artifact stages;
- local verification through `cargo xtask contract-check` and
  `cargo xtask verify`; and
- nonduplicative scheduling of the default Cargo workspace test pass.

Out of scope:

- enforcing every RED/GREEN observation automatically;
- prescribing a single test file layout for every future crate;
- replacing domain-specific topic test plans.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| TESTS-REQ-001 | policy | Agent and contributor entry points reference the testing workflow shelf. | AGENTS.md, CONTRIBUTING.md |
| TESTS-REQ-002 | policy | Nontrivial contract changes use a visible RED/GREEN cycle before implementation is claimed complete. | AGENTS.md, CONTRIBUTING.md |
| TESTS-REQ-003 | policy | Topic test plans are the ledger for implemented, planned, gap, and policy requirements, cases, oracles, evidence, and fixtures. | AGENTS.md, docs/topics/README.md |
| TESTS-REQ-004 | policy | Tests assert software behavior and stable artifacts, not implementation details, documentation details, repository structure, diagnostic prose, or incidental output. | AGENTS.md |
| TESTS-REQ-005 | policy | Fixtures are reused across compatible stages, while executable encoder behavior and reviewed golden bytes remain separate steps. | fixtures/README.md, ROADMAP.md |
| TESTS-REQ-006 | policy | Local verification includes topic contract checking and the full `cargo xtask verify` gate. | AGENTS.md, xtask/src/main.rs, xtask/src/contract_check.rs, xtask/src/tests.rs |
| TESTS-REQ-007 | implemented | The local gate schedules one default Cargo workspace test pass, which covers unit, integration, and documentation tests without repeating doctests. | xtask/src/main.rs, xtask/src/tests.rs |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| AGENTS.md | Agent-facing workflow entry point. | References the testing workflow topic and RED/GREEN rule. |
| CONTRIBUTING.md | Human-facing contribution entry point. | References the same testing workflow topic and local verification gate. |
| docs/topics/tests/README.md | Canonical testing workflow contract. | Describes the RED/GREEN contract, fixture reuse, and verification commands. |
| docs/topics/README.md | Topic index. | Lists the testing workflow topic. |
| fixtures/README.md | Fixture corpus contract. | Defines positive, negative, and golden fixture roles. |
| xtask/src/main.rs, xtask/src/contract_check.rs, xtask/src/tests.rs | Local verification implementation. | Provides `verify` dispatch, `contract-check` behavior, and harness tests; tests cover tool behavior, not policy prose. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TESTS-TP-001 | policy | Policy discovery | TESTS-REQ-001 | Review confirms the workflow policy is discoverable from contributor and agent entry points. | - | AGENTS.md, CONTRIBUTING.md, docs/topics/README.md | Documentation detail; do not encode as a Rust test. |
| TESTS-TP-002 | policy | Workflow policy | TESTS-REQ-002, TESTS-REQ-003, TESTS-REQ-004 | Review confirms the workflow policy requires RED/GREEN and behavior-level tests only. | - | docs/topics/tests/README.md, docs/topics/tests/test-plan.md | Policy detail; do not encode as a Rust test. |
| TESTS-TP-003 | policy | Fixture policy | TESTS-REQ-005 | Review confirms the workflow policy keeps executable encoder behavior separate from reviewed golden bytes and exact digests. | - | docs/topics/tests/README.md, ROADMAP.md, fixtures/README.md | Policy detail; do not encode as a Rust test. |
| TESTS-TP-004 | policy | Local gate policy | TESTS-REQ-006 | Review confirms the local gate is documented; executable tests remain focused on validator behavior. | - | xtask/src/main.rs, xtask/src/contract_check.rs, xtask/src/tests.rs | Tool behavior is covered by existing `contract_graph_*` tests. |
| TESTS-TP-005 | implemented | Verification scheduling | TESTS-REQ-007 | An injected command recorder observes exactly one default `cargo test --workspace --all-features` invocation. | verify_rust_commands_runs_one_default_workspace_test_pass | xtask/src/main.rs, xtask/src/tests.rs | Proves workspace doctests are not scheduled a second time by xtask. |

## Determinism Obligations

- Workflow policy is reviewed as documentation, not asserted through prose
  string tests.
- Executable tests assert software behavior, such as validator acceptance or
  rejection of structured input.
- Link and evidence resolution behavior is covered by existing `contract_graph_*`
  tests.

## Open Gaps

- No automated tool can prove that a contributor actually observed RED before
  implementation. The report or pull request body remains the evidence for that
  sequencing.

## Study Validation Snapshot

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| TESTS-REQ-008 | gap | The retained study validation helper overlays current source on its committed snapshot without resurrecting staged or unstaged deletions. | docs/plans/study-feedback/evidence/guard-runner.py.txt |

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TESTS-TP-008 | gap | Snapshot fidelity | TESTS-REQ-008 | A real Git snapshot matches modified and untracked bytes, removes staged/unstaged deletions and old rename paths, and omits an added-then-missing path. | docs/plans/study-feedback/evidence/snapshot-deletion-regression.py.txt | docs/plans/study-feedback/evidence/snapshot-deletion-regression.py.txt | Python probe passes in Docker; registration remains a gap because the contract checker recognizes only Rust test functions. |

The Python snapshot probe is executable and has observed RED/GREEN evidence.
REQ-008 and TP-008 remain `gap` for automatic contract-graph evidence registration;
this does not label the source correction unimplemented or claim Rust coverage.
