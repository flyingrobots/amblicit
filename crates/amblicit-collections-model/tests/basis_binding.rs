//! Origin-binding cases use real independent model worlds, not mock authorities.

use amblicit_collections_model::{
    prepare, Action, FailureKind, Limits, Payload, Place, Reading, World,
};

#[test]
fn a_reading_from_another_world_cannot_be_corroborated() {
    let place = Place::root(0, 1);
    let source = World::new(1, [(place.clone(), Payload(7))]).unwrap();
    let mut destination = World::new(1, [(place.clone(), Payload(8))]).unwrap();
    let before = destination.snapshot();
    let candidate = prepare(&source.snapshot(), &[Action::Get(place)], Limits::default()).unwrap();
    assert_eq!(candidate.readings(), [Reading::Get(Some(Payload(7)))]);
    assert_eq!(
        destination.admit(candidate).unwrap_err().kind,
        FailureKind::BasisMismatch
    );
    assert_eq!(destination.snapshot(), before);
}

#[test]
fn equal_membership_does_not_transfer_mutation_authority() {
    let source = World::new(1, []).unwrap();
    let mut destination = World::new(1, []).unwrap();
    let before = destination.snapshot();
    let candidate = prepare(
        &source.snapshot(),
        &[Action::Add(Place::root(0, 1), Payload(7))],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        destination.admit(candidate).unwrap_err().kind,
        FailureKind::BasisMismatch
    );
    assert_eq!(destination.snapshot(), before);
}

#[test]
fn an_empty_candidate_still_belongs_to_its_world() {
    let source = World::new(1, []).unwrap();
    let mut destination = World::new(1, []).unwrap();
    let before = destination.snapshot();
    let candidate = prepare(&source.snapshot(), &[], Limits::default()).unwrap();
    assert_eq!(
        destination.admit(candidate).unwrap_err().kind,
        FailureKind::BasisMismatch
    );
    assert_eq!(destination.snapshot(), before);
}

#[test]
fn cloning_a_snapshot_preserves_its_origin() {
    let place = Place::root(0, 1);
    let mut world = World::new(1, [(place.clone(), Payload(7))]).unwrap();
    let snapshot = world.snapshot();
    let copy = snapshot.clone();
    let candidate = prepare(
        &copy,
        &[Action::Replace(place.clone(), Payload(8))],
        Limits::default(),
    )
    .unwrap();
    world.admit(candidate).unwrap();
    assert_eq!(world.snapshot().entries(), &[(place, Payload(8))].into());
    assert_eq!(copy, snapshot);
}
