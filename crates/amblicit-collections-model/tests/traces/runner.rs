//! Public-model execution compared with the independent trace oracle.

use super::{
    adapter,
    oracle::{Observation, Oracle},
    program::{Actor, Cell, Reason, Request, State, Step, Trace, Violation},
};
use amblicit_collections_model::{prepare, Candidate, Limits, Payload, World};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    Accepted,
    Refused,
    Read,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceFailure {
    pub kind: Violation,
    pub step: Option<usize>,
    pub expected: State,
    pub actual: Option<State>,
}

struct Prepared {
    candidate: Candidate,
    observation: Observation,
}

struct Interpreter {
    world: World,
    oracle: Oracle,
    pending: [Option<Prepared>; 2],
    events: BTreeSet<Event>,
    capacity: u16,
}

pub fn run(trace: &Trace) -> Result<BTreeSet<Event>, Box<TraceFailure>> {
    let early = |kind| {
        Box::new(TraceFailure {
            kind,
            step: None,
            expected: trace.initial,
            actual: None,
        })
    };
    if !trace.valid() {
        return Err(early(Violation::InvalidTrace));
    }
    let initial = Cell::ALL.into_iter().filter_map(|cell| {
        trace
            .initial
            .get(cell)
            .map(|value| (adapter::place(cell), Payload(value)))
    });
    let world = World::new(trace.capacity, initial).map_err(|_| early(Violation::Construction))?;
    let mut interpreter = Interpreter {
        world,
        oracle: Oracle::new(trace.initial, trace.capacity),
        pending: [None, None],
        events: BTreeSet::new(),
        capacity: trace.capacity,
    };
    for (index, step) in trace.steps.iter().enumerate() {
        interpreter.step(step).map_err(|kind| {
            Box::new(TraceFailure {
                kind,
                step: Some(index),
                expected: interpreter.oracle.state,
                actual: adapter::state(&interpreter.world.snapshot()).ok(),
            })
        })?;
    }
    Ok(interpreter.events)
}

impl Interpreter {
    fn step(&mut self, step: &Step) -> Result<(), Violation> {
        match step {
            Step::Prepare { actor, requests } => self.prepare(*actor, requests)?,
            Step::Commit(actor) => self.commit(*actor)?,
        }
        if adapter::state(&self.world.snapshot())? != self.oracle.state {
            return Err(Violation::Membership);
        }
        Ok(())
    }

    fn prepare(&mut self, actor: Actor, requests: &[Request]) -> Result<(), Violation> {
        self.pending[actor.index()] = None;
        let actions: Vec<_> = requests.iter().copied().map(adapter::action).collect();
        let expected = self.oracle.prepare(requests);
        let actual = prepare(&self.world.snapshot(), &actions, Limits::default());
        match (expected, actual) {
            (Ok(observation), Ok(candidate)) => {
                let readings: Vec<_> = observation.readings.iter().map(adapter::reading).collect();
                if candidate.readings() != readings {
                    return Err(Violation::Readings);
                }
                if !readings.is_empty() {
                    self.events.insert(Event::Read);
                }
                self.pending[actor.index()] = Some(Prepared {
                    candidate,
                    observation,
                });
            }
            (Err(expected), Err(actual)) if adapter::reason(expected) == actual.kind => {
                self.events.insert(Event::Refused);
            }
            _ => return Err(Violation::Preparation),
        }
        Ok(())
    }

    fn commit(&mut self, actor: Actor) -> Result<(), Violation> {
        let Some(prepared) = self.pending[actor.index()].take() else {
            return Ok(());
        };
        let before = self.world.snapshot();
        let expected = self.oracle.admit(&prepared.observation);
        let actual = self.world.admit(prepared.candidate);
        let after = self.world.snapshot();
        let state = adapter::state(&after)?;
        if state.over_capacity(self.capacity) {
            return Err(Violation::Capacity);
        }
        if state.has_orphan() {
            return Err(Violation::Ownership);
        }
        if actual.is_err() && before != after {
            return Err(Violation::AtomicRefusal);
        }
        match (expected, actual) {
            (Ok(()), Ok(_)) => {
                self.events.insert(Event::Accepted);
            }
            (Err(Reason::Conflict), Ok(_)) => return Err(Violation::StaleAccepted),
            (Err(_), Ok(_)) => return Err(Violation::UnexpectedAdmission),
            (Ok(()), Err(_)) => return Err(Violation::UnexpectedRefusal),
            (Err(expected), Err(actual)) => {
                if adapter::reason(expected) != actual.kind {
                    return Err(Violation::WrongRefusal);
                }
                self.events.insert(Event::Refused);
                if expected == Reason::Conflict {
                    self.events.insert(Event::Conflict);
                }
            }
        }
        Ok(())
    }
}
