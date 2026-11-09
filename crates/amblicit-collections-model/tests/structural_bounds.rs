//! Small, deterministic contract cases; no I/O, clocks, threads, or randomness.

use amblicit_collections_model::{
    prepare, Action, FailureKind, Limits, Payload, Place, Reading, World,
};

#[test]
fn initial_state_preserves_exactly_sixty_four_members() {
    let members: Vec<_> = (0..64)
        .map(|key| (Place::root(0, key), Payload(7)))
        .collect();
    let expected = members.iter().cloned().collect();
    let world = World::new(64, members).unwrap();
    assert_eq!(world.snapshot().entries(), &expected);
}

#[test]
fn initial_state_refuses_a_sixty_fifth_member() {
    let members = (0..65).map(|key| (Place::root(0, key), Payload(7)));
    let failure = World::new(64, members).unwrap_err();
    assert_eq!(failure.kind, FailureKind::ModelBound);
    assert_eq!(failure.work_used(), 0);
}

#[test]
fn initial_state_refuses_duplicate_places() {
    let place = Place::root(0, 1);
    let failure = World::new(2, [(place.clone(), Payload(7)), (place, Payload(7))]).unwrap_err();
    assert_eq!(failure.kind, FailureKind::AlreadyPresent);
}

#[test]
fn initial_state_refuses_an_orphan() {
    let orphan = Place::root(0, 1).child(0, 1).unwrap();
    let failure = World::new(1, [(orphan, Payload(7))]).unwrap_err();
    assert_eq!(failure.kind, FailureKind::OwnerMissing);
}

#[test]
fn initial_state_refuses_collection_over_capacity() {
    let failure = World::new(
        1,
        [
            (Place::root(0, 1), Payload(7)),
            (Place::root(0, 2), Payload(8)),
        ],
    )
    .unwrap_err();
    assert_eq!(failure.kind, FailureKind::Capacity);
}

#[test]
fn initial_state_accepts_a_child_before_its_owner() {
    let parent = Place::root(0, 1);
    let child = parent.child(0, 1).unwrap();
    let world = World::new(
        1,
        [(child.clone(), Payload(8)), (parent.clone(), Payload(7))],
    )
    .unwrap();
    assert_eq!(
        world.snapshot().entries(),
        &[(parent, Payload(7)), (child, Payload(8))].into()
    );
}

#[test]
fn sixty_four_requests_keep_their_observations() {
    let mut world = World::new(1, []).unwrap();
    let requests = vec![Action::Get(Place::root(0, 1)); 64];
    let candidate = prepare(&world.snapshot(), &requests, Limits::default()).unwrap();
    assert_eq!(candidate.readings(), vec![Reading::Get(None); 64]);
    assert_eq!(candidate.work_used(), 128);
    assert_eq!(world.admit(candidate).unwrap().work_used, 129);
}

#[test]
fn a_sixty_fifth_request_refuses_before_evaluation() {
    let world = World::new(1, []).unwrap();
    let requests = vec![Action::Get(Place::root(0, 1)); 65];
    let failure = prepare(&world.snapshot(), &requests, Limits::default()).unwrap_err();
    assert_eq!(failure.kind, FailureKind::ModelBound);
    assert_eq!(failure.work_used(), 0);
}

#[test]
fn adding_a_sixty_fifth_live_member_is_atomic_refusal() {
    let mut world = World::new(64, (0..64).map(|key| (Place::root(0, key), Payload(7)))).unwrap();
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[Action::Add(Place::root(1, 1), Payload(8))],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ModelBound
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn four_owner_components_are_representable() {
    let root = Place::root(0, 1);
    let one = root.child(0, 1).unwrap();
    let two = one.child(0, 1).unwrap();
    let three = two.child(0, 1).unwrap();
    let four = three.child(0, 1).unwrap();
    let world = World::new(
        1,
        [root, one, two, three, four.clone()].map(|p| (p, Payload(7))),
    )
    .unwrap();
    let candidate = prepare(&world.snapshot(), &[Action::Get(four)], Limits::default()).unwrap();
    assert_eq!(candidate.readings(), [Reading::Get(Some(Payload(7)))]);
}

fn depth_four() -> Place {
    Place::root(0, 1)
        .child(0, 1)
        .unwrap()
        .child(0, 1)
        .unwrap()
        .child(0, 1)
        .unwrap()
        .child(0, 1)
        .unwrap()
}

#[test]
fn a_fifth_owner_component_refuses() {
    assert_eq!(
        depth_four().child(0, 1).unwrap_err().kind,
        FailureKind::ModelBound
    );
}

#[test]
fn a_fifth_owner_collection_refuses() {
    assert_eq!(
        depth_four().children(0).unwrap_err().kind,
        FailureKind::ModelBound
    );
}

fn replace(world: &mut World, payload: i64) {
    let candidate = prepare(
        &world.snapshot(),
        &[Action::Replace(Place::root(0, 1), Payload(payload))],
        Limits::default(),
    )
    .unwrap();
    world.admit(candidate).unwrap();
}

fn replace_repeatedly(world: &mut World, count: u16) {
    for value in 0..count {
        replace(world, i64::from(value));
    }
}

#[test]
fn the_one_hundred_twenty_ninth_mutation_refuses() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, [(place.clone(), Payload(-1))]).unwrap();
    replace_repeatedly(&mut world, 128);
    let before = world.snapshot();
    assert_eq!(before.entries(), &[(place.clone(), Payload(127))].into());
    let candidate = prepare(
        &before,
        &[Action::Replace(place, Payload(128))],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ModelBound
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn read_only_admission_survives_exhausted_mutation_history() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, [(place.clone(), Payload(-1))]).unwrap();
    replace_repeatedly(&mut world, 128);
    let before = world.snapshot();
    let candidate = prepare(&before, &[Action::Get(place)], Limits::default()).unwrap();
    assert_eq!(candidate.readings(), [Reading::Get(Some(Payload(127)))]);
    assert_eq!(world.admit(candidate).unwrap().work_used, 4);
    assert_eq!(world.snapshot(), before);
}

#[test]
fn refused_operations_do_not_consume_mutation_history() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, [(place.clone(), Payload(-1))]).unwrap();
    replace_repeatedly(&mut world, 127);
    let invalid = prepare(
        &world.snapshot(),
        &[Action::Add(place.clone(), Payload(999))],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(invalid).unwrap_err().kind,
        FailureKind::AlreadyPresent
    );
    replace(&mut world, 999);
    assert_eq!(world.snapshot().entries(), &[(place, Payload(999))].into());
}
