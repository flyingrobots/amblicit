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
| COLL-REQ-007 | planned | Versioned artifact shapes, exact semantic binding, and feature agreement confer no authority | docs/design/collections-wire-v1.md |

## Cases

| ID | Status | Scenario | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COLL-TP-001 | implemented | Two adds compete for the last slot | COLL-REQ-002, COLL-REQ-003 | First admission succeeds; second conflicts without state changes | competing_adds_conflict_on_the_observed_cardinality | crates/amblicit-collections-model/tests/conformance.rs | Model only; no production concurrency claim |
| COLL-TP-002 | implemented | Empty iteration races insertion | COLL-REQ-002 | Empty reading contains range support; later admission conflicts after insertion | empty_iteration_keeps_range_support_and_detects_insertion | crates/amblicit-collections-model/tests/conformance.rs | Model only; explicit support and state oracles |
| COLL-TP-003 | implemented | Parent removal races child insertion | COLL-REQ-001, COLL-REQ-002 | At most one commits; exact state remains valid in both schedules | parent_removal_and_child_insertion_conflict_in_both_schedules | crates/amblicit-collections-model/tests/conformance.rs | Model only; checks owner and children-range support |
| COLL-TP-004 | implemented | Full collection exchanges one member | COLL-REQ-003 | Final count fits capacity; an oversized candidate refuses without changing state | full_collection_can_exchange_members_atomically, whole_candidate_capacity_refuses_two_adds_into_one_slot | crates/amblicit-collections-model/tests/conformance.rs | Model only; initial omitted-capacity scaffold failed the refusal oracle |
| COLL-TP-005 | implemented | Add/get and remove/get in one operation | COLL-REQ-002 | Reads return the original absence/payload, irrespective of buffered writes | buffered_add_and_remove_do_not_change_reads | crates/amblicit-collections-model/tests/conformance.rs | Model only; final state checked separately from readings |
| COLL-TP-006 | implemented | Aliases mutate one place | COLL-REQ-001, COLL-REQ-003 | Identical and mixed exclusive mutations refuse; different collection fields stay distinct | canonical_aliases_refuse_even_identical_mutations, remove_then_add_is_still_two_exclusive_mutations, same_key_in_different_fields_is_not_an_alias | crates/amblicit-collections-model/tests/conformance.rs | Model only; omitted exclusivity check failed both refusal oracles |
| COLL-TP-007 | implemented | Repeated identical ensures | COLL-REQ-005 | One physical obligation remains, work spent on both stays charged, conflicting content refuses | identical_ensures_coalesce_into_one_obligation, identical_ensures_keep_the_work_spent_on_both_requests, conflicting_ensures_do_not_normalize | crates/amblicit-collections-model/tests/immutable_content.rs | Symbolic content model only |
| COLL-TP-008 | implemented | Remove then re-add across admitted operations | COLL-REQ-001 | Place identity is reused; stale readings are refused despite equal payload | place_reuse_does_not_refresh_old_observations | crates/amblicit-collections-model/tests/conformance.rs | Model only; retained slot versions distinguish place reuse |
| COLL-TP-009 | implemented | Replace an owner's payload | COLL-REQ-001 | Owned members remain unchanged and payload copying creates no ownership | replacing_and_copying_payloads_preserve_structural_ownership | crates/amblicit-collections-model/tests/conformance.rs | Model only; payloads are opaque scalar application data |
| COLL-TP-010 | implemented | Payload key disagrees with storage key | COLL-REQ-001 | Traversal and removal use the storage key and preserve the other slot | entry_keys_are_authoritative_even_when_payloads_name_other_keys | crates/amblicit-collections-model/tests/conformance.rs | Model only; exact reading and final-state oracles |
| COLL-TP-011 | implemented | Move fails at destination | COLL-REQ-001, COLL-REQ-003 | Source membership and stable owner remain intact; no partial changes | failed_move_preserves_source_membership_and_stable_owner | crates/amblicit-collections-model/tests/conformance.rs | Model only; no Jedit planner imported |
| COLL-TP-012 | implemented | Same-value replacement | COLL-REQ-003 | Payload equality holds but prior observation evidence becomes stale | same_value_replacement_still_invalidates_prior_write_evidence | crates/amblicit-collections-model/tests/conformance.rs | Model only; not an Echo write receipt |
| COLL-TP-013 | implemented | Nonmatching scans and nested work exhaust bounds | COLL-REQ-002, COLL-REQ-004 | Actual inspections consume shared work, exact bounds pass, one-short refuses | nested_traversals_charge_all_inspections, nested_work_does_not_reset_the_operation_budget, nested_scan_coverage_is_shared | crates/amblicit-collections-model/tests/nested_evaluation.rs | Nonmatching scan work persists across group boundaries, 13 units succeed and 12 refuse |
| COLL-TP-014 | implemented | Multiple failures are simultaneously possible | COLL-REQ-004 | Declared phase and canonical-subject precedence selects a stable outcome; whole-operation attribution survives grouping | duplicate_attribution_uses_the_least_place_in_either_request_order, domain_order_is_place_order_instead_of_request_or_kind_order, conflict_reports_the_first_canonical_support_fact_before_domains, integrity_precedes_missing_closure_and_names_the_conflicting_id | crates/amblicit-collections-model/tests/failure_attribution.rs | Small public-API fixtures; explicit kind/subject oracles, not compiler source spans |
| COLL-TP-015 | implemented | New ensured parent names a new child | COLL-REQ-005 | Complete candidate closure permits either order, missing, cyclic, or mistyped links refuse | a_new_parent_can_precede_its_new_child, a_new_child_can_precede_its_new_parent, a_missing_child_refuses_the_complete_proposal, a_content_cycle_refuses, a_child_of_the_wrong_kind_refuses | crates/amblicit-collections-model/tests/immutable_content.rs | Symbolic typed closure, not production hashing |
| COLL-TP-016 | implemented | Attempt mutation of immutable content | COLL-REQ-005 | Mutable place operations cannot address content, different content or kind under one ID is integrity failure | content_kind_cannot_change_under_one_identity, mutable_place_operations_do_not_address_content, conflicting_concurrent_ensures_refuse_the_second_admission | crates/amblicit-collections-model/tests/content_admission.rs | Separate immutable namespace, append-only publication |
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

| ID | Status | Scenario | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COLL-TP-035 | implemented | Negotiate required and offered collection contract headers | COLL-REQ-007 | Exact v1/version/semantic digest match, canonical feature sets, traversal dependency, explicit refusal of unsupported features; agreement is metadata only | matching_unknown_versions_are_still_unsupported, equal_versions_do_not_hide_different_semantics, a_known_but_unoffered_feature_refuses, malformed_required_header_precedes_malformed_offer | crates/amblicit-collections-model/tests/contract_agreement.rs | Pure reference agreement; production codec and target admission belong to downstream issues |

The 22 contract-agreement fixtures were written against a no-check scaffold;
all 22 failed at behavior assertions before implementation. The completed checker
passes them alongside the 98 prior model cases. Its output is checked metadata,
not a decoded wire body, published semantic digest, observation capability,
candidate, or execution receipt. It has no world/provider/I/O access.
Nine selected mutations were each detected by their designated behavior
assertions: accepting an unknown version or semantic coordinate, ignoring the
semantic digest, ignoring feature order, omitting traversal's prerequisite,
checking the offer first, reversing missing-feature selection, granting unused
offered features, and ignoring unknown features. The source was restored and
the package cache invalidated before the 120-case suite and strict Clippy passed.
Verbatim output, the finite TypeScript driver, and exact before/after sources
are retained outside Git. This does not calibrate the downstream byte codec.

Failure-attribution fixtures specify logical subjects and phase precedence
before implementation. The initial scaffold retained the old decisions and
reported every subject as `Operation`: 19 assertions failed and the foreign-basis
control passed. The normalized-duplicate work case also failed on its category:
an early duplicate hid the work required to select the canonical duplicate.
Twenty public-API cases now pass. Their logical subject is attached to the
whole-operation refusal; no compiler source-span adapter is present.

Twelve bounded calibration mutations were each detected by their designated
behavior assertion: first duplicate instead of least duplicate, ensure before
exclusive normalization, largest conflicting content ID, reversed domain order,
content before membership, domains before corroboration, omitted origin check,
largest overfull collection, authored edge order, capacity before owner survival,
closure before integrity, and reversed support order. Each run cleaned only this
crate's shared target artifacts before compiling. Original sources were restored
and all 98 focused cases plus strict Clippy passed afterward. This calibration
covers the selected failure-precedence rules, not the still-planned capacity,
empty-range, ownership, generated-trace, or deeper content calibration work.

Verbatim evidence, exact source hashes, original/mutated source pairs, and the
finite TypeScript driver are retained outside Git. The initial calibration
driver failed during Node type stripping, before executing a model test; that
setup failure is retained separately and is not behavioral RED. The successful
focused command is `cargo +1.96.0 test -p amblicit-collections-model --test
failure_attribution --locked`; the restored suite uses `--tests --locked`.

Immutable-content fixtures use the symbolic-ID contract and exact costs
in `collections-reference-v1.md`. A leaf ensure costs six, two identical leaf
ensures cost nine and publish one node, and a new one-edge parent plus leaf costs
15. Missing, mistyped, cyclic, conflicting, oversized, and stale-basis proposals
must refuse without changing mutable or immutable state. Weak names stay inert.

| ID | Status | Scenario | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COLL-TP-031 | implemented | Content link bound and whole-operation work | COLL-REQ-004, COLL-REQ-005 | A fifth link refuses before copying, exact cost succeeds and one-short refuses atomically | a_fifth_content_link_refuses_before_copying, a_leaf_ensure_uses_six_work_units, a_leaf_ensure_refuses_one_unit_short_without_publication | crates/amblicit-collections-model/tests/immutable_content.rs | Catalog and history bounds are covered by COLL-TP-034 |
| COLL-TP-032 | implemented | Content closure stays on the captured basis | COLL-REQ-002, COLL-REQ-005 | Later child insertion cannot repair a missing child, an established typed child supports a parent, identical concurrent ensures publish once, conflicting ensures refuse | later_child_insertion_does_not_repair_a_missing_basis_child, an_existing_typed_child_can_support_a_new_parent, identical_concurrent_ensures_publish_once, conflicting_concurrent_ensures_refuse_the_second_admission | crates/amblicit-collections-model/tests/content_admission.rs | Immutable state only grows through validated ensures |
| COLL-TP-033 | implemented | Joint mutable and immutable admission | COLL-REQ-003, COLL-REQ-005 | A failure in either namespace prevents publication in both, weak names do not read mutable slots | a_mutable_capacity_failure_prevents_content_publication, a_content_failure_prevents_mutable_publication, a_mutable_name_is_inert_immutable_data | crates/amblicit-collections-model/tests/immutable_content.rs | No intermediate application-state changes |
| COLL-TP-034 | implemented | Content storage and history bounds | COLL-REQ-004, COLL-REQ-005 | Four links and 64 nodes fit, a 65th node refuses, exhausted history permits existing identical content but refuses new publication | four_content_links_are_admitted, sixty_four_content_nodes_fit, a_sixty_fifth_live_content_node_refuses, an_existing_ensure_can_be_corroborated_after_history_exhaustion, new_content_refuses_after_history_exhaustion | crates/amblicit-collections-model/tests/content_admission.rs | Storage is bounded independently of mutable membership |

The first immutable-content RED run failed 13 behavior assertions and passed the
mutable-capacity control. The publication suite's initial RED run stopped at its
11 seed-publication assertions. Those failures demonstrate the missing public
ensure path, not calibration of deeper concurrent/closure assertions. All 25
content cases now execute their assertions successfully; targeted mutation
calibration of the deeper assertions remains required before completion.

Nested evaluation uses explicit request groups. Groups retain the same basis,
work meter, scan count, and whole-candidate normalizer. Each group entry costs
one work unit; group nodes count toward the 64-request envelope. Four nested
groups are admitted, a fifth refuses before evaluation. For two traversals of
one root member under two group nodes, the first rejecting the payload and the
second selecting it, preparation costs 11 and corroboration costs two. Budget
13 succeeds, 12 refuses, and a shared scan limit of one refuses as incomplete.

| ID | Status | Scenario | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COLL-TP-030 | implemented | Nested candidates preserve operation scope | COLL-REQ-002, COLL-REQ-003, COLL-REQ-004 | Nested reads use the original snapshot, aliases refuse globally, full-collection exchange uses final count, all nested nodes share bounds | nested_reads_ignore_buffered_writes, exclusive_aliases_across_groups_refuse, nested_exchange_uses_whole_candidate_capacity, four_groups_preserve_the_leaf_observation, a_fifth_group_refuses_before_evaluation, sixty_four_nodes_include_group_overhead, nested_nodes_cannot_hide_a_sixty_fifth_request | crates/amblicit-collections-model/tests/nested_evaluation.rs | Reference request groups do not add a compiler language feature |

| ID | Status | Scenario | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COLL-TP-029 | implemented | Different worlds reuse numeric versions | COLL-REQ-002 | Candidate from another world refuses even when membership or versions are equal, same-world snapshot clones remain admissible | a_reading_from_another_world_cannot_be_corroborated, equal_membership_does_not_transfer_mutation_authority, an_empty_candidate_still_belongs_to_its_world, cloning_a_snapshot_preserves_its_origin | crates/amblicit-collections-model/tests/basis_binding.rs | Model origin binding is distinct from production wire identity |

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
