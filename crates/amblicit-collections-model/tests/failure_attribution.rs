//! Small deterministic public-API fixtures. Oracles are the declared precedence
//! and literal logical identities, independent of model validation helpers.

use amblicit_collections_model::{
    prepare, Action, Collection, Content, ContentId, ContentKind, ContentRef, Failure, FailureKind,
    FailureSubject, Limits, Payload, Place, Predicate, Support, World,
};

fn assert_failure(failure: Failure, kind: FailureKind, subject: FailureSubject) {
    assert_eq!((failure.kind, failure.subject), (kind, subject));
}

#[test]
fn duplicate_attribution_uses_the_least_place_in_either_request_order() {
    let world = World::new(8, []).unwrap();
    let low = Place::root(1, 2);
    let high = Place::root(2, 1);
    for places in [
        [high.clone(), high.clone(), low.clone(), low.clone()],
        [low.clone(), high.clone(), low.clone(), high.clone()],
    ] {
        let actions: Vec<_> = places.into_iter().map(Action::Remove).collect();
        assert_failure(
            prepare(&world.snapshot(), &actions, Limits::default()).unwrap_err(),
            FailureKind::DuplicateMutation,
            FailureSubject::Place(low.clone()),
        );
    }
}

#[test]
fn regrouping_does_not_move_the_duplicate_subject_to_a_call_site() {
    let world = World::new(1, []).unwrap();
    let place = Place::root(1, 1);
    let action = Action::Remove(place.clone());
    let actions = [
        Action::Group(vec![action.clone()]),
        Action::Group(vec![action]),
    ];
    assert_failure(
        prepare(&world.snapshot(), &actions, Limits::default()).unwrap_err(),
        FailureKind::DuplicateMutation,
        FailureSubject::Place(place),
    );
}

#[test]
fn duplicate_normalization_precedes_conflicting_content() {
    let world = World::new(1, []).unwrap();
    let place = Place::root(1, 1);
    let actions = [
        Action::Ensure(ContentId(1), Content::Leaf(Payload(1))),
        Action::Ensure(ContentId(1), Content::Leaf(Payload(2))),
        Action::Remove(place.clone()),
        Action::Remove(place.clone()),
    ];
    assert_failure(
        prepare(&world.snapshot(), &actions, Limits::default()).unwrap_err(),
        FailureKind::DuplicateMutation,
        FailureSubject::Place(place),
    );
}

#[test]
fn content_normalization_selects_the_least_conflicting_id() {
    let world = World::new(1, []).unwrap();
    let actions = [
        Action::Ensure(ContentId(9), Content::Leaf(Payload(1))),
        Action::Ensure(ContentId(9), Content::Leaf(Payload(3))),
        Action::Ensure(ContentId(2), Content::Leaf(Payload(2))),
        Action::Ensure(ContentId(2), Content::Leaf(Payload(3))),
    ];
    assert_failure(
        prepare(&world.snapshot(), &actions, Limits::default()).unwrap_err(),
        FailureKind::ContentIntegrity,
        FailureSubject::Content(ContentId(2)),
    );
}

#[test]
fn domain_order_is_place_order_instead_of_request_or_kind_order() {
    let low = Place::root(1, 1);
    let high = Place::root(2, 1);
    for actions in [
        [
            Action::Add(high.clone(), Payload(2)),
            Action::Remove(low.clone()),
        ],
        [
            Action::Remove(low.clone()),
            Action::Add(high.clone(), Payload(2)),
        ],
    ] {
        let mut world = World::new(1, [(high.clone(), Payload(1))]).unwrap();
        let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
        assert_failure(
            world.admit(candidate).unwrap_err(),
            FailureKind::NotFound,
            FailureSubject::Place(low.clone()),
        );
    }
}

#[test]
fn domains_precede_capacity_and_content_closure() {
    let missing = Place::root(9, 1);
    let actions = [
        Action::Add(Place::root(1, 1), Payload(1)),
        Action::Add(Place::root(1, 2), Payload(2)),
        Action::Ensure(
            ContentId(1),
            Content::Branch(vec![ContentRef {
                id: ContentId(2),
                kind: ContentKind::Leaf,
            }]),
        ),
        Action::Remove(missing.clone()),
    ];
    let mut world = World::new(1, []).unwrap();
    let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
    assert_failure(
        world.admit(candidate).unwrap_err(),
        FailureKind::NotFound,
        FailureSubject::Place(missing),
    );
}

#[test]
fn owner_survival_precedes_capacity() {
    let owner = Place::root(1, 1);
    let child = owner.child(1, 1).unwrap();
    let mut world = World::new(1, [(owner.clone(), Payload(1))]).unwrap();
    let actions = [
        Action::Remove(owner),
        Action::Add(child.clone(), Payload(2)),
        Action::Add(Place::root(2, 1), Payload(3)),
        Action::Add(Place::root(2, 2), Payload(4)),
    ];
    let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
    assert_failure(
        world.admit(candidate).unwrap_err(),
        FailureKind::OwnerSurvival,
        FailureSubject::Place(child),
    );
}

#[test]
fn capacity_selects_the_least_collection_before_content_validation() {
    let mut world = World::new(0, []).unwrap();
    let actions = [
        Action::Add(Place::root(9, 1), Payload(1)),
        Action::Add(Place::root(2, 1), Payload(2)),
        Action::Ensure(
            ContentId(1),
            Content::Branch(vec![ContentRef {
                id: ContentId(2),
                kind: ContentKind::Leaf,
            }]),
        ),
    ];
    let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
    assert_failure(
        world.admit(candidate).unwrap_err(),
        FailureKind::Capacity,
        FailureSubject::Collection(Collection::root(2)),
    );
}

#[test]
fn conflict_reports_the_first_canonical_support_fact_before_domains() {
    let low = Place::root(1, 1);
    let high = Place::root(2, 1);
    let mut world = World::new(2, [(low.clone(), Payload(1)), (high.clone(), Payload(2))]).unwrap();
    let stale = prepare(
        &world.snapshot(),
        &[
            Action::Get(high.clone()),
            Action::Get(low.clone()),
            Action::Remove(Place::root(0, 0)),
        ],
        Limits::default(),
    )
    .unwrap();
    let write = prepare(
        &world.snapshot(),
        &[
            Action::Replace(high, Payload(3)),
            Action::Replace(low.clone(), Payload(4)),
        ],
        Limits::default(),
    )
    .unwrap();
    world.admit(write).unwrap();
    assert_failure(
        world.admit(stale).unwrap_err(),
        FailureKind::Conflict,
        FailureSubject::Support(Support::Slot(low)),
    );
}

#[test]
fn unavailable_observation_names_its_missing_owner_before_normalization() {
    let owner = Place::root(1, 1);
    let place = owner.child(1, 1).unwrap();
    let world = World::new(1, []).unwrap();
    let actions = [
        Action::Add(place.clone(), Payload(1)),
        Action::Add(place.clone(), Payload(2)),
        Action::Get(place),
    ];
    assert_failure(
        prepare(&world.snapshot(), &actions, Limits::default()).unwrap_err(),
        FailureKind::Unavailable,
        FailureSubject::Place(owner),
    );
}

#[test]
fn incomplete_traversal_names_the_collection() {
    let world = World::new(1, [(Place::root(1, 1), Payload(1))]).unwrap();
    let actions = [Action::Iter(Collection::root(1), Predicate::All)];
    assert_failure(
        prepare(&world.snapshot(), &actions, Limits { work: 100, scan: 0 }).unwrap_err(),
        FailureKind::Incomplete,
        FailureSubject::Collection(Collection::root(1)),
    );
}

#[test]
fn exhaustion_is_operation_wide_even_before_a_known_duplicate() {
    let world = World::new(1, []).unwrap();
    let actions = [1, 1, 2, 2].map(|key| Action::Remove(Place::root(1, key)));
    assert_failure(
        prepare(&world.snapshot(), &actions, Limits { work: 19, scan: 0 }).unwrap_err(),
        FailureKind::WorkExhausted,
        FailureSubject::Operation,
    );
}

#[test]
fn foreign_basis_precedes_domain_validation() {
    let source = World::new(1, []).unwrap();
    let candidate = prepare(
        &source.snapshot(),
        &[Action::Remove(Place::root(1, 1))],
        Limits::default(),
    )
    .unwrap();
    let mut destination = World::new(1, []).unwrap();
    assert_failure(
        destination.admit(candidate).unwrap_err(),
        FailureKind::BasisMismatch,
        FailureSubject::Operation,
    );
}

#[test]
fn integrity_precedes_missing_closure_and_names_the_conflicting_id() {
    let mut world = World::new(1, []).unwrap();
    let seed = prepare(
        &world.snapshot(),
        &[Action::Ensure(ContentId(9), Content::Leaf(Payload(1)))],
        Limits::default(),
    )
    .unwrap();
    world.admit(seed).unwrap();
    let actions = [
        Action::Ensure(
            ContentId(1),
            Content::Branch(vec![ContentRef {
                id: ContentId(2),
                kind: ContentKind::Leaf,
            }]),
        ),
        Action::Ensure(ContentId(9), Content::Leaf(Payload(2))),
    ];
    let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
    assert_failure(
        world.admit(candidate).unwrap_err(),
        FailureKind::ContentIntegrity,
        FailureSubject::Content(ContentId(9)),
    );
}

#[test]
fn closure_checks_roots_then_edges_in_canonical_order() {
    let mut world = World::new(1, []).unwrap();
    let actions = [
        Action::Ensure(
            ContentId(2),
            Content::Branch(vec![ContentRef {
                id: ContentId(3),
                kind: ContentKind::Leaf,
            }]),
        ),
        Action::Ensure(
            ContentId(1),
            Content::Branch(vec![
                ContentRef {
                    id: ContentId(9),
                    kind: ContentKind::Leaf,
                },
                ContentRef {
                    id: ContentId(8),
                    kind: ContentKind::Leaf,
                },
            ]),
        ),
    ];
    let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
    assert_failure(
        world.admit(candidate).unwrap_err(),
        FailureKind::ContentMissing,
        FailureSubject::Content(ContentId(8)),
    );
}

#[test]
fn a_mistyped_target_names_the_target() {
    let mut world = World::new(1, []).unwrap();
    let actions = [
        Action::Ensure(
            ContentId(1),
            Content::Branch(vec![ContentRef {
                id: ContentId(2),
                kind: ContentKind::Branch,
            }]),
        ),
        Action::Ensure(ContentId(2), Content::Leaf(Payload(1))),
    ];
    let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
    assert_failure(
        world.admit(candidate).unwrap_err(),
        FailureKind::ContentType,
        FailureSubject::Content(ContentId(2)),
    );
}

#[test]
fn a_cycle_names_the_reentered_active_node() {
    let mut world = World::new(1, []).unwrap();
    let actions = [Action::Ensure(
        ContentId(4),
        Content::Branch(vec![ContentRef {
            id: ContentId(4),
            kind: ContentKind::Branch,
        }]),
    )];
    let candidate = prepare(&world.snapshot(), &actions, Limits::default()).unwrap();
    assert_failure(
        world.admit(candidate).unwrap_err(),
        FailureKind::ContentCycle,
        FailureSubject::Content(ContentId(4)),
    );
}

#[test]
fn constructor_orphan_names_the_member() {
    let owner = Place::root(1, 1);
    let orphan = owner.child(1, 1).unwrap();
    assert_failure(
        World::new(1, [(orphan.clone(), Payload(1))]).unwrap_err(),
        FailureKind::OwnerMissing,
        FailureSubject::Place(orphan),
    );
}

#[test]
fn constructor_duplicate_names_the_member() {
    let owner = Place::root(1, 1);
    assert_failure(
        World::new(
            1,
            [(owner.clone(), Payload(1)), (owner.clone(), Payload(2))],
        )
        .unwrap_err(),
        FailureKind::AlreadyPresent,
        FailureSubject::Place(owner),
    );
}

#[test]
fn constructor_capacity_names_the_collection() {
    assert_failure(
        World::new(0, [(Place::root(3, 1), Payload(1))]).unwrap_err(),
        FailureKind::Capacity,
        FailureSubject::Collection(Collection::root(3)),
    );
}
