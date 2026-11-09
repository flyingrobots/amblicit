//! Draft standalone collections model. Production crates do not depend on it.
//!
//! Capacity, exclusive mutations, and snapshot observations are implemented.
//! Ownership and work bounds remain planned; this is not a conforming evaluator.

use std::collections::{BTreeMap, BTreeSet};

/// A tuple path: each component is an owning-collection field and storage key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place {
    collection: Collection,
    key: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Collection {
    owner: Vec<(u8, u8)>,
    field: u8,
}

impl Collection {
    #[must_use]
    pub fn root(field: u8) -> Self {
        Self {
            owner: vec![],
            field,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Support {
    Slot(Place),
    Range(Collection),
}

#[derive(Clone, Copy, Debug)]
pub enum Predicate {
    All,
    Equal(Payload),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    Get(Option<Payload>),
    Entries(Vec<(Place, Payload)>),
}

impl Place {
    #[must_use]
    pub fn root(field: u8, key: u8) -> Self {
        Self {
            collection: Collection::root(field),
            key,
        }
    }
}

/// Opaque application data; it contains no owning collections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload(pub i64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    capacity: u16,
    entries: BTreeMap<Place, Payload>,
    epoch: u64,
    versions: BTreeMap<Support, u64>,
}

impl Snapshot {
    #[must_use]
    pub fn entries(&self) -> &BTreeMap<Place, Payload> {
        &self.entries
    }

    fn version(&self, key: &Support) -> u64 {
        self.versions.get(key).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Add(Place, Payload),
    Remove(Place),
    Replace(Place, Payload),
    Get(Place),
    Iter(Collection, Predicate),
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
    Conflict,
    Unavailable,
    ModelBound,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    actions: Vec<Action>,
    readings: Vec<Reading>,
    support: BTreeMap<Support, u64>,
}

impl Candidate {
    #[must_use]
    pub fn readings(&self) -> &[Reading] {
        &self.readings
    }

    #[must_use]
    pub fn support(&self) -> &BTreeMap<Support, u64> {
        &self.support
    }
}

#[derive(Clone, Debug)]
pub struct World {
    state: Snapshot,
}

impl World {
    #[must_use]
    pub fn new(capacity: u16, entries: impl IntoIterator<Item = (Place, Payload)>) -> Self {
        let entries: BTreeMap<Place, Payload> = entries.into_iter().collect();
        let mut versions = BTreeMap::new();
        for place in entries.keys() {
            versions.insert(Support::Slot(place.clone()), 1);
            versions.insert(Support::Range(place.collection.clone()), 1);
        }
        Self {
            state: Snapshot {
                capacity,
                entries,
                epoch: 1,
                versions,
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
    /// Returns `Conflict` for stale evidence, `Capacity` for an oversized final
    /// collection, or `ModelBound` when the model's version counter is exhausted.
    pub fn admit(&mut self, candidate: Candidate) -> Result<(), Failure> {
        for (key, version) in candidate.support {
            if self.state.version(&key) != version {
                return Err(Failure {
                    kind: FailureKind::Conflict,
                });
            }
        }
        let mut proposed = self.state.entries.clone();
        let mut touched = BTreeSet::new();
        for action in candidate.actions {
            match action {
                Action::Add(place, payload) => {
                    touched.insert(Support::Slot(place.clone()));
                    touched.insert(Support::Range(place.collection.clone()));
                    proposed.insert(place, payload);
                }
                Action::Remove(place) => {
                    touched.insert(Support::Slot(place.clone()));
                    touched.insert(Support::Range(place.collection.clone()));
                    proposed.remove(&place);
                }
                Action::Replace(place, payload) => {
                    touched.insert(Support::Slot(place.clone()));
                    proposed.insert(place, payload);
                }
                Action::Get(_) | Action::Iter(_, _) => {}
            }
        }
        let mut counts = BTreeMap::new();
        for place in proposed.keys() {
            *counts.entry(&place.collection).or_insert(0_usize) += 1;
        }
        if counts
            .values()
            .any(|count| *count > usize::from(self.state.capacity))
        {
            return Err(Failure {
                kind: FailureKind::Capacity,
            });
        }
        if !touched.is_empty() {
            let epoch = self.state.epoch.checked_add(1).ok_or(Failure {
                kind: FailureKind::ModelBound,
            })?;
            for key in touched {
                self.state.versions.insert(key, epoch);
            }
            self.state.epoch = epoch;
            self.state.entries = proposed;
        }
        Ok(())
    }
}

/// Construct an inert proposal from requests.
///
/// # Errors
/// Returns `DuplicateMutation` for two exclusive requests at the same place.
pub fn prepare(
    basis: &Snapshot,
    actions: &[Action],
    _limits: Limits,
) -> Result<Candidate, Failure> {
    let mut places = BTreeSet::new();
    for action in actions {
        let place = match action {
            Action::Add(place, _) | Action::Remove(place) | Action::Replace(place, _) => place,
            Action::Get(_) | Action::Iter(_, _) => continue,
        };
        if !places.insert(place) {
            return Err(Failure {
                kind: FailureKind::DuplicateMutation,
            });
        }
    }
    let mut candidate = Candidate {
        actions: actions.to_vec(),
        readings: vec![],
        support: BTreeMap::new(),
    };
    for action in actions {
        match action {
            Action::Add(place, _) | Action::Remove(place) => {
                candidate.observe(basis, Support::Slot(place.clone()));
                candidate.observe(basis, Support::Range(place.collection.clone()));
            }
            Action::Replace(place, _) => {
                candidate.observe(basis, Support::Slot(place.clone()));
            }
            Action::Get(place) => {
                candidate.observe(basis, Support::Slot(place.clone()));
                candidate
                    .readings
                    .push(Reading::Get(basis.entries.get(place).copied()));
            }
            Action::Iter(collection, predicate) => {
                candidate.observe(basis, Support::Range(collection.clone()));
                let start = Place {
                    collection: collection.clone(),
                    key: 0,
                };
                let end = Place {
                    collection: collection.clone(),
                    key: u8::MAX,
                };
                let mut entries = vec![];
                for (place, payload) in basis.entries.range(start..=end) {
                    candidate.observe(basis, Support::Slot(place.clone()));
                    if matches!(predicate, Predicate::All)
                        || matches!(predicate, Predicate::Equal(value) if value == payload)
                    {
                        entries.push((place.clone(), *payload));
                    }
                }
                candidate.readings.push(Reading::Entries(entries));
            }
        }
    }
    Ok(candidate)
}

impl Candidate {
    fn observe(&mut self, basis: &Snapshot, key: Support) {
        let version = basis.version(&key);
        self.support.insert(key, version);
    }
}
