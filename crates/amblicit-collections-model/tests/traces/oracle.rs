//! Semantic oracle using array membership and committed requests, not model facts.

use super::program::{Cell, ExpectedRead, Reason, Request, Selection, Space, State};

pub struct Observation {
    basis: State,
    history_at: usize,
    pub requests: Vec<Request>,
    pub readings: Vec<ExpectedRead>,
}

pub struct Oracle {
    pub state: State,
    capacity: u16,
    history: Vec<Vec<Request>>,
}

impl Oracle {
    pub fn new(state: State, capacity: u16) -> Self {
        Self {
            state,
            capacity,
            history: vec![],
        }
    }

    pub fn prepare(&self, requests: &[Request]) -> Result<Observation, Reason> {
        let mut readings = vec![];
        let mut mutations = [false; 6];
        let mut duplicate = false;
        for request in requests {
            if !matches!(request, Request::Add(_, _))
                && request
                    .space()
                    .owner()
                    .is_some_and(|owner| self.state.get(owner).is_none())
            {
                return Err(Reason::Unavailable);
            }
            if let Some(cell) = request.mutation() {
                duplicate |= mutations[cell.index()];
                mutations[cell.index()] = true;
            }
            match *request {
                Request::Get(cell) => readings.push(ExpectedRead::Slot(self.state.get(cell))),
                Request::Iter(space, selection) => readings.push(self.entries(space, selection)),
                Request::Add(_, _) | Request::Remove(_) | Request::Replace(_, _) => {}
            }
        }
        if duplicate {
            return Err(Reason::DuplicateMutation);
        }
        Ok(Observation {
            basis: self.state,
            history_at: self.history.len(),
            requests: requests.to_vec(),
            readings,
        })
    }

    fn entries(&self, space: Space, selection: Selection) -> ExpectedRead {
        let values = space
            .cells()
            .into_iter()
            .filter_map(|cell| {
                let value = self.state.get(cell)?;
                match selection {
                    Selection::Equal(expected) if expected != value => None,
                    Selection::All | Selection::Equal(_) => Some((cell, value)),
                }
            })
            .collect();
        ExpectedRead::Entries(values)
    }

    pub fn admit(&mut self, observation: &Observation) -> Result<(), Reason> {
        for changes in &self.history[observation.history_at..] {
            if observation
                .requests
                .iter()
                .any(|request| changes.iter().any(|change| interferes(*request, *change)))
            {
                return Err(Reason::Conflict);
            }
        }
        for cell in Cell::ALL {
            if let Some(request) = observation
                .requests
                .iter()
                .find(|request| request.mutation() == Some(cell))
            {
                validate_domain(observation.basis, *request)?;
            }
        }
        let mut proposed = self.state;
        for request in &observation.requests {
            proposed.apply(*request);
        }
        if proposed.has_orphan() {
            return Err(Reason::OwnerSurvival);
        }
        if proposed.over_capacity(self.capacity) {
            return Err(Reason::Capacity);
        }
        self.state = proposed;
        let writes: Vec<_> = observation
            .requests
            .iter()
            .copied()
            .filter(|request| request.mutation().is_some())
            .collect();
        if !writes.is_empty() {
            self.history.push(writes);
        }
        Ok(())
    }
}

fn validate_domain(basis: State, request: Request) -> Result<(), Reason> {
    match request {
        Request::Add(cell, _)
            if cell
                .space()
                .owner()
                .is_some_and(|owner| basis.get(owner).is_none()) =>
        {
            Err(Reason::OwnerMissing)
        }
        Request::Add(cell, _) if basis.get(cell).is_some() => Err(Reason::AlreadyPresent),
        Request::Remove(cell) | Request::Replace(cell, _) if basis.get(cell).is_none() => {
            Err(Reason::NotFound)
        }
        Request::Remove(Cell::Owner) if basis.count(Space::Children) != 0 => Err(Reason::NotEmpty),
        _ => Ok(()),
    }
}

fn interferes(observed: Request, changed: Request) -> bool {
    let Some(changed_cell) = changed.mutation() else {
        return false;
    };
    let membership = matches!(changed, Request::Add(_, _) | Request::Remove(_));
    if membership && observed.space().owner() == Some(changed_cell) {
        return true;
    }
    match observed {
        Request::Get(cell) | Request::Replace(cell, _) => cell == changed_cell,
        Request::Iter(space, _) => space == changed_cell.space(),
        Request::Add(cell, _) => {
            cell == changed_cell || (membership && cell.space() == changed_cell.space())
        }
        Request::Remove(cell) => {
            cell == changed_cell
                || (membership && cell.space() == changed_cell.space())
                || (membership && changed_cell.space().owner() == Some(cell))
        }
    }
}
