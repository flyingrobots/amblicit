//! Explicit nested request programs; expected readings and costs are hand-specified.

use amblicit_collections_model::{
    prepare, Action, Collection, FailureKind, Limits, Payload, Place, Predicate, Reading, World,
};

fn two_nested_scans() -> Vec<Action> {
    vec![Action::Group(vec![
        Action::Iter(Collection::root(0), Predicate::Equal(Payload(-1))),
        Action::Group(vec![Action::Iter(Collection::root(0), Predicate::All)]),
    ])]
}

#[test]
fn nested_traversals_charge_all_inspections() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, [(place.clone(), Payload(7))]).unwrap();
    let candidate = prepare(
        &world.snapshot(),
        &two_nested_scans(),
        Limits { work: 13, scan: 2 },
    )
    .unwrap();
    assert_eq!(
        candidate.readings(),
        [
            Reading::Entries(vec![]),
            Reading::Entries(vec![(place, Payload(7))])
        ]
    );
    assert_eq!(candidate.work_used(), 11);
    assert_eq!(world.admit(candidate).unwrap().work_used, 13);
}

#[test]
fn nested_work_does_not_reset_the_operation_budget() {
    let mut world = World::new(1, [(Place::root(0, 1), Payload(7))]).unwrap();
    let before = world.snapshot();
    let candidate = prepare(&before, &two_nested_scans(), Limits { work: 12, scan: 2 }).unwrap();
    let failure = world.admit(candidate).unwrap_err();
    assert_eq!(failure.kind, FailureKind::WorkExhausted);
    assert_eq!(failure.work_used(), 12);
    assert_eq!(world.snapshot(), before);
}

#[test]
fn nested_scan_coverage_is_shared() {
    let world = World::new(1, [(Place::root(0, 1), Payload(7))]).unwrap();
    let failure = prepare(
        &world.snapshot(),
        &two_nested_scans(),
        Limits { work: 13, scan: 1 },
    )
    .unwrap_err();
    assert_eq!(failure.kind, FailureKind::Incomplete);
    assert_eq!(failure.work_used(), 8);
}

#[test]
fn nested_reads_ignore_buffered_writes() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, []).unwrap();
    let requests = [Action::Group(vec![
        Action::Add(place.clone(), Payload(7)),
        Action::Group(vec![Action::Get(place.clone())]),
        Action::Get(place.clone()),
    ])];
    let candidate = prepare(&world.snapshot(), &requests, Limits::default()).unwrap();
    assert_eq!(
        candidate.readings(),
        [Reading::Get(None), Reading::Get(None)]
    );
    world.admit(candidate).unwrap();
    assert_eq!(world.snapshot().entries(), &[(place, Payload(7))].into());
}

#[test]
fn exclusive_aliases_across_groups_refuse() {
    let place = Place::root(0, 1);
    let world = World::new(1, [(place.clone(), Payload(7))]).unwrap();
    let requests = [
        Action::Group(vec![Action::Replace(place.clone(), Payload(8))]),
        Action::Group(vec![Action::Group(vec![Action::Replace(
            place,
            Payload(8),
        )])]),
    ];
    let failure = prepare(&world.snapshot(), &requests, Limits::default()).unwrap_err();
    assert_eq!(failure.kind, FailureKind::DuplicateMutation);
}

#[test]
fn nested_exchange_uses_whole_candidate_capacity() {
    let old = Place::root(0, 1);
    let new = Place::root(0, 2);
    let mut world = World::new(1, [(old.clone(), Payload(7))]).unwrap();
    let requests = [
        Action::Group(vec![Action::Add(new.clone(), Payload(8))]),
        Action::Group(vec![Action::Group(vec![Action::Remove(old)])]),
    ];
    let candidate = prepare(&world.snapshot(), &requests, Limits::default()).unwrap();
    world.admit(candidate).unwrap();
    assert_eq!(world.snapshot().entries(), &[(new, Payload(8))].into());
}

fn nested(depth: u8) -> Action {
    (0..depth).fold(Action::Get(Place::root(0, 1)), |request, _| {
        Action::Group(vec![request])
    })
}

#[test]
fn four_groups_preserve_the_leaf_observation() {
    let mut world = World::new(1, []).unwrap();
    let candidate = prepare(&world.snapshot(), &[nested(4)], Limits::default()).unwrap();
    assert_eq!(candidate.readings(), [Reading::Get(None)]);
    assert_eq!(world.admit(candidate).unwrap().work_used, 7);
}

#[test]
fn a_fifth_group_refuses_before_evaluation() {
    let world = World::new(1, []).unwrap();
    let failure = prepare(&world.snapshot(), &[nested(5)], Limits::default()).unwrap_err();
    assert_eq!(failure.kind, FailureKind::ModelBound);
    assert_eq!(failure.work_used(), 0);
}

#[test]
fn sixty_four_nodes_include_group_overhead() {
    let mut world = World::new(1, []).unwrap();
    let mut requests = vec![Action::Get(Place::root(0, 1)); 31];
    requests.push(Action::Group(vec![Action::Get(Place::root(0, 1)); 32]));
    let candidate = prepare(&world.snapshot(), &requests, Limits::default()).unwrap();
    assert_eq!(candidate.readings(), vec![Reading::Get(None); 63]);
    assert_eq!(world.admit(candidate).unwrap().work_used, 128);
}

#[test]
fn nested_nodes_cannot_hide_a_sixty_fifth_request() {
    let world = World::new(1, []).unwrap();
    let mut requests = vec![Action::Get(Place::root(0, 1)); 31];
    requests.push(Action::Group(vec![Action::Get(Place::root(0, 1)); 33]));
    let failure = prepare(&world.snapshot(), &requests, Limits::default()).unwrap_err();
    assert_eq!(failure.kind, FailureKind::ModelBound);
    assert_eq!(failure.work_used(), 0);
}
