//! Draft standalone collections model. Production crates do not depend on it.
//!
//! Whole-candidate capacity and exclusive mutations are implemented. The other
//! contract laws remain planned; this is not yet a conforming evaluator.

use std::collections::{BTreeMap, BTreeSet};

/// A tuple path: each component is an owning-collection field and storage key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place(Vec<(u8, u8)>);

impl Place {
    #[must_use]
    pub fn root(field: u8, key: u8) -> Self {
        Self(vec![(field, key)])
    }
}

/// Opaque application data; it contains no owning collections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload(pub i64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    capacity: u16,
    entries: BTreeMap<Place, Payload>,
}

impl Snapshot {
    #[must_use]
    pub fn entries(&self) -> &BTreeMap<Place, Payload> {
        &self.entries
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Add(Place, Payload),
    Remove(Place),
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub work: u32,
    pub scan: u16,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            work: 10_000,
            scan: 256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Capacity,
    DuplicateMutation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct World {
    state: Snapshot,
}

impl World {
    #[must_use]
    pub fn new(capacity: u16, entries: impl IntoIterator<Item = (Place, Payload)>) -> Self {
        Self {
            state: Snapshot {
                capacity,
                entries: entries.into_iter().collect(),
            },
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.state.clone()
    }

    /// Apply a whole candidate atomically when its resulting counts fit.
    ///
    /// # Errors
    /// Returns `Capacity` if any collection's final count exceeds its bound.
    pub fn admit(&mut self, candidate: Candidate) -> Result<(), Failure> {
        let mut proposed = self.state.entries.clone();
        for action in candidate.actions {
            match action {
                Action::Add(place, payload) => {
                    proposed.insert(place, payload);
                }
                Action::Remove(place) => {
                    proposed.remove(&place);
                }
            }
        }
        let mut counts = BTreeMap::new();
        for place in proposed.keys() {
            let mut collection = place.0.clone();
            if let Some((_, key)) = collection.last_mut() {
                *key = 0;
            }
            *counts.entry(collection).or_insert(0_usize) += 1;
        }
        if counts
            .values()
            .any(|count| *count > usize::from(self.state.capacity))
        {
            return Err(Failure {
                kind: FailureKind::Capacity,
            });
        }
        self.state.entries = proposed;
        Ok(())
    }
}

/// Construct an inert proposal from requests.
///
/// # Errors
/// Returns `DuplicateMutation` for two exclusive requests at the same place.
pub fn prepare(
    _basis: &Snapshot,
    actions: &[Action],
    _limits: Limits,
) -> Result<Candidate, Failure> {
    let mut places = BTreeSet::new();
    for action in actions {
        let (Action::Add(place, _) | Action::Remove(place)) = action;
        if !places.insert(place) {
            return Err(Failure {
                kind: FailureKind::DuplicateMutation,
            });
        }
    }
    Ok(Candidate {
        actions: actions.to_vec(),
    })
}
