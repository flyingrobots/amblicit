//! Content publication across operation boundaries and independent namespaces.

use amblicit_collections_model::{
    prepare, Action, Content, ContentId, ContentKind, ContentRef, FailureKind, Limits, Payload,
    Place, Receipt, World,
};

fn apply(world: &mut World, actions: &[Action]) -> Receipt {
    let candidate = prepare(&world.snapshot(), actions, Limits::default()).unwrap();
    world.admit(candidate).unwrap()
}

fn leaf(id: u8, payload: i64) -> Action {
    Action::Ensure(ContentId(id), Content::Leaf(Payload(payload)))
}

#[test]
fn identical_concurrent_ensures_publish_once() {
    let mut world = World::new(1, []).unwrap();
    let basis = world.snapshot();
    let first = prepare(&basis, &[leaf(1, 7)], Limits::default()).unwrap();
    let second = prepare(&basis, &[leaf(1, 7)], Limits::default()).unwrap();
    assert_eq!(world.admit(first).unwrap().content_added, 1);
    assert_eq!(world.admit(second).unwrap().content_added, 0);
    assert_eq!(
        world.snapshot().contents(),
        &[(ContentId(1), Content::Leaf(Payload(7)))].into()
    );
}

#[test]
fn conflicting_concurrent_ensures_refuse_the_second_admission() {
    let mut world = World::new(1, []).unwrap();
    let basis = world.snapshot();
    let first = prepare(&basis, &[leaf(1, 7)], Limits::default()).unwrap();
    let second = prepare(&basis, &[leaf(1, 8)], Limits::default()).unwrap();
    assert_eq!(world.admit(first).unwrap().content_added, 1);
    let before = world.snapshot();
    assert_eq!(
        world.admit(second).unwrap_err().kind,
        FailureKind::ContentIntegrity
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn later_child_insertion_does_not_repair_a_missing_basis_child() {
    let mut world = World::new(1, []).unwrap();
    let parent = prepare(
        &world.snapshot(),
        &[Action::Ensure(
            ContentId(1),
            Content::Branch(vec![ContentRef {
                id: ContentId(2),
                kind: ContentKind::Leaf,
            }]),
        )],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(apply(&mut world, &[leaf(2, 7)]).content_added, 1);
    let before = world.snapshot();
    assert_eq!(
        world.admit(parent).unwrap_err().kind,
        FailureKind::ContentMissing
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn an_existing_typed_child_can_support_a_new_parent() {
    let mut world = World::new(1, []).unwrap();
    assert_eq!(apply(&mut world, &[leaf(2, 7)]).content_added, 1);
    let parent = Content::Branch(vec![ContentRef {
        id: ContentId(2),
        kind: ContentKind::Leaf,
    }]);
    assert_eq!(
        apply(&mut world, &[Action::Ensure(ContentId(1), parent.clone())]).content_added,
        1
    );
    assert_eq!(
        world.snapshot().contents(),
        &[
            (ContentId(1), parent),
            (ContentId(2), Content::Leaf(Payload(7))),
        ]
        .into()
    );
}

#[test]
fn content_kind_cannot_change_under_one_identity() {
    let mut world = World::new(1, []).unwrap();
    assert_eq!(apply(&mut world, &[leaf(1, 7)]).content_added, 1);
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[Action::Ensure(ContentId(1), Content::Branch(vec![]))],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ContentIntegrity
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn mutable_place_operations_do_not_address_content() {
    let mut world = World::new(1, []).unwrap();
    assert_eq!(apply(&mut world, &[leaf(1, 7)]).content_added, 1);
    let place = Place::root(0, 1);
    apply(&mut world, &[Action::Add(place.clone(), Payload(10))]);
    apply(&mut world, &[Action::Replace(place.clone(), Payload(20))]);
    apply(&mut world, &[Action::Remove(place)]);
    assert_eq!(
        world.snapshot().contents(),
        &[(ContentId(1), Content::Leaf(Payload(7)))].into()
    );
    assert!(world.snapshot().entries().is_empty());
}

#[test]
fn sixty_four_content_nodes_fit() {
    let mut world = World::new(1, []).unwrap();
    let requests: Vec<_> = (0..64).map(|id| leaf(id, 7)).collect();
    assert_eq!(apply(&mut world, &requests).content_added, 64);
    let expected = (0..64)
        .map(|id| (ContentId(id), Content::Leaf(Payload(7))))
        .collect();
    assert_eq!(world.snapshot().contents(), &expected);
}

#[test]
fn a_sixty_fifth_live_content_node_refuses() {
    let mut world = World::new(1, []).unwrap();
    let requests: Vec<_> = (0..64).map(|id| leaf(id, 7)).collect();
    assert_eq!(apply(&mut world, &requests).content_added, 64);
    let before = world.snapshot();
    let candidate = prepare(&before, &[leaf(64, 8)], Limits::default()).unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ModelBound
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn four_content_links_are_admitted() {
    let mut world = World::new(1, []).unwrap();
    let branch = Content::Branch(vec![
        ContentRef {
            id: ContentId(2),
            kind: ContentKind::Leaf
        };
        4
    ]);
    let requests = [Action::Ensure(ContentId(1), branch.clone()), leaf(2, 7)];
    assert_eq!(apply(&mut world, &requests).content_added, 2);
    assert_eq!(
        world.snapshot().contents(),
        &[
            (ContentId(1), branch),
            (ContentId(2), Content::Leaf(Payload(7))),
        ]
        .into()
    );
}

fn exhaust_history(world: &mut World) {
    for value in 0..127 {
        apply(world, &[Action::Replace(Place::root(0, 1), Payload(value))]);
    }
}

#[test]
fn an_existing_ensure_can_be_corroborated_after_history_exhaustion() {
    let mut world = World::new(1, [(Place::root(0, 1), Payload(-1))]).unwrap();
    assert_eq!(apply(&mut world, &[leaf(1, 7)]).content_added, 1);
    exhaust_history(&mut world);
    let before = world.snapshot();
    assert_eq!(apply(&mut world, &[leaf(1, 7)]).content_added, 0);
    assert_eq!(world.snapshot(), before);
}

#[test]
fn new_content_refuses_after_history_exhaustion() {
    let mut world = World::new(1, [(Place::root(0, 1), Payload(-1))]).unwrap();
    assert_eq!(apply(&mut world, &[leaf(1, 7)]).content_added, 1);
    exhaust_history(&mut world);
    let before = world.snapshot();
    let candidate = prepare(&before, &[leaf(2, 8)], Limits::default()).unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ModelBound
    );
    assert_eq!(world.snapshot(), before);
}
