//! Independently specified typed-closure fixtures with symbolic content identities.

use amblicit_collections_model::{
    prepare, Action, Content, ContentId, ContentKind, ContentRef, FailureKind, Limits, Payload,
    Place, World,
};

fn reference(id: u8, kind: ContentKind) -> ContentRef {
    ContentRef {
        id: ContentId(id),
        kind,
    }
}

#[test]
fn a_leaf_ensure_uses_six_work_units() {
    let mut world = World::new(1, []).unwrap();
    let candidate = prepare(
        &world.snapshot(),
        &[Action::Ensure(ContentId(1), Content::Leaf(Payload(7)))],
        Limits { work: 6, scan: 0 },
    )
    .unwrap();
    assert_eq!(candidate.work_used(), 3);
    let receipt = world.admit(candidate).unwrap();
    assert_eq!(receipt.work_used, 6);
    assert_eq!(receipt.content_added, 1);
    assert_eq!(
        world.snapshot().contents(),
        &[(ContentId(1), Content::Leaf(Payload(7)))].into()
    );
}

#[test]
fn a_leaf_ensure_refuses_one_unit_short_without_publication() {
    let mut world = World::new(1, []).unwrap();
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[Action::Ensure(ContentId(1), Content::Leaf(Payload(7)))],
        Limits { work: 5, scan: 0 },
    )
    .unwrap();
    let failure = world.admit(candidate).unwrap_err();
    assert_eq!(failure.kind, FailureKind::WorkExhausted);
    assert_eq!(failure.work_used(), 5);
    assert_eq!(world.snapshot(), before);
}

fn duplicate_leaf() -> [Action; 2] {
    [
        Action::Ensure(ContentId(1), Content::Leaf(Payload(7))),
        Action::Ensure(ContentId(1), Content::Leaf(Payload(7))),
    ]
}

#[test]
fn identical_ensures_coalesce_into_one_obligation() {
    let world = World::new(1, []).unwrap();
    let candidate = prepare(&world.snapshot(), &duplicate_leaf(), Limits::default()).unwrap();
    assert_eq!(
        candidate.ensures(),
        &[(ContentId(1), Content::Leaf(Payload(7)))].into()
    );
}

#[test]
fn identical_ensures_keep_the_work_spent_on_both_requests() {
    let mut world = World::new(1, []).unwrap();
    let candidate = prepare(
        &world.snapshot(),
        &duplicate_leaf(),
        Limits { work: 9, scan: 0 },
    )
    .unwrap();
    assert_eq!(candidate.work_used(), 6);
    let receipt = world.admit(candidate).unwrap();
    assert_eq!(receipt.work_used, 9);
    assert_eq!(receipt.content_added, 1);
}

#[test]
fn conflicting_ensures_do_not_normalize() {
    let world = World::new(1, []).unwrap();
    let failure = prepare(
        &world.snapshot(),
        &[
            Action::Ensure(ContentId(1), Content::Leaf(Payload(7))),
            Action::Ensure(ContentId(1), Content::Leaf(Payload(8))),
        ],
        Limits::default(),
    )
    .unwrap_err();
    assert_eq!(failure.kind, FailureKind::ContentIntegrity);
    assert_eq!(failure.work_used(), 6);
}

fn parent_child() -> [Action; 2] {
    [
        Action::Ensure(
            ContentId(1),
            Content::Branch(vec![reference(2, ContentKind::Leaf)]),
        ),
        Action::Ensure(ContentId(2), Content::Leaf(Payload(7))),
    ]
}

fn assert_complete_parent_child(requests: &[Action]) {
    let mut world = World::new(1, []).unwrap();
    let candidate = prepare(&world.snapshot(), requests, Limits { work: 15, scan: 0 }).unwrap();
    assert_eq!(candidate.work_used(), 8);
    let receipt = world.admit(candidate).unwrap();
    assert_eq!(receipt.work_used, 15);
    assert_eq!(receipt.content_added, 2);
    assert_eq!(
        world.snapshot().contents(),
        &[
            (
                ContentId(1),
                Content::Branch(vec![reference(2, ContentKind::Leaf)])
            ),
            (ContentId(2), Content::Leaf(Payload(7))),
        ]
        .into()
    );
}

#[test]
fn a_new_parent_can_precede_its_new_child() {
    assert_complete_parent_child(&parent_child());
}

#[test]
fn a_new_child_can_precede_its_new_parent() {
    let [parent, child] = parent_child();
    assert_complete_parent_child(&[child, parent]);
}

#[test]
fn a_missing_child_refuses_the_complete_proposal() {
    let mut world = World::new(1, []).unwrap();
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[Action::Ensure(
            ContentId(1),
            Content::Branch(vec![reference(2, ContentKind::Leaf)]),
        )],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ContentMissing
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn a_child_of_the_wrong_kind_refuses() {
    let mut world = World::new(1, []).unwrap();
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[
            Action::Ensure(
                ContentId(1),
                Content::Branch(vec![reference(2, ContentKind::Branch)]),
            ),
            Action::Ensure(ContentId(2), Content::Leaf(Payload(7))),
        ],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ContentType
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn a_content_cycle_refuses() {
    let mut world = World::new(1, []).unwrap();
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[
            Action::Ensure(
                ContentId(1),
                Content::Branch(vec![reference(2, ContentKind::Branch)]),
            ),
            Action::Ensure(
                ContentId(2),
                Content::Branch(vec![reference(1, ContentKind::Branch)]),
            ),
        ],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ContentCycle
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn a_mutable_name_is_inert_immutable_data() {
    let missing = Place::root(0, 99);
    let mut world = World::new(0, []).unwrap();
    let candidate = prepare(
        &world.snapshot(),
        &[Action::Ensure(ContentId(1), Content::Name(missing.clone()))],
        Limits::default(),
    )
    .unwrap();
    assert!(
        candidate.support().is_empty(),
        "a weak name must not read its target"
    );
    world.admit(candidate).unwrap();
    assert_eq!(
        world.snapshot().contents(),
        &[(ContentId(1), Content::Name(missing))].into()
    );
    assert!(
        world.snapshot().entries().is_empty(),
        "a weak name must not create its target"
    );
}

#[test]
fn a_mutable_capacity_failure_prevents_content_publication() {
    let mut world = World::new(0, []).unwrap();
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[
            Action::Ensure(ContentId(1), Content::Leaf(Payload(7))),
            Action::Add(Place::root(0, 1), Payload(8)),
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
fn a_content_failure_prevents_mutable_publication() {
    let mut world = World::new(1, []).unwrap();
    let before = world.snapshot();
    let candidate = prepare(
        &before,
        &[
            Action::Add(Place::root(0, 1), Payload(8)),
            Action::Ensure(
                ContentId(1),
                Content::Branch(vec![reference(2, ContentKind::Leaf)]),
            ),
        ],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        world.admit(candidate).unwrap_err().kind,
        FailureKind::ContentMissing
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn a_fifth_content_link_refuses_before_copying() {
    let world = World::new(1, []).unwrap();
    let failure = prepare(
        &world.snapshot(),
        &[Action::Ensure(
            ContentId(1),
            Content::Branch(vec![reference(2, ContentKind::Leaf); 5]),
        )],
        Limits::default(),
    )
    .unwrap_err();
    assert_eq!(failure.kind, FailureKind::ModelBound);
    assert_eq!(failure.work_used(), 0);
}
