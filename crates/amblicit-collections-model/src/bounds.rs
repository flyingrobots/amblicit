//! Finite model envelope and validated initial membership.

use super::{Action, BTreeMap, Failure, FailureKind, Payload, Place, Snapshot, Support, World};

pub(crate) const MAX_MEMBERS: usize = 64;
pub(crate) const MAX_ACTIONS: usize = 64;
pub(crate) const MAX_OWNER_DEPTH: usize = 4;
pub(crate) const MAX_MUTATING_COMMITS: u64 = 128;
const MAX_EVALUATION_DEPTH: usize = 4;

pub(crate) fn failure(kind: FailureKind) -> Failure {
    Failure { kind, work_used: 0 }
}

pub(crate) fn requests(actions: &[Action]) -> Result<(), Failure> {
    let mut remaining = MAX_ACTIONS;
    request_nodes(actions, &mut remaining, 0)
}

fn request_nodes(actions: &[Action], remaining: &mut usize, depth: usize) -> Result<(), Failure> {
    if depth > MAX_EVALUATION_DEPTH || actions.len() > *remaining {
        return Err(failure(FailureKind::ModelBound));
    }
    for action in actions {
        *remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| failure(FailureKind::ModelBound))?;
        if matches!(action, Action::Ensure(_, value) if value.references().len() > super::content::MAX_CONTENT_LINKS)
        {
            return Err(failure(FailureKind::ModelBound));
        }
        if let Action::Group(children) = action {
            let next_depth = depth
                .checked_add(1)
                .ok_or_else(|| failure(FailureKind::ModelBound))?;
            request_nodes(children, remaining, next_depth)?;
        }
    }
    Ok(())
}

fn initial_entries(
    capacity: u16,
    entries: impl IntoIterator<Item = (Place, Payload)>,
) -> Result<BTreeMap<Place, Payload>, Failure> {
    let mut members = BTreeMap::new();
    for (place, payload) in entries {
        if members.len() >= MAX_MEMBERS {
            return Err(failure(FailureKind::ModelBound));
        }
        if members.insert(place, payload).is_some() {
            return Err(failure(FailureKind::AlreadyPresent));
        }
    }
    let mut counts = BTreeMap::new();
    for place in members.keys() {
        if place
            .collection
            .owner_place()
            .is_some_and(|owner| !members.contains_key(&owner))
        {
            return Err(failure(FailureKind::OwnerMissing));
        }
        // At most MAX_MEMBERS entries can reach this loop, so the count fits u16.
        let count = counts.entry(&place.collection).or_insert(0_u16);
        *count = count
            .checked_add(1)
            .ok_or_else(|| failure(FailureKind::ModelBound))?;
        if *count > capacity {
            return Err(failure(FailureKind::Capacity));
        }
    }
    Ok(members)
}

impl World {
    /// Construct a reference world from explicit initial membership.
    ///
    /// Consumes at most 65 input members and retains at most 64. Initial membership
    /// is independent of input order. An empty collection may have zero capacity.
    ///
    /// # Errors
    /// Returns `ModelBound` above 64 members, `AlreadyPresent` for a duplicate
    /// place, `OwnerMissing` for an orphan, or `Capacity` for an oversized
    /// collection. Construction is outside operation work accounting.
    pub fn new(
        capacity: u16,
        entries: impl IntoIterator<Item = (Place, Payload)>,
    ) -> Result<Self, Failure> {
        let entries = initial_entries(capacity, entries)?;
        let mut versions = BTreeMap::new();
        for place in entries.keys() {
            versions.insert(Support::Slot(place.clone()), 1);
            versions.insert(Support::Range(place.collection.clone()), 1);
            versions.insert(Support::Owner(place.clone()), 1);
            if let Some(owner) = place.collection.owner_place() {
                versions.insert(Support::Children(owner), 1);
            }
        }
        Ok(Self {
            state: Snapshot {
                origin: super::origin::Origin::new(),
                capacity,
                entries,
                contents: BTreeMap::new(),
                epoch: 1,
                versions,
            },
        })
    }
}
