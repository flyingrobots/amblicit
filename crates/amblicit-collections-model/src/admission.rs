//! Prepare both namespaces before publishing any application-state change.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    bounds, content, Action, Candidate, Failure, FailureKind, FailureSubject, Meter, Payload,
    Place, Receipt, Snapshot, Support, World,
};

struct Members {
    entries: BTreeMap<Place, Payload>,
    touched: BTreeSet<Support>,
}

impl World {
    /// Corroborate the candidate and atomically publish its complete proposal.
    ///
    /// # Errors
    /// Returns `BasisMismatch` for another world, `Conflict` for stale support,
    /// a typed domain/content obstruction, or a work/model limit failure.
    /// Every refusal preserves both mutable membership and immutable content.
    pub fn admit(&mut self, candidate: Candidate) -> Result<Receipt, Failure> {
        let mut meter = candidate.meter;
        if candidate.origin != self.state.origin {
            return Err(meter.failure(FailureKind::BasisMismatch));
        }
        for (key, version) in &candidate.support {
            meter.charge(1)?;
            if self.state.version(key) != *version {
                return Err(meter
                    .failure(FailureKind::Conflict)
                    .at(FailureSubject::Support(key.clone())));
            }
        }
        let members = prepare_members(&self.state, &candidate, &mut meter)?;
        let new_ids = content::validate_proposal(&candidate, &self.state.contents, &mut meter)?;
        let content_added = new_ids.len();
        if members.is_none() && new_ids.is_empty() {
            return Ok(Receipt {
                work_used: meter.used,
                content_added,
            });
        }
        if self.state.epoch > bounds::MAX_MUTATING_COMMITS {
            return Err(meter.failure(FailureKind::ModelBound));
        }
        let epoch = self
            .state
            .epoch
            .checked_add(1)
            .ok_or_else(|| meter.failure(FailureKind::ModelBound))?;
        let publication = members
            .as_ref()
            .map_or(0, |value| value.touched.len())
            .checked_add(content_added)
            .ok_or_else(|| meter.failure(FailureKind::ModelBound))?;
        meter.charge(publication)?;
        // No fallible model step follows the first application-state write.
        if let Some(members) = members {
            for key in members.touched {
                self.state.versions.insert(key, epoch);
            }
            self.state.entries = members.entries;
        }
        for (id, value) in candidate.ensures {
            if new_ids.contains(&id) {
                self.state.contents.insert(id, value);
            }
        }
        self.state.epoch = epoch;
        Ok(Receipt {
            work_used: meter.used,
            content_added,
        })
    }
}

fn prepare_members(
    basis: &Snapshot,
    candidate: &Candidate,
    meter: &mut Meter,
) -> Result<Option<Members>, Failure> {
    if candidate
        .actions
        .iter()
        .all(|action| action.mutation_place().is_none())
    {
        return Ok(None);
    }
    basis.validate_domains(candidate, meter)?;
    meter.charge(basis.entries.len())?;
    let mut members = Members {
        entries: basis.entries.clone(),
        touched: BTreeSet::new(),
    };
    for action in &candidate.actions {
        if action.mutation_place().is_some() {
            meter.charge(1)?;
            members.apply(action);
        }
    }
    validate_members(&members.entries, basis.capacity, meter)?;
    Ok(Some(members))
}

impl Members {
    fn membership_support(&mut self, place: &Place) {
        self.touched.insert(Support::Slot(place.clone()));
        self.touched
            .insert(Support::Range(place.collection.clone()));
        self.touched.insert(Support::Owner(place.clone()));
        if let Some(owner) = place.collection.owner_place() {
            self.touched.insert(Support::Children(owner));
        }
    }

    fn apply(&mut self, action: &Action) {
        match action {
            Action::Add(place, payload) => {
                self.membership_support(place);
                self.entries.insert(place.clone(), *payload);
            }
            Action::Remove(place) => {
                self.membership_support(place);
                self.entries.remove(place);
            }
            Action::Replace(place, payload) => {
                self.touched.insert(Support::Slot(place.clone()));
                self.entries.insert(place.clone(), *payload);
            }
            Action::Get(_) | Action::Iter(_, _) | Action::Group(_) | Action::Ensure(_, _) => {}
        }
    }
}

fn validate_members(
    entries: &BTreeMap<Place, Payload>,
    capacity: u16,
    meter: &mut Meter,
) -> Result<(), Failure> {
    if entries.len() > bounds::MAX_MEMBERS {
        return Err(meter.failure(FailureKind::ModelBound));
    }
    let mut counts = BTreeMap::new();
    for place in entries.keys() {
        meter.charge(1)?;
        if place
            .collection
            .owner_place()
            .is_some_and(|owner| !entries.contains_key(&owner))
        {
            return Err(meter
                .failure(FailureKind::OwnerSurvival)
                .at(FailureSubject::Place(place.clone())));
        }
        let count = counts.entry(&place.collection).or_insert(0_u16);
        *count = count
            .checked_add(1)
            .ok_or_else(|| meter.failure(FailureKind::ModelBound))?;
    }
    if let Some((collection, _)) = counts.into_iter().find(|(_, count)| *count > capacity) {
        return Err(meter
            .failure(FailureKind::Capacity)
            .at(FailureSubject::Collection(collection.clone())));
    }
    Ok(())
}
