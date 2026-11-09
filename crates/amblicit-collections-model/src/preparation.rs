//! This module owns fixed-basis request evaluation and candidate normalization.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    bounds, Action, Candidate, Collection, Failure, FailureKind, FailureSubject, Limits, Meter,
    Place, Predicate, Reading, Snapshot, Support,
};

#[derive(Clone, Copy)]
enum OwnerRequirement {
    Present,
    DeferredToAdmission,
}

/// Construct an inert proposal from requests on one fixed snapshot.
///
/// # Errors
/// Returns a typed observation failure, exhausted work, incomplete scan coverage,
/// or duplicate exclusive mutation. A failed observation yields no candidate.
pub fn prepare(basis: &Snapshot, actions: &[Action], limits: Limits) -> Result<Candidate, Failure> {
    bounds::requests(actions)?;
    let mut candidate = Candidate {
        ensures: BTreeMap::new(),
        content_basis: BTreeMap::new(),
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
    candidate.evaluate(basis, actions)?;
    candidate.normalize_mutations()?;
    candidate.normalize_content(basis)?;
    Ok(candidate)
}

impl Candidate {
    fn normalize_mutations(&mut self) -> Result<(), Failure> {
        let mut places = BTreeSet::new();
        let mut duplicates = BTreeSet::new();
        for action in &self.actions {
            if let Some(place) = action.mutation_place() {
                self.meter.charge(1)?;
                if !places.insert(place) {
                    duplicates.insert(place);
                }
            }
        }
        if let Some(place) = duplicates.into_iter().next() {
            return Err(self
                .meter
                .failure(FailureKind::DuplicateMutation)
                .at(FailureSubject::Place(place.clone())));
        }
        Ok(())
    }

    fn evaluate(&mut self, basis: &Snapshot, actions: &[Action]) -> Result<(), Failure> {
        for action in actions {
            self.meter.charge(1)?;
            if let Action::Ensure(_, value) = action {
                value.charge_value(&mut self.meter)?;
            }
            if !matches!(action, Action::Group(_)) {
                self.actions.push(action.clone());
            }
            self.observe_action(basis, action)?;
        }
        Ok(())
    }

    fn observe_action(&mut self, basis: &Snapshot, action: &Action) -> Result<(), Failure> {
        match action {
            Action::Ensure(_, _) => {}
            Action::Group(children) => self.evaluate(basis, children)?,
            Action::Add(place, _) | Action::Remove(place) => {
                self.observe_owner(
                    basis,
                    &place.collection,
                    if matches!(action, Action::Remove(_)) {
                        OwnerRequirement::Present
                    } else {
                        OwnerRequirement::DeferredToAdmission
                    },
                )?;
                self.observe(basis, Support::Slot(place.clone()))?;
                self.observe(basis, Support::Range(place.collection.clone()))?;
                if matches!(action, Action::Remove(_)) {
                    self.observe(basis, Support::Children(place.clone()))?;
                }
            }
            Action::Replace(place, _) => {
                self.observe_owner(basis, &place.collection, OwnerRequirement::Present)?;
                self.observe(basis, Support::Slot(place.clone()))?;
            }
            Action::Get(place) => {
                self.observe_owner(basis, &place.collection, OwnerRequirement::Present)?;
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
        self.observe_owner(basis, collection, OwnerRequirement::Present)?;
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
            self.meter.inspect().map_err(|failure| {
                if failure.kind == FailureKind::Incomplete {
                    failure.at(FailureSubject::Collection(collection.clone()))
                } else {
                    failure
                }
            })?;
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
        requirement: OwnerRequirement,
    ) -> Result<(), Failure> {
        if let Some(owner) = collection.owner_place() {
            self.observe(basis, Support::Owner(owner.clone()))?;
            if matches!(requirement, OwnerRequirement::Present)
                && !basis.entries.contains_key(&owner)
            {
                return Err(self
                    .meter
                    .failure(FailureKind::Unavailable)
                    .at(FailureSubject::Place(owner)));
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
