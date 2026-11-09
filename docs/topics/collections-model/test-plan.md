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
| COLL-TP-001 | implemented | Two adds compete for the last slot | COLL-REQ-002, COLL-REQ-003 | First admission succeeds; second conflicts without state changes | competing_adds_conflict_on_the_observed_cardinality | crates/amblicit-collections-model/tests/conformance.rs | Model only; no production concurrency claim |
| COLL-TP-002 | implemented | Empty iteration races insertion | COLL-REQ-002 | Empty reading contains range support; later admission conflicts after insertion | empty_iteration_keeps_range_support_and_detects_insertion | crates/amblicit-collections-model/tests/conformance.rs | Model only; explicit support and state oracles |
| COLL-TP-003 | implemented | Parent removal races child insertion | COLL-REQ-001, COLL-REQ-002 | At most one commits; exact state remains valid in both schedules | parent_removal_and_child_insertion_conflict_in_both_schedules | crates/amblicit-collections-model/tests/conformance.rs | Model only; checks owner and children-range support |
| COLL-TP-004 | implemented | Full collection exchanges one member | COLL-REQ-003 | Final count fits capacity; an oversized candidate refuses without changing state | full_collection_can_exchange_members_atomically, whole_candidate_capacity_refuses_two_adds_into_one_slot | crates/amblicit-collections-model/tests/conformance.rs | Model only; initial omitted-capacity scaffold failed the refusal oracle |
| COLL-TP-005 | implemented | Add/get and remove/get in one operation | COLL-REQ-002 | Reads return the original absence/payload, irrespective of buffered writes | buffered_add_and_remove_do_not_change_reads | crates/amblicit-collections-model/tests/conformance.rs | Model only; final state checked separately from readings |
| COLL-TP-006 | implemented | Aliases mutate one place | COLL-REQ-001, COLL-REQ-003 | Identical and mixed exclusive mutations refuse; different collection fields stay distinct | canonical_aliases_refuse_even_identical_mutations, remove_then_add_is_still_two_exclusive_mutations, same_key_in_different_fields_is_not_an_alias | crates/amblicit-collections-model/tests/conformance.rs | Model only; omitted exclusivity check failed both refusal oracles |
| COLL-TP-007 | planned | Repeated identical ensures | COLL-REQ-005 | One physical obligation remains; work spent on both stays charged | - | - | Independent fixture; no runtime claim |
| COLL-TP-008 | implemented | Remove then re-add across admitted operations | COLL-REQ-001 | Place identity is reused; stale readings are refused despite equal payload | place_reuse_does_not_refresh_old_observations | crates/amblicit-collections-model/tests/conformance.rs | Model only; retained slot versions distinguish place reuse |
| COLL-TP-009 | implemented | Replace an owner's payload | COLL-REQ-001 | Owned members remain unchanged and payload copying creates no ownership | replacing_and_copying_payloads_preserve_structural_ownership | crates/amblicit-collections-model/tests/conformance.rs | Model only; payloads are opaque scalar application data |
| COLL-TP-010 | implemented | Payload key disagrees with storage key | COLL-REQ-001 | Traversal and removal use the storage key and preserve the other slot | entry_keys_are_authoritative_even_when_payloads_name_other_keys | crates/amblicit-collections-model/tests/conformance.rs | Model only; exact reading and final-state oracles |
| COLL-TP-011 | implemented | Move fails at destination | COLL-REQ-001, COLL-REQ-003 | Source membership and stable owner remain intact; no partial changes | failed_move_preserves_source_membership_and_stable_owner | crates/amblicit-collections-model/tests/conformance.rs | Model only; no Jedit planner imported |
| COLL-TP-012 | implemented | Same-value replacement | COLL-REQ-003 | Payload equality holds but prior observation evidence becomes stale | same_value_replacement_still_invalidates_prior_write_evidence | crates/amblicit-collections-model/tests/conformance.rs | Model only; not an Echo write receipt |
| COLL-TP-013 | planned | Nonmatching scans and nested work exhaust bounds | COLL-REQ-002, COLL-REQ-004 | Actual inspections consume shared work; exact bounds pass, one-short refuses | nonmatching_entries_consume_work_and_scan_coverage, owned_traversals_share_one_operation_scan_limit | crates/amblicit-collections-model/tests/conformance.rs | Partial: repeated owned traversals share limits; true nested evaluation remains unimplemented |
| COLL-TP-014 | planned | Multiple failures are simultaneously possible | COLL-REQ-004 | Declared phase and canonical-subject precedence selects a stable outcome | - | - | Independent fixture; no runtime claim |
| COLL-TP-015 | planned | New ensured parent names a new child | COLL-REQ-005 | Complete candidate closure permits it; missing, cyclic, or mistyped links refuse | - | - | Independent fixture; no runtime claim |
| COLL-TP-016 | planned | Attempt mutation of immutable content | COLL-REQ-005 | Every modeled mutation route refuses; different content under one ID is integrity failure | - | - | Independent fixture; no runtime claim |
| COLL-TP-017 | implemented | Remove children and their nonempty owner together | COLL-REQ-001, COLL-REQ-003 | Snapshot NotEmpty refuses in either request order without changing state | removing_children_does_not_satisfy_snapshot_emptiness | crates/amblicit-collections-model/tests/conformance.rs | Model only; candidate emptiness is insufficient |
| COLL-TP-018 | implemented | New owner plus child; empty owner removal plus child | COLL-REQ-001, COLL-REQ-003 | New-owner population is unsupported; owner-survival violation refuses atomically | a_new_owner_cannot_be_populated_in_the_same_candidate, an_empty_owner_cannot_be_removed_while_adding_a_child | crates/amblicit-collections-model/tests/conformance.rs | Model only; separate structured failure kinds |
| COLL-TP-019 | implemented | Absent, unavailable, incomplete, and conflicting observations | COLL-REQ-002, COLL-REQ-004 | Distinct structured outcomes; failed coverage yields no candidate | member_obstructions_do_not_mean_unavailable_observation, nonmatching_entries_consume_work_and_scan_coverage, exhausted_work_cannot_be_used_as_an_absence_witness, empty_iteration_keeps_range_support_and_detects_insertion | crates/amblicit-collections-model/tests/conformance.rs | Model only; unavailable owner is distinct from absent slot |
| COLL-TP-020 | implemented | Filter excludes an inspected payload | COLL-REQ-002 | Changing that payload invalidates the selection evidence | excluded_payloads_still_support_a_filtered_reading | crates/amblicit-collections-model/tests/conformance.rs | Model only; a result filter does not erase observation support |
| COLL-TP-021 | planned | Disable capacity validation | COLL-REQ-006 | Last-slot/exchange fixtures expose an invalid accepted state | - | - | Independent fixture; no runtime claim |
| COLL-TP-022 | planned | Drop empty-range support | COLL-REQ-006 | Empty-read/insertion fixture exposes incorrectly admitted stale selection | - | - | Independent fixture; no runtime claim |
| COLL-TP-023 | planned | Omit owner-survival validation | COLL-REQ-006 | Empty-owner removal plus child produces a counterexample | - | - | Independent fixture; no runtime claim |
| COLL-TP-024 | planned | Bounded generated traces | COLL-REQ-001, COLL-REQ-003, COLL-REQ-006 | Independent state/ownership/capacity invariants; reproducible minimized counterexample on disagreement | - | - | Independent fixture; no runtime claim |
| COLL-TP-025 | implemented | Exact whole-operation work budget | COLL-REQ-004 | Empty-root add costs 12 units; budget 11 refuses at the final evidence update with nine units spent and no state change | whole_operation_budget_includes_atomic_admission | crates/amblicit-collections-model/tests/conformance.rs | Explicit cost oracle specified before the meter; no Echo gas claim |

## Validation boundary

Structural-bound cases are specified before their implementation. V1 admits at
most 64 live members, 64 requests per operation, four owner-path components,
and 128 successful mutating admissions per world. Constructors reject duplicate
places, orphaned members, and over-capacity collections. Model bounds are distinct
from operation work exhaustion; a refused candidate preserves all live state.

| ID | Status | Scenario | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COLL-TP-026 | implemented | Exact structural limits and one beyond | COLL-REQ-001, COLL-REQ-004 | 64 members and requests pass, 65 refuse, depth four passes, depth five refuses before copying | initial_state_preserves_exactly_sixty_four_members, initial_state_refuses_a_sixty_fifth_member, sixty_four_requests_keep_their_observations, a_sixty_fifth_request_refuses_before_evaluation, adding_a_sixty_fifth_live_member_is_atomic_refusal, four_owner_components_are_representable, a_fifth_owner_component_refuses, a_fifth_owner_collection_refuses | crates/amblicit-collections-model/tests/structural_bounds.rs | Fixed small-model envelope, not production limits |
| COLL-TP-027 | implemented | Invalid initial world | COLL-REQ-001, COLL-REQ-003 | Duplicate place, missing owner and collection over capacity return distinct typed failures | initial_state_refuses_duplicate_places, initial_state_refuses_an_orphan, initial_state_refuses_collection_over_capacity, initial_state_accepts_a_child_before_its_owner | crates/amblicit-collections-model/tests/structural_bounds.rs | Constructor input establishes model invariants |
| COLL-TP-028 | implemented | Finite version history | COLL-REQ-002, COLL-REQ-004 | Admission 128 succeeds, 129 refuses without state change, read-only corroboration still succeeds | the_one_hundred_twenty_ninth_mutation_refuses, read_only_admission_survives_exhausted_mutation_history, refused_operations_do_not_consume_mutation_history | crates/amblicit-collections-model/tests/structural_bounds.rs | Refused operations do not advance history |

RED/GREEN evidence must name the actual command, source revision, fixture, and
failure. Setup failures do not establish behavioral RED. Mutation calibration
must show that each broken interpretation is caught by a relevant independent
oracle, rather than by an unrelated setup or parser failure.

Generated traces will have explicit state, step, seed, storage, and wall-time
limits. The required repository gate remains `cargo xtask verify`; local
execution uses the shared guarded Docker worker and project resource budget.
