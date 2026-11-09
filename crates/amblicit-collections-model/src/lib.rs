//! Draft standalone collections model. Production crates do not depend on it.
//!
//! Capacity, exclusive mutations, snapshot observations, and ownership are
//! implemented. Work bounds and immutable content remain planned; this is not
//! yet a conforming evaluator.

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

    fn owner_place(&self) -> Option<Place> {
        let (&(field, key), ancestors) = self.owner.split_last()?;
        Some(Place {
            collection: Self {
                owner: ancestors.to_vec(),
                field,
            },
            key,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Support {
    Slot(Place),
    Range(Collection),
    Owner(Place),
    Children(Place),
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

    #[must_use]
    pub fn child(&self, field: u8, key: u8) -> Self {
        let mut owner = self.collection.owner.clone();
        owner.push((self.collection.field, self.key));
        Self {
            collection: Collection { owner, field },
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

    fn has_children(&self, owner: &Place) -> bool {
        self.entries
            .keys()
            .any(|place| place.collection.owner_place().as_ref() == Some(owner))
    }

    fn validate_domains(&self, candidate: &Candidate) -> Result<(), Failure> {
        let mutations: BTreeMap<_, _> = candidate
            .actions
            .iter()
            .filter_map(|action| action.mutation_place().map(|place| (place, action)))
            .collect();
        for (place, action) in mutations {
            let kind = match action {
                Action::Add(_, _)
                    if place
                        .collection
                        .owner_place()
                        .is_some_and(|owner| !self.entries.contains_key(&owner)) =>
                {
                    Some(FailureKind::OwnerMissing)
                }
                Action::Add(_, _) if self.entries.contains_key(place) => {
                    Some(FailureKind::AlreadyPresent)
                }
                Action::Remove(_) | Action::Replace(_, _) if !self.entries.contains_key(place) => {
                    Some(FailureKind::NotFound)
                }
                Action::Remove(_) if self.has_children(place) => Some(FailureKind::NotEmpty),
                _ => None,
            };
            if let Some(kind) = kind {
                return Err(Failure { kind });
            }
        }
        Ok(())
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

impl Action {
    fn mutation_place(&self) -> Option<&Place> {
        match self {
            Self::Add(place, _) | Self::Remove(place) | Self::Replace(place, _) => Some(place),
            Self::Get(_) | Self::Iter(_, _) => None,
        }
    }
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
    NotEmpty,
    OwnerMissing,
    OwnerSurvival,
    AlreadyPresent,
    NotFound,
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
            versions.insert(Support::Owner(place.clone()), 1);
            if let Some(owner) = place.collection.owner_place() {
                versions.insert(Support::Children(owner), 1);
            }
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
    /// Returns `Conflict` for stale evidence; a membership or ownership failure
    /// for an obstructed mutation; `Capacity` for an oversized final collection;
    /// or `ModelBound` when the model's version counter is exhausted.
    pub fn admit(&mut self, candidate: Candidate) -> Result<(), Failure> {
        for (key, version) in &candidate.support {
            if self.state.version(key) != *version {
                return Err(Failure {
                    kind: FailureKind::Conflict,
                });
            }
        }
        self.state.validate_domains(&candidate)?;
        let mut proposed = self.state.entries.clone();
        let mut touched = BTreeSet::new();
        for action in candidate.actions {
            match action {
                Action::Add(place, payload) => {
                    touched.insert(Support::Slot(place.clone()));
                    touched.insert(Support::Range(place.collection.clone()));
                    touched.insert(Support::Owner(place.clone()));
                    if let Some(owner) = place.collection.owner_place() {
                        touched.insert(Support::Children(owner));
                    }
                    proposed.insert(place, payload);
                }
                Action::Remove(place) => {
                    touched.insert(Support::Slot(place.clone()));
                    touched.insert(Support::Range(place.collection.clone()));
                    touched.insert(Support::Owner(place.clone()));
                    if let Some(owner) = place.collection.owner_place() {
                        touched.insert(Support::Children(owner));
                    }
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
            if place
                .collection
                .owner_place()
                .is_some_and(|owner| !proposed.contains_key(&owner))
            {
                return Err(Failure {
                    kind: FailureKind::OwnerSurvival,
                });
            }
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
                candidate.observe_owner(
                    basis,
                    &place.collection,
                    matches!(action, Action::Remove(_)),
                )?;
                candidate.observe(basis, Support::Slot(place.clone()));
                candidate.observe(basis, Support::Range(place.collection.clone()));
                if matches!(action, Action::Remove(_)) {
                    candidate.observe(basis, Support::Children(place.clone()));
                }
            }
            Action::Replace(place, _) => {
                candidate.observe_owner(basis, &place.collection, true)?;
                candidate.observe(basis, Support::Slot(place.clone()));
            }
            Action::Get(place) => {
                candidate.observe_owner(basis, &place.collection, true)?;
                candidate.observe(basis, Support::Slot(place.clone()));
                candidate
                    .readings
                    .push(Reading::Get(basis.entries.get(place).copied()));
            }
            Action::Iter(collection, predicate) => {
                candidate.observe_owner(basis, collection, true)?;
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
    fn observe_owner(
        &mut self,
        basis: &Snapshot,
        collection: &Collection,
        require_present: bool,
    ) -> Result<(), Failure> {
        if let Some(owner) = collection.owner_place() {
            if require_present && !basis.entries.contains_key(&owner) {
                return Err(Failure {
                    kind: FailureKind::Unavailable,
                });
            }
            self.observe(basis, Support::Owner(owner));
        }
        Ok(())
    }

    fn observe(&mut self, basis: &Snapshot, key: Support) {
        let version = basis.version(&key);
        self.support.insert(key, version);
    }
}
