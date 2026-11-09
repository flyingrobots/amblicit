# Collections reference model: conformance plan

The draft contract is the source of requirements. Fixture expectations are
authored independently of the transition evaluator. Implemented rows below
name executable evidence; the other rows remain planned. The full repository
gate and model conformance are still pending.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| COLL-REQ-001 | planned | Canonical places, payload separation, and stable ownership | docs/design/collections-reference-v1.md |
| COLL-REQ-002 | planned | Fixed-basis evaluation and complete positive/negative support | docs/design/collections-reference-v1.md |
| COLL-REQ-003 | planned | Normalize and validate the whole candidate before atomic admission | docs/design/collections-reference-v1.md |
| COLL-REQ-004 | planned | Bound actual work and retain distinct deterministic outcomes | docs/design/collections-reference-v1.md |
| COLL-REQ-005 | planned | Typed immutable content closure and evidence-preserving ensures | docs/design/collections-reference-v1.md |
| COLL-REQ-006 | planned | Corpus detects omitted capacity, range, and owner guards | docs/design/collections-reference-v1.md |

## Cases

| ID | Status | Scenario | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COLL-TP-001 | planned | Two adds compete for the last slot | COLL-REQ-002, COLL-REQ-003 | First admission succeeds; second conflicts; cardinality never exceeds capacity | - | - | Independent fixture; no runtime claim |
| COLL-TP-002 | planned | Empty iteration races insertion | COLL-REQ-002 | Empty reading contains range support; later admission conflicts after insertion | - | - | Independent fixture; no runtime claim |
| COLL-TP-003 | planned | Parent removal races child insertion | COLL-REQ-001, COLL-REQ-002 | At most one commits; no orphan exists in either schedule | - | - | Independent fixture; no runtime claim |
| COLL-TP-004 | implemented | Full collection exchanges one member | COLL-REQ-003 | Final count fits capacity; an oversized candidate refuses without changing state | full_collection_can_exchange_members_atomically; whole_candidate_capacity_refuses_two_adds_into_one_slot | crates/amblicit-collections-model/tests/conformance.rs | Model only; initial omitted-capacity scaffold failed the refusal oracle |
| COLL-TP-005 | planned | Add/get and remove/get in one operation | COLL-REQ-002 | Reads return the original absence/payload, irrespective of buffered writes | - | - | Independent fixture; no runtime claim |
| COLL-TP-006 | implemented | Aliases mutate one place | COLL-REQ-001, COLL-REQ-003 | Identical and mixed exclusive mutations refuse; different collection fields stay distinct | canonical_aliases_refuse_even_identical_mutations; remove_then_add_is_still_two_exclusive_mutations; same_key_in_different_fields_is_not_an_alias | crates/amblicit-collections-model/tests/conformance.rs | Model only; omitted exclusivity check failed both refusal oracles |
| COLL-TP-007 | planned | Repeated identical ensures | COLL-REQ-005 | One physical obligation remains; work spent on both stays charged | - | - | Independent fixture; no runtime claim |
| COLL-TP-008 | planned | Remove then re-add across admitted operations | COLL-REQ-001 | Place identity is reused; stale readings are refused despite equal payload | - | - | Independent fixture; no runtime claim |
| COLL-TP-009 | planned | Replace an owner's payload | COLL-REQ-001 | Owned members remain unchanged and payload copying creates no ownership | - | - | Independent fixture; no runtime claim |
| COLL-TP-010 | planned | Payload key disagrees with storage key | COLL-REQ-001 | Traversal and removal use the storage key and preserve the other slot | - | - | Independent fixture; no runtime claim |
| COLL-TP-011 | planned | Move fails at destination | COLL-REQ-001, COLL-REQ-003 | Source membership and stable owner remain intact; no partial changes | - | - | Independent fixture; no runtime claim |
| COLL-TP-012 | planned | Same-value replacement | COLL-REQ-003 | Payload equality holds but write/attempt evidence advances | - | - | Independent fixture; no runtime claim |
| COLL-TP-013 | planned | Nonmatching scans and nested work exhaust bounds | COLL-REQ-002, COLL-REQ-004 | Actual inspections consume shared work; exact bounds pass, one-short refuses | - | - | Independent fixture; no runtime claim |
| COLL-TP-014 | planned | Multiple failures are simultaneously possible | COLL-REQ-004 | Declared phase and canonical-subject precedence selects a stable outcome | - | - | Independent fixture; no runtime claim |
| COLL-TP-015 | planned | New ensured parent names a new child | COLL-REQ-005 | Complete candidate closure permits it; missing, cyclic, or mistyped links refuse | - | - | Independent fixture; no runtime claim |
| COLL-TP-016 | planned | Attempt mutation of immutable content | COLL-REQ-005 | Every modeled mutation route refuses; different content under one ID is integrity failure | - | - | Independent fixture; no runtime claim |
| COLL-TP-017 | planned | Remove children and their nonempty owner together | COLL-REQ-001, COLL-REQ-003 | Snapshot NotEmpty still refuses the complete candidate | - | - | Independent fixture; no runtime claim |
| COLL-TP-018 | planned | New owner plus child; empty owner removal plus child | COLL-REQ-001, COLL-REQ-003 | New-owner population is unsupported; owner-survival violation refuses atomically | - | - | Independent fixture; no runtime claim |
| COLL-TP-019 | planned | Absent, unavailable, incomplete, and conflicting observations | COLL-REQ-002, COLL-REQ-004 | Distinct structured outcomes; only established absence authorizes add | - | - | Independent fixture; no runtime claim |
| COLL-TP-020 | planned | Filter excludes an inspected payload | COLL-REQ-002 | Changing that payload invalidates the selection evidence | - | - | Independent fixture; no runtime claim |
| COLL-TP-021 | planned | Disable capacity validation | COLL-REQ-006 | Last-slot/exchange fixtures expose an invalid accepted state | - | - | Independent fixture; no runtime claim |
| COLL-TP-022 | planned | Drop empty-range support | COLL-REQ-006 | Empty-read/insertion fixture exposes incorrectly admitted stale selection | - | - | Independent fixture; no runtime claim |
| COLL-TP-023 | planned | Omit owner-survival validation | COLL-REQ-006 | Empty-owner removal plus child produces a counterexample | - | - | Independent fixture; no runtime claim |
| COLL-TP-024 | planned | Bounded generated traces | COLL-REQ-001, COLL-REQ-003, COLL-REQ-006 | Independent state/ownership/capacity invariants; reproducible minimized counterexample on disagreement | - | - | Independent fixture; no runtime claim |

## Validation boundary

RED/GREEN evidence must name the actual command, source revision, fixture, and
failure. Setup failures do not establish behavioral RED. Mutation calibration
must show that each broken interpretation is caught by a relevant independent
oracle, rather than by an unrelated setup or parser failure.

Generated traces will have explicit state, step, seed, storage, and wall-time
limits. The required repository gate remains `cargo xtask verify`; local
execution uses the shared guarded Docker worker and project resource budget.
