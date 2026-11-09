use amblicit_collections_model::{prepare, Action, FailureKind, Limits, Payload, Place, World};

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
