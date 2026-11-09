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
