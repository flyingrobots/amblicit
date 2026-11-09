//! Translation of finite fixture identities to the model's public API.

use super::program::{Cell, ExpectedRead, Reason, Request, Selection, Space, State, Violation};
use amblicit_collections_model::{
    Action, Collection, FailureKind, Payload, Place, Predicate, Reading, Snapshot,
};

pub fn place(cell: Cell) -> Place {
    match cell {
        Cell::Owner => Place::root(1, 0),
        Cell::Peer => Place::root(1, 1),
        Cell::RemoteA => Place::root(2, 0),
        Cell::RemoteB => Place::root(2, 1),
        Cell::ChildA => Place::root(1, 0).child(3, 0).unwrap(),
        Cell::ChildB => Place::root(1, 0).child(3, 1).unwrap(),
    }
}

fn collection(space: Space) -> Collection {
    match space {
        Space::Owners => Collection::root(1),
        Space::Remote => Collection::root(2),
        Space::Children => place(Cell::Owner).children(3).unwrap(),
    }
}

pub fn action(request: Request) -> Action {
    match request {
        Request::Add(cell, value) => Action::Add(place(cell), Payload(value)),
        Request::Remove(cell) => Action::Remove(place(cell)),
        Request::Replace(cell, value) => Action::Replace(place(cell), Payload(value)),
        Request::Get(cell) => Action::Get(place(cell)),
        Request::Iter(space, selection) => Action::Iter(
            collection(space),
            match selection {
                Selection::All => Predicate::All,
                Selection::Equal(value) => Predicate::Equal(Payload(value)),
            },
        ),
    }
}

pub fn reading(expected: &ExpectedRead) -> Reading {
    match expected {
        ExpectedRead::Slot(value) => Reading::Get(value.map(Payload)),
        ExpectedRead::Entries(entries) => Reading::Entries(
            entries
                .iter()
                .map(|(cell, value)| (place(*cell), Payload(*value)))
                .collect(),
        ),
    }
}

pub fn reason(expected: Reason) -> FailureKind {
    match expected {
        Reason::Unavailable => FailureKind::Unavailable,
        Reason::DuplicateMutation => FailureKind::DuplicateMutation,
        Reason::Conflict => FailureKind::Conflict,
        Reason::OwnerMissing => FailureKind::OwnerMissing,
        Reason::AlreadyPresent => FailureKind::AlreadyPresent,
        Reason::NotFound => FailureKind::NotFound,
        Reason::NotEmpty => FailureKind::NotEmpty,
        Reason::OwnerSurvival => FailureKind::OwnerSurvival,
        Reason::Capacity => FailureKind::Capacity,
    }
}

pub fn state(snapshot: &Snapshot) -> Result<State, Violation> {
    if !snapshot.contents().is_empty() {
        return Err(Violation::UnexpectedContent);
    }
    let mut state = State::EMPTY;
    for (actual, payload) in snapshot.entries() {
        let cell = Cell::ALL
            .into_iter()
            .find(|cell| place(*cell) == *actual)
            .ok_or(Violation::UnknownPlace)?;
        state.set(cell, Some(payload.0));
    }
    Ok(state)
}
