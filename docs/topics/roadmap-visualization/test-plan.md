# Roadmap Visualization Test Plan

This plan maps the shipped snapshot builder and viewer to their executable
Python and browser checks. It does not claim implementation of the collections,
compiler, provider, or rope tasks displayed by the viewer.

## Evidence registration boundary

The checks below exist and execute. Their rows use `gap` for automatic
contract-graph evidence registration because `cargo xtask contract-check`
recognizes Rust test functions only. This follows the existing
[Python evidence convention](../tests/test-plan.md#study-validation-snapshot).
The gap does not mean the viewer or its checks are unimplemented, and the rows
are not `policy` substitutes for software tests. Run the named commands
separately; `cargo xtask verify` does not execute Python or Playwright.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| RDAG-REQ-001 | gap | Snapshot validation admits only bound, unique, nonreflexive, acyclic prerequisite edges; the viewer preserves their direction and does not add hierarchy or synthetic edges. | docs/design/collections-optics-dag/build.py, docs/design/collections-optics-dag/verify.mjs |
| RDAG-REQ-002 | gap | Analysis produces a complete MECE workstream partition, earliest antichain waves, an exact-width antichain and chain-cover certificate, and a direct longest path with task/edge units. | docs/design/collections-optics-dag/build.py |
| RDAG-REQ-003 | gap | The standalone page supports project colors, task focus, search, a text list, export, keyboard selection, pan/zoom, narrow screens, and light/dark themes without network requests. | docs/design/collections-optics-dag/page.template.html |
| RDAG-REQ-004 | gap | The in-memory readiness preview enables a task only after its own assumed predecessors finish, without a global wave barrier, resource admission, or tracker writes. | docs/design/collections-optics-dag/page.template.html |
| RDAG-REQ-005 | gap | The fixed snapshot and template deterministically regenerate the checked-in HTML; check mode detects stale generated output without rewriting it. | docs/design/collections-optics-dag/build.py |
| RDAG-REQ-006 | gap | Completing a simulated task retains keyboard focus in its card; window resizing preserves a reader-adjusted map until Fit or Reset; duplicate supplied GitHub issue identities reject. | docs/design/collections-optics-dag/page.template.html, docs/design/collections-optics-dag/build.py |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| docs/design/collections-optics-dag/snapshot.json | Dated native GitHub prerequisite graph plus editorial labels and ownership. | The captured graph contains 22 nodes and 32 edges; the browser must render the same oriented edge set. |
| docs/design/collections-optics-dag/verify_analysis.py | Certificate checks and bounded invalid-input variants derived from the snapshot. | Independent reachability validates every antichain, chain-cover relation, wave ordering, and direct longest-path edge; duplicate or missing ownership and cycles/resource edges reject. |
| docs/design/collections-optics-dag/verify.mjs | Offline browser interaction scenarios over the generated artifact. | Rendered edges, eligible controls, exported data, visible task sets, and viewport geometry match the scenario's assertions. |
| docs/design/collections-optics-dag/page.template.html | Editable page source. | A single data insertion point receives escaped snapshot JSON. |
| docs/design/collections-optics-dag/index.html | Generated standalone page. | Build check matches regeneration; the browser executes without HTTP requests or JavaScript errors. |

The small width counterexample is constructed in
`test_generation_width_is_not_poset_width`: A precedes B and C while D is
independent. Its largest generation has size two and its maximum antichain has
size three. This is separate from the captured roadmap's eight-versus-nine
result.

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| RDAG-TP-001 | gap | Invalid graph | RDAG-REQ-001 | Adding a backward cycle or resource-contention edge raises a validation error before rendering. | test_cycles_and_nonprecedence_edges_are_rejected | docs/design/collections-optics-dag/verify_analysis.py | Python execution; automatic Rust evidence-name registration remains a gap. |
| RDAG-TP-002 | gap | Independent certificates | RDAG-REQ-002 | Every transitive predecessor is in an earlier wave; a nine-member antichain and nine-chain covering partition agree; the nine-task longest path uses actual direct edges. | test_partition_antichains_and_width_certificates, test_generation_width_is_not_poset_width | docs/design/collections-optics-dag/verify_analysis.py, docs/design/collections-optics-dag/snapshot.json | The oracle computes reachability independently of the builder's matching algorithm. |
| RDAG-TP-003 | gap | Ownership rejection | RDAG-REQ-002 | Duplicate workstream membership and omitted task ownership both reject. | test_overlapping_workstream_is_rejected, test_unassigned_task_is_rejected | docs/design/collections-optics-dag/verify_analysis.py | Each captured executable task has exactly one workstream owner. |
| RDAG-TP-004 | gap | Browser graph and interaction | RDAG-REQ-001, RDAG-REQ-003 | The DOM and JSON export contain exactly the snapshot edges; focus never invents edges; colors, search/list, keyboard, zoom/pan, themes and mobile bounds meet assertions in an offline context. | node docs/design/collections-optics-dag/verify.mjs | docs/design/collections-optics-dag/verify.mjs, docs/design/collections-optics-dag/index.html, docs/design/collections-optics-dag/snapshot.json | Existing Playwright and compatible Chromium are required; the check installs nothing. |
| RDAG-TP-005 | gap | Rolling eligibility | RDAG-REQ-004 | Assuming publication finished enables its consumers while unrelated first-wave work remains; list-runtime stays blocked until both prerequisites finish; the maximum-frontier example exposes nine eligible tasks. | node docs/design/collections-optics-dag/verify.mjs | docs/design/collections-optics-dag/verify.mjs, docs/design/collections-optics-dag/index.html | Completion is an in-memory assumption, not accepted runtime evidence. |
| RDAG-TP-006 | gap | Generated artifact | RDAG-REQ-005 | Check mode compares regenerated text against the retained HTML and exits nonzero on a mismatch. | python3 docs/design/collections-optics-dag/build.py --check | docs/design/collections-optics-dag/build.py, docs/design/collections-optics-dag/page.template.html, docs/design/collections-optics-dag/snapshot.json, docs/design/collections-optics-dag/index.html | This command verifies freshness; it does not refresh the source graph or supply a cross-platform canonical serialization profile. |
| RDAG-TP-007 | gap | Additional invalid endpoints | RDAG-REQ-001 | Duplicate, self-referential, and unbound edges each raise ValueError before rendering. | test_invalid_endpoints_are_rejected | docs/design/collections-optics-dag/verify_analysis.py | Added coverage of existing guards; automatic Rust evidence-name registration remains a gap. |
| RDAG-TP-008 | gap | Interaction continuity | RDAG-REQ-006 | Keyboard completion focuses the same card's enabled task control; resizing retains a panned or zoomed transform; Fit restores automatic resizing. | node docs/design/collections-optics-dag/verify.mjs | docs/design/collections-optics-dag/verify.mjs, docs/design/collections-optics-dag/index.html | Both focus and zoom assertions failed before their implementation fixes and pass afterward. |
| RDAG-TP-009 | gap | Unique issue identity | RDAG-REQ-006 | Two distinct task IDs with the same supplied issueId raise ValueError; synthetic nodes may omit issueId. | test_duplicate_issue_identity_is_rejected | docs/design/collections-optics-dag/verify_analysis.py | Failed before the guard and passes afterward; the four-node width fixture covers omitted issueId. |
| RDAG-TP-010 | gap | Cross-workstream contracts | RDAG-REQ-002 | The reported cross-workstream edge list exactly matches captured edges with different workstream owners, without omissions or duplicates. | test_cross_workstream_edges_match_owners | docs/design/collections-optics-dag/verify_analysis.py, docs/design/collections-optics-dag/snapshot.json | Added coverage of existing behavior; not a new algorithm claim. |

## Commands and coverage

From the repository root:

```sh
python3 docs/design/collections-optics-dag/build.py --check
python3 docs/design/collections-optics-dag/verify_analysis.py
node docs/design/collections-optics-dag/verify.mjs
```

The [artifact guide](../../design/collections-optics-dag/README.md) documents
the optional Playwright module, Chromium executable, and screenshot-directory
environment variables. The check bounds launch to 30 seconds; a separate
one-minute timer closes the browser after launch. These commands are explicit
companion checks to the normal local
gate, not commands that the Rust evidence-name checker executes.

## Known gaps

- Non-Rust test references are not automatically registered by the topic
  checker, and these companion checks are not scheduled by the Rust CI jobs.
- The browser suite exercises the captured graph; it is not a general browser
  accessibility certification or a test of arbitrary imported graph sizes.
- There is no resource-feasible scheduler, duration forecast, completion
  authority, tracker writer, or compiler/runtime acceptance implementation in
  this viewer.
- Native tracker edges are dated observations. Their presence does not
  independently prove semantic necessity, and their absence does not prove
  independence.
