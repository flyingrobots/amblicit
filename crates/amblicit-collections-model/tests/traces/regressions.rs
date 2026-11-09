//! Exact deletion-minimal traces retained from seed 159 under three guard faults.

use super::program::{Actor, Cell, Request, Selection, Space, State, Step, Trace};

pub fn capacity() -> Trace {
    Trace {
        seed: 159,
        case: 0,
        capacity: 1,
        initial: State([None, None, Some(0), None, None, None]),
        steps: vec![
            Step::Prepare {
                actor: Actor::First,
                requests: vec![Request::Add(Cell::RemoteB, 2)],
            },
            Step::Commit(Actor::First),
        ],
    }
}

pub fn empty_range() -> Trace {
    Trace {
        seed: 159,
        case: 1,
        capacity: 1,
        initial: State::EMPTY,
        steps: vec![
            Step::Prepare {
                actor: Actor::First,
                requests: vec![Request::Iter(Space::Remote, Selection::Equal(-1))],
            },
            Step::Prepare {
                actor: Actor::Second,
                requests: vec![Request::Add(Cell::RemoteB, 0)],
            },
            Step::Commit(Actor::Second),
            Step::Commit(Actor::First),
        ],
    }
}

pub fn ownership() -> Trace {
    Trace {
        seed: 159,
        case: 2,
        capacity: 1,
        initial: State([Some(-1), None, None, None, None, None]),
        steps: vec![
            Step::Prepare {
                actor: Actor::First,
                requests: vec![Request::Remove(Cell::Owner), Request::Add(Cell::ChildB, 0)],
            },
            Step::Commit(Actor::First),
        ],
    }
}
