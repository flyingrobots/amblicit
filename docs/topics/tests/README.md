# Testing Workflow Topic

Status: current HEAD workflow contract.

This shelf defines how Edict organizes tests, fixtures, and RED/GREEN evidence.
It applies to humans and agents. It does not replace the domain-specific topic
test plans; it describes the shared workflow those plans use.

## Public Entry Points

The testing workflow is referenced from the repo entry points:

- [AGENTS.md](../../../AGENTS.md), for agent instructions.
- [CONTRIBUTING.md](../../../CONTRIBUTING.md), for contributor instructions.

Both entry points point back to this shelf so RED/GREEN practice has one durable
home. [TESTS-REQ-001]

## RED/GREEN Contract

For every nontrivial behavior, contract, workflow, release, schema, validation,
or public-surface change, Edict expects a visible RED/GREEN cycle:

- update the owning topic `test-plan.md` with planned requirement and case rows;
- write the deterministic test, fixture, doctest, or contract check before the
  implementation;
- run the narrowest relevant command and observe the RED failure;
- implement the smallest coherent change that turns that test GREEN;
- mark planned rows as implemented only when executable evidence exists;
- report the RED and GREEN commands in the final report or pull request body.
  [TESTS-REQ-002]

Purely mechanical edits do not need synthetic RED/GREEN work. When a change has
no contract impact, the final report or pull request body should say why no
topic shelf or RED/GREEN cycle was required. [TESTS-REQ-002]

## Test Plans

Topic `test-plan.md` files are the planning ledger for tests. A planned row may
describe intended evidence before code exists, but an implemented row must name
executable evidence and fixtures that exist in the repository. [TESTS-REQ-003]

Policy-only rows use `policy` status. They record human-review workflow
contracts that deliberately should not be turned into Rust tests because doing
so would assert documentation detail rather than software behavior.
[TESTS-REQ-003] [TESTS-REQ-004]

Tests assert structured behavior and stable artifacts. Negative tests should
assert stable error kinds or structured values, not `is_err()` alone or
diagnostic prose. [TESTS-REQ-004]

Tests must assert software behavior. They must not assert implementation detail,
documentation detail, or repository structure. A test may exercise documentation
tooling behavior, such as `contract-check` rejecting an invalid fixture path or
unknown evidence name, but it must not pass merely because prose contains a
phrase or a file appears at a particular path. [TESTS-REQ-004]

## Fixtures

Fixtures should be reused across stages when the semantic artifact is the same:

- source fixtures in `fixtures/lang/` drive parser, surface-validation,
  compiler-spine, and future canonicalization tests when the lowerable subset
  supports them;
- Core schema fixtures in `fixtures/core/schema/` prove accepted and rejected
  schema shapes;
- future canonical fixtures under `fixtures/core/` should separate executable
  encoder behavior from reviewed golden bytes and exact digests. [TESTS-REQ-005]

For v0.3, the expected sequence is:

- #21 adds executable canonical encoding behavior without freezing reviewed
  digest goldens;
- #22 freezes reviewed golden bytes and exact digests produced by that encoder.
  [TESTS-REQ-005]

## Verification

`cargo xtask contract-check` validates topic-shelf metadata, status values,
local links, implemented evidence names, and fixture paths. `cargo xtask verify`
runs the full local gate. [TESTS-REQ-006]

The local gate schedules one default Cargo workspace test pass. Cargo's default
test selection covers unit, integration, and documentation tests, so the gate
does not repeat workspace doctests as a second pass. [TESTS-REQ-007]

The verification matrix is tracked in [test-plan.md](./test-plan.md).

## Study validation snapshots

The retained study helper overlays current files onto a committed snapshot. It
combines HEAD-relative deletions (with rename detection disabled) and missing
index paths, so staged deletions, rename sources, and added-then-missing files
cannot reappear in the validation source. The executable snapshot regression
uses an isolated Git fixture and checks the resulting file set and bytes:

```text
python3 docs/plans/study-feedback/evidence/snapshot-deletion-regression.py.txt
```

Run this probe inside the bounded Docker validation worker. It verifies the
retained study helper; it does not establish a general snapshot service.
