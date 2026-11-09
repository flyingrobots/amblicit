//! Draft standalone collections model. Production crates do not depend on it.
//!
//! Capacity, exclusive mutations, snapshot observations, and ownership are
//! implemented, with shared operation work/scan limits and a finite state/history
//! envelope. Immutable content remains planned; this is not a conforming evaluator.

use std::collections::{BTreeMap, BTreeSet};

mod bounds;
mod origin;

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

    /// Construct a place in an owned collection.
    ///
    /// # Errors
    /// Returns `ModelBound` when the owner path exceeds the model envelope.
    pub fn child(&self, field: u8, key: u8) -> Result<Self, Failure> {
        if self.collection.owner.len() >= bounds::MAX_OWNER_DEPTH {
            return Err(bounds::failure(FailureKind::ModelBound));
        }
        let mut owner = self.collection.owner.clone();
        owner.push((self.collection.field, self.key));
        Ok(Self {
            collection: Collection { owner, field },
            key,
        })
    }

    /// Select an owned collection without changing its membership.
    ///
    /// # Errors
    /// Returns `ModelBound` when the owner path exceeds the model envelope.
    pub fn children(&self, field: u8) -> Result<Collection, Failure> {
        self.child(field, 0).map(|place| place.collection)
    }
}

/// Opaque application data; it contains no owning collections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload(pub i64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    origin: origin::Origin,
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

    fn has_children(&self, owner: &Place, meter: &mut Meter) -> Result<bool, Failure> {
        for place in self.entries.keys() {
            meter.charge(1)?;
            if place.collection.owner_place().as_ref() == Some(owner) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn validate_domains(&self, candidate: &Candidate, meter: &mut Meter) -> Result<(), Failure> {
        let mutations: BTreeMap<_, _> = candidate
            .actions
            .iter()
            .filter_map(|action| action.mutation_place().map(|place| (place, action)))
            .collect();
        for (place, action) in mutations {
            meter.charge(1)?;
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
                Action::Remove(_) if self.has_children(place, meter)? => {
                    Some(FailureKind::NotEmpty)
                }
                _ => None,
            };
            if let Some(kind) = kind {
                return Err(meter.failure(kind));
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
    BasisMismatch,
    Unavailable,
    ModelBound,
    NotEmpty,
    OwnerMissing,
    OwnerSurvival,
    AlreadyPresent,
    NotFound,
    WorkExhausted,
    Incomplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    work_used: u32,
}

impl Failure {
    #[must_use]
    pub fn work_used(&self) -> u32 {
        self.work_used
    }
}

#[derive(Clone, Copy, Debug)]
struct Meter {
    limits: Limits,
    used: u32,
    scans: u16,
}

impl Meter {
    fn failure(&self, kind: FailureKind) -> Failure {
        Failure {
            kind,
            work_used: self.used,
        }
    }

    fn charge(&mut self, units: usize) -> Result<(), Failure> {
        let units = u32::try_from(units).map_err(|_| self.failure(FailureKind::WorkExhausted))?;
        let next = self
            .used
            .checked_add(units)
            .filter(|next| *next <= self.limits.work)
            .ok_or_else(|| self.failure(FailureKind::WorkExhausted))?;
        self.used = next;
        Ok(())
    }

    fn inspect(&mut self) -> Result<(), Failure> {
        if self.scans >= self.limits.scan {
            return Err(self.failure(FailureKind::Incomplete));
        }
        self.charge(1)?;
        self.scans += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub work_used: u32,
}

#[derive(Clone, Debug)]
#[must_use]
pub struct Candidate {
    origin: origin::Origin,
    actions: Vec<Action>,
    readings: Vec<Reading>,
    support: BTreeMap<Support, u64>,
    meter: Meter,
}

impl Candidate {
    #[must_use]
    pub fn work_used(&self) -> u32 {
        self.meter.used
    }

    #[must_use]
    pub fn readings(&self) -> &[Reading] {
        &self.readings
    }

    #[must_use]
    pub fn support(&self) -> &BTreeMap<Support, u64> {
        &self.support
    }
}

#[derive(Debug)]
pub struct World {
    state: Snapshot,
}

impl World {
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.state.clone()
    }

    /// Apply a whole candidate atomically when its resulting counts fit.
    ///
    /// # Errors
    /// Returns `BasisMismatch` for another world's evidence, `Conflict` for stale
    /// evidence, a membership or ownership failure
    /// for an obstructed mutation; `Capacity` for an oversized final collection;
    /// or `ModelBound` when the model's version counter is exhausted.
    pub fn admit(&mut self, candidate: Candidate) -> Result<Receipt, Failure> {
        let mut meter = candidate.meter;
        if candidate.origin != self.state.origin {
            return Err(meter.failure(FailureKind::BasisMismatch));
        }
        for (key, version) in &candidate.support {
            meter.charge(1)?;
            if self.state.version(key) != *version {
                return Err(meter.failure(FailureKind::Conflict));
            }
        }
        if candidate
            .actions
            .iter()
            .all(|action| action.mutation_place().is_none())
        {
            return Ok(Receipt {
                work_used: meter.used,
            });
        }
        if self.state.epoch > bounds::MAX_MUTATING_COMMITS {
            return Err(meter.failure(FailureKind::ModelBound));
        }
        self.state.validate_domains(&candidate, &mut meter)?;
        meter.charge(self.state.entries.len())?;
        let mut proposed = self.state.entries.clone();
        let mut touched = BTreeSet::new();
        for action in candidate.actions {
            if action.mutation_place().is_some() {
                meter.charge(1)?;
            }
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
        if proposed.len() > bounds::MAX_MEMBERS {
            return Err(meter.failure(FailureKind::ModelBound));
        }
        let mut counts = BTreeMap::new();
        for place in proposed.keys() {
            meter.charge(1)?;
            if place
                .collection
                .owner_place()
                .is_some_and(|owner| !proposed.contains_key(&owner))
            {
                return Err(meter.failure(FailureKind::OwnerSurvival));
            }
            *counts.entry(&place.collection).or_insert(0_usize) += 1;
        }
        if counts
            .values()
            .any(|count| *count > usize::from(self.state.capacity))
        {
            return Err(meter.failure(FailureKind::Capacity));
        }
        if !touched.is_empty() {
            let epoch = self
                .state
                .epoch
                .checked_add(1)
                .ok_or_else(|| meter.failure(FailureKind::ModelBound))?;
            meter.charge(touched.len())?;
            for key in touched {
                self.state.versions.insert(key, epoch);
            }
            self.state.epoch = epoch;
            self.state.entries = proposed;
        }
        Ok(Receipt {
            work_used: meter.used,
        })
    }
}

/// Construct an inert proposal from requests on one fixed snapshot.
///
/// # Errors
/// Returns a typed observation failure, exhausted work, incomplete scan coverage,
/// or duplicate exclusive mutation. A failed observation yields no candidate.
pub fn prepare(basis: &Snapshot, actions: &[Action], limits: Limits) -> Result<Candidate, Failure> {
    if actions.len() > bounds::MAX_ACTIONS {
        return Err(bounds::failure(FailureKind::ModelBound));
    }
    let mut candidate = Candidate {
        origin: basis.origin.clone(),
        actions: vec![],
        readings: vec![],
        support: BTreeMap::new(),
        meter: Meter {
            limits,
            used: 0,
            scans: 0,
        },
    };
    for action in actions {
        candidate.meter.charge(1)?;
        candidate.actions.push(action.clone());
        candidate.observe_action(basis, action)?;
    }
    let mut places = BTreeSet::new();
    for action in &candidate.actions {
        if let Some(place) = action.mutation_place() {
            candidate.meter.charge(1)?;
            if !places.insert(place) {
                return Err(candidate.meter.failure(FailureKind::DuplicateMutation));
            }
        }
    }
    Ok(candidate)
}

impl Candidate {
    fn observe_action(&mut self, basis: &Snapshot, action: &Action) -> Result<(), Failure> {
        match action {
            Action::Add(place, _) | Action::Remove(place) => {
                self.observe_owner(
                    basis,
                    &place.collection,
                    matches!(action, Action::Remove(_)),
                )?;
                self.observe(basis, Support::Slot(place.clone()))?;
                self.observe(basis, Support::Range(place.collection.clone()))?;
                if matches!(action, Action::Remove(_)) {
                    self.observe(basis, Support::Children(place.clone()))?;
                }
            }
            Action::Replace(place, _) => {
                self.observe_owner(basis, &place.collection, true)?;
                self.observe(basis, Support::Slot(place.clone()))?;
            }
            Action::Get(place) => {
                self.observe_owner(basis, &place.collection, true)?;
                self.observe(basis, Support::Slot(place.clone()))?;
                let value = basis.entries.get(place);
                if value.is_some() {
                    self.meter.charge(1)?;
                }
                self.readings.push(Reading::Get(value.copied()));
            }
            Action::Iter(collection, predicate) => self.iterate(basis, collection, *predicate)?,
        }
        Ok(())
    }

    fn iterate(
        &mut self,
        basis: &Snapshot,
        collection: &Collection,
        predicate: Predicate,
    ) -> Result<(), Failure> {
        self.observe_owner(basis, collection, true)?;
        self.observe(basis, Support::Range(collection.clone()))?;
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
            self.meter.inspect()?;
            self.observe(basis, Support::Slot(place.clone()))?;
            if matches!(predicate, Predicate::All)
                || matches!(predicate, Predicate::Equal(value) if value == *payload)
            {
                self.meter.charge(1)?;
                entries.push((place.clone(), *payload));
            }
        }
        self.readings.push(Reading::Entries(entries));
        Ok(())
    }

    fn observe_owner(
        &mut self,
        basis: &Snapshot,
        collection: &Collection,
        require_present: bool,
    ) -> Result<(), Failure> {
        if let Some(owner) = collection.owner_place() {
            self.observe(basis, Support::Owner(owner.clone()))?;
            if require_present && !basis.entries.contains_key(&owner) {
                return Err(self.meter.failure(FailureKind::Unavailable));
            }
        }
        Ok(())
    }

    fn observe(&mut self, basis: &Snapshot, key: Support) -> Result<(), Failure> {
        self.meter.charge(1)?;
        let version = basis.version(&key);
        self.support.insert(key, version);
        Ok(())
    }
}
