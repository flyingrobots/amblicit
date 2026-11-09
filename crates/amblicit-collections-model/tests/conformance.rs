use amblicit_collections_model::{
    prepare, Action, Collection, FailureKind, Limits, Payload, Place, Predicate, Reading, Support,
    World,
};

// Expectations are specified directly from the contract, not by a second call
// to the evaluator or by a serialized copy of its output.
#[test]
fn whole_candidate_capacity_refuses_two_adds_into_one_slot() {
    let mut world = World::new(1, []);
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[
            Action::Add(Place::root(0, 1), Payload(10)),
            Action::Add(Place::root(0, 2), Payload(20)),
        ],
        Limits::default(),
    )
    .unwrap();
    let failure = world.admit(candidate).unwrap_err();
    assert_eq!(failure.kind, FailureKind::Capacity);
    assert_eq!(world.snapshot(), before);
}

#[test]
fn full_collection_can_exchange_members_atomically() {
    let old = Place::root(0, 1);
    let new = Place::root(0, 2);
    let mut world = World::new(1, [(old.clone(), Payload(10))]);
    let candidate = prepare(
        &world.snapshot(),
        &[Action::Add(new.clone(), Payload(20)), Action::Remove(old)],
        Limits::default(),
    )
    .unwrap();
    world.admit(candidate).unwrap();
    assert_eq!(world.snapshot().entries(), &[(new, Payload(20))].into());
}

#[test]
fn canonical_aliases_refuse_even_identical_mutations() {
    let world = World::new(2, []);
    let first_description = Place::root(0, 1);
    let alias = Place::root(0, 1);
    let failure = prepare(
        &world.snapshot(),
        &[
            Action::Add(first_description, Payload(10)),
            Action::Add(alias, Payload(10)),
        ],
        Limits::default(),
    )
    .unwrap_err();
    assert_eq!(failure.kind, FailureKind::DuplicateMutation);
}

#[test]
fn remove_then_add_is_still_two_exclusive_mutations() {
    let place = Place::root(0, 1);
    let world = World::new(2, [(place.clone(), Payload(10))]);
    let failure = prepare(
        &world.snapshot(),
        &[
            Action::Remove(place.clone()),
            Action::Add(place, Payload(20)),
        ],
        Limits::default(),
    )
    .unwrap_err();
    assert_eq!(failure.kind, FailureKind::DuplicateMutation);
}

#[test]
fn same_key_in_different_fields_is_not_an_alias() {
    let left = Place::root(0, 1);
    let right = Place::root(1, 1);
    let mut world = World::new(1, []);
    let candidate = prepare(
        &world.snapshot(),
        &[
            Action::Add(left.clone(), Payload(10)),
            Action::Add(right.clone(), Payload(20)),
        ],
        Limits::default(),
    )
    .unwrap();
    world.admit(candidate).unwrap();
    assert_eq!(
        world.snapshot().entries(),
        &[(left, Payload(10)), (right, Payload(20))].into()
    );
}

fn apply(world: &mut World, actions: &[Action]) {
    let candidate = prepare(&world.snapshot(), actions, Limits::default()).unwrap();
    world.admit(candidate).unwrap();
}

#[test]
fn competing_adds_conflict_on_the_observed_cardinality() {
    let mut world = World::new(1, []);
    let basis = world.snapshot();
    let first = prepare(
        &basis,
        &[Action::Add(Place::root(0, 1), Payload(10))],
        Limits::default(),
    )
    .unwrap();
    let second = prepare(
        &basis,
        &[Action::Add(Place::root(0, 2), Payload(20))],
        Limits::default(),
    )
    .unwrap();
    world.admit(first).unwrap();
    let after_first = world.snapshot();
    assert_eq!(world.admit(second).unwrap_err().kind, FailureKind::Conflict);
    assert_eq!(world.snapshot(), after_first);
}

#[test]
fn buffered_add_and_remove_do_not_change_reads() {
    let old = Place::root(0, 1);
    let new = Place::root(0, 2);
    let mut world = World::new(1, [(old.clone(), Payload(10))]);
    let candidate = prepare(
        &world.snapshot(),
        &[
            Action::Add(new.clone(), Payload(20)),
            Action::Get(new.clone()),
            Action::Remove(old.clone()),
            Action::Get(old),
        ],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        candidate.readings(),
        &[Reading::Get(None), Reading::Get(Some(Payload(10)))]
    );
    world.admit(candidate).unwrap();
    assert_eq!(world.snapshot().entries(), &[(new, Payload(20))].into());
}

#[test]
fn empty_iteration_keeps_range_support_and_detects_insertion() {
    let mut world = World::new(2, []);
    let collection = Collection::root(0);
    let candidate = prepare(
        &world.snapshot(),
        &[Action::Iter(collection.clone(), Predicate::All)],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(candidate.readings(), &[Reading::Entries(vec![])]);
    assert!(candidate
        .support()
        .contains_key(&Support::Range(collection)));
    apply(&mut world, &[Action::Add(Place::root(0, 1), Payload(10))]);
    let before = world.snapshot();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::Conflict
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn excluded_payloads_still_support_a_filtered_reading() {
    let selected = Place::root(0, 1);
    let excluded = Place::root(0, 2);
    let mut world = World::new(
        2,
        [
            (selected.clone(), Payload(7)),
            (excluded.clone(), Payload(9)),
        ],
    );
    let candidate = prepare(
        &world.snapshot(),
        &[Action::Iter(
            Collection::root(0),
            Predicate::Equal(Payload(7)),
        )],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        candidate.readings(),
        &[Reading::Entries(vec![(selected, Payload(7))])]
    );
    assert!(candidate
        .support()
        .contains_key(&Support::Slot(excluded.clone())));
    apply(&mut world, &[Action::Replace(excluded, Payload(10))]);
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::Conflict
    );
}

#[test]
fn place_reuse_does_not_refresh_old_observations() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, [(place.clone(), Payload(10))]);
    let stale = prepare(
        &world.snapshot(),
        &[Action::Remove(place.clone())],
        Limits::default(),
    )
    .unwrap();
    apply(&mut world, &[Action::Remove(place.clone())]);
    apply(&mut world, &[Action::Add(place.clone(), Payload(10))]);
    let before = world.snapshot();
    assert_eq!(world.admit(stale).unwrap_err().kind, FailureKind::Conflict);
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.snapshot().entries(), &[(place, Payload(10))].into());
}

#[test]
fn unrelated_collection_changes_need_not_conflict() {
    let left = Place::root(0, 1);
    let right = Place::root(1, 1);
    let mut world = World::new(1, []);
    let first = prepare(
        &world.snapshot(),
        &[Action::Add(left.clone(), Payload(10))],
        Limits::default(),
    )
    .unwrap();
    apply(&mut world, &[Action::Add(right.clone(), Payload(20))]);
    world.admit(first).unwrap();
    assert_eq!(
        world.snapshot().entries(),
        &[(left, Payload(10)), (right, Payload(20))].into()
    );
}

#[test]
fn parent_removal_and_child_insertion_conflict_in_both_schedules() {
    let parent = Place::root(0, 1);
    let child = parent.child(0, 1);
    for insert_first in [false, true] {
        let mut world = World::new(2, [(parent.clone(), Payload(10))]);
        let removal = prepare(
            &world.snapshot(),
            &[Action::Remove(parent.clone())],
            Limits::default(),
        )
        .unwrap();
        let insertion = prepare(
            &world.snapshot(),
            &[Action::Add(child.clone(), Payload(20))],
            Limits::default(),
        )
        .unwrap();
        assert!(removal
            .support()
            .contains_key(&Support::Children(parent.clone())));
        assert!(insertion
            .support()
            .contains_key(&Support::Owner(parent.clone())));
        let stale = if insert_first {
            world.admit(insertion).unwrap();
            removal
        } else {
            world.admit(removal).unwrap();
            insertion
        };
        let after_first = world.snapshot();
        assert_eq!(world.admit(stale).unwrap_err().kind, FailureKind::Conflict);
        assert_eq!(world.snapshot(), after_first);
        let expected = if insert_first {
            [(parent.clone(), Payload(10)), (child.clone(), Payload(20))].into()
        } else {
            [].into()
        };
        assert_eq!(world.snapshot().entries(), &expected);
    }
}

#[test]
fn removing_children_does_not_satisfy_snapshot_emptiness() {
    let parent = Place::root(0, 1);
    let child = parent.child(0, 1);
    for parent_first in [false, true] {
        let mut world = World::new(
            2,
            [(parent.clone(), Payload(10)), (child.clone(), Payload(20))],
        );
        let before = world.snapshot();
        let mut actions = [
            Action::Remove(child.clone()),
            Action::Remove(parent.clone()),
        ];
        if parent_first {
            actions.reverse();
        }
        let candidate = prepare(&before, &actions, Limits::default()).unwrap();
        assert_eq!(
            world.admit(candidate).unwrap_err().kind,
            FailureKind::NotEmpty
        );
        assert_eq!(world.snapshot(), before);
    }
}

#[test]
fn a_new_owner_cannot_be_populated_in_the_same_candidate() {
    let parent = Place::root(0, 1);
    let child = parent.child(0, 1);
    let mut world = World::new(2, []);
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[
            Action::Add(parent, Payload(10)),
            Action::Add(child, Payload(20)),
        ],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::OwnerMissing
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn an_empty_owner_cannot_be_removed_while_adding_a_child() {
    let parent = Place::root(0, 1);
    let child = parent.child(0, 1);
    let mut world = World::new(2, [(parent.clone(), Payload(10))]);
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[Action::Remove(parent), Action::Add(child, Payload(20))],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::OwnerSurvival
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn replacing_and_copying_payloads_preserve_structural_ownership() {
    let parent = Place::root(0, 1);
    let copy = Place::root(0, 2);
    let child = parent.child(0, 1);
    let mut world = World::new(
        2,
        [(parent.clone(), Payload(10)), (child.clone(), Payload(20))],
    );
    apply(
        &mut world,
        &[
            Action::Replace(parent.clone(), Payload(30)),
            Action::Add(copy.clone(), Payload(30)),
        ],
    );
    assert_eq!(
        world.snapshot().entries(),
        &[
            (parent, Payload(30)),
            (copy.clone(), Payload(30)),
            (child, Payload(20))
        ]
        .into()
    );
    let read = prepare(
        &world.snapshot(),
        &[Action::Get(copy.child(0, 1))],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(read.readings(), &[Reading::Get(None)]);
}

#[test]
fn entry_keys_are_authoritative_even_when_payloads_name_other_keys() {
    let first = Place::root(0, 1);
    let second = Place::root(0, 2);
    let mut world = World::new(
        2,
        [(first.clone(), Payload(2)), (second.clone(), Payload(1))],
    );
    let candidate = prepare(
        &world.snapshot(),
        &[
            Action::Iter(Collection::root(0), Predicate::All),
            Action::Remove(first.clone()),
        ],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        candidate.readings(),
        &[Reading::Entries(vec![
            (first, Payload(2)),
            (second.clone(), Payload(1)),
        ])]
    );
    world.admit(candidate).unwrap();
    assert_eq!(world.snapshot().entries(), &[(second, Payload(1))].into());
}

#[test]
fn failed_move_preserves_source_membership_and_stable_owner() {
    let owner = Place::root(0, 1);
    let child = owner.child(0, 1);
    let source = Place::root(1, 1);
    let destination = Place::root(2, 1);
    // Payloads are opaque data: 1/2 describe the owner's application location.
    let mut world = World::new(
        1,
        [
            (owner.clone(), Payload(1)),
            (child, Payload(7)),
            (source.clone(), Payload(1)),
            (destination, Payload(9)),
        ],
    );
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[
            Action::Replace(owner, Payload(2)),
            Action::Remove(source),
            Action::Add(Place::root(2, 2), Payload(1)),
        ],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::Capacity
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn same_value_replacement_still_invalidates_prior_write_evidence() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, [(place.clone(), Payload(10))]);
    let reading = prepare(
        &world.snapshot(),
        &[Action::Get(place.clone())],
        Limits::default(),
    )
    .unwrap();
    apply(&mut world, &[Action::Replace(place.clone(), Payload(10))]);
    assert_eq!(world.snapshot().entries(), &[(place, Payload(10))].into());
    assert_eq!(
        world.admit(reading).unwrap_err().kind,
        FailureKind::Conflict
    );
}

#[test]
fn member_obstructions_do_not_mean_unavailable_observation() {
    let existing = Place::root(0, 1);
    let missing = Place::root(0, 2);
    for (action, expected) in [
        (
            Action::Add(existing.clone(), Payload(20)),
            FailureKind::AlreadyPresent,
        ),
        (Action::Remove(missing.clone()), FailureKind::NotFound),
        (
            Action::Replace(missing.clone(), Payload(20)),
            FailureKind::NotFound,
        ),
    ] {
        let mut world = World::new(2, [(existing.clone(), Payload(10))]);
        let before = world.snapshot();
        let candidate = prepare(&before, &[action], Limits::default()).unwrap();
        assert_eq!(world.admit(candidate).unwrap_err().kind, expected);
        assert_eq!(world.snapshot(), before);
    }
    let world = World::new(2, []);
    let reading = prepare(
        &world.snapshot(),
        &[Action::Get(missing.clone())],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(reading.readings(), &[Reading::Get(None)]);
    let unavailable = prepare(
        &world.snapshot(),
        &[Action::Get(missing.child(0, 1))],
        Limits::default(),
    )
    .unwrap_err();
    assert_eq!(unavailable.kind, FailureKind::Unavailable);
}

#[test]
fn whole_operation_budget_includes_atomic_admission() {
    for work in [11, 12] {
        let mut world = World::new(1, []);
        let before = world.snapshot();
        let candidate = prepare(
            &before,
            &[Action::Add(Place::root(0, 1), Payload(10))],
            Limits { work, scan: 0 },
        )
        .unwrap();
        assert_eq!(candidate.work_used(), 4);
        if work == 12 {
            assert_eq!(world.admit(candidate).unwrap().work_used, 12);
            assert_eq!(
                world.snapshot().entries(),
                &[(Place::root(0, 1), Payload(10))].into()
            );
        } else {
            let failure = world.admit(candidate).unwrap_err();
            assert_eq!(failure.kind, FailureKind::WorkExhausted);
            assert_eq!(failure.work_used(), 9);
            assert_eq!(world.snapshot(), before);
        }
    }
}

#[test]
fn nonmatching_entries_consume_work_and_scan_coverage() {
    let mut world = World::new(
        3,
        [
            (Place::root(0, 1), Payload(1)),
            (Place::root(0, 2), Payload(2)),
            (Place::root(0, 3), Payload(3)),
        ],
    );
    let actions = [Action::Iter(
        Collection::root(0),
        Predicate::Equal(Payload(99)),
    )];
    let complete = prepare(&world.snapshot(), &actions, Limits { work: 12, scan: 3 }).unwrap();
    assert_eq!(complete.readings(), &[Reading::Entries(vec![])]);
    assert_eq!(complete.work_used(), 8);
    assert_eq!(world.admit(complete).unwrap().work_used, 12);
    let incomplete =
        prepare(&world.snapshot(), &actions, Limits { work: 12, scan: 2 }).unwrap_err();
    assert_eq!(incomplete.kind, FailureKind::Incomplete);
    let exhausted = prepare(&world.snapshot(), &actions, Limits { work: 7, scan: 3 }).unwrap_err();
    assert_eq!(exhausted.kind, FailureKind::WorkExhausted);
}

#[test]
fn owned_traversals_share_one_operation_scan_limit() {
    let parent = Place::root(0, 1);
    let mut world = World::new(
        1,
        [
            (parent.clone(), Payload(10)),
            (parent.child(0, 1), Payload(20)),
        ],
    );
    let actions = [
        Action::Iter(parent.children(0), Predicate::All),
        Action::Iter(parent.children(0), Predicate::All),
    ];
    let incomplete =
        prepare(&world.snapshot(), &actions, Limits { work: 15, scan: 1 }).unwrap_err();
    assert_eq!(incomplete.kind, FailureKind::Incomplete);
    let complete = prepare(&world.snapshot(), &actions, Limits { work: 15, scan: 2 }).unwrap();
    assert_eq!(complete.work_used(), 12);
    assert_eq!(world.admit(complete).unwrap().work_used, 15);
}

#[test]
fn exhausted_work_cannot_be_used_as_an_absence_witness() {
    let world = World::new(1, []);
    let failure = prepare(
        &world.snapshot(),
        &[Action::Get(Place::root(0, 1))],
        Limits { work: 0, scan: 0 },
    )
    .unwrap_err();
    assert_eq!(failure.kind, FailureKind::WorkExhausted);
    assert_eq!(failure.work_used(), 0);
}
