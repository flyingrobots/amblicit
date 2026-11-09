//! Symbolic immutable content for model conformance, not production digests.

use std::collections::BTreeSet;

use super::{
    Action, BTreeMap, Candidate, Failure, FailureKind, FailureSubject, Meter, Payload, Place,
    Snapshot,
};

pub(crate) const MAX_CONTENT_NODES: usize = 64;
pub(crate) const MAX_CONTENT_LINKS: usize = 4;

/// Frozen symbolic identity supplied by the fixture author.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContentId(
    /// Fixture coordinate; it is not computed from or authenticated against bytes.
    pub u8,
);

/// The semantic kind a content reference expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContentKind {
    /// A scalar immutable payload without outgoing content links.
    Leaf,
    /// An immutable ordered sequence of typed content links.
    Branch,
    /// An inert name for a mutable place, without authority over that place.
    Name,
}

/// A typed immutable edge; its target must exist in the complete closure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentRef {
    /// Symbolic target identity resolved from the captured basis plus proposal.
    pub id: ContentId,
    /// Required target kind, checked during complete closure validation.
    pub kind: ContentKind,
}

/// Immutable model data. Names remain weak data, not closure edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    /// Immutable scalar data with no outgoing links.
    Leaf(Payload),
    /// At most four typed links; admission requires their complete acyclic closure.
    Branch(Vec<ContentRef>),
    /// A weak mutable name; admission neither reads nor creates its target.
    Name(Place),
}

impl Content {
    pub(crate) fn references(&self) -> &[ContentRef] {
        match self {
            Self::Branch(references) => references,
            Self::Leaf(_) | Self::Name(_) => &[],
        }
    }

    pub(crate) fn kind(&self) -> ContentKind {
        match self {
            Self::Leaf(_) => ContentKind::Leaf,
            Self::Branch(_) => ContentKind::Branch,
            Self::Name(_) => ContentKind::Name,
        }
    }

    pub(crate) fn charge_value(&self, meter: &mut Meter) -> Result<(), Failure> {
        meter.charge(1)?;
        meter.charge(self.references().len())
    }
}

impl Candidate {
    pub(crate) fn normalize_content(&mut self, basis: &Snapshot) -> Result<(), Failure> {
        let mut conflicts = BTreeSet::new();
        for action in &self.actions {
            let Action::Ensure(id, value) = action else {
                continue;
            };
            value.charge_value(&mut self.meter)?;
            if let Some(previous) = self.ensures.get(id) {
                if previous != value {
                    conflicts.insert(*id);
                }
            } else {
                self.ensures.insert(*id, value.clone());
            }
        }
        if let Some(id) = conflicts.into_iter().next() {
            return Err(self
                .meter
                .failure(FailureKind::ContentIntegrity)
                .at(FailureSubject::Content(id)));
        }
        if !self.ensures.is_empty() {
            for (id, value) in &basis.contents {
                value.charge_value(&mut self.meter)?;
                self.content_basis.insert(*id, value.clone());
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_proposal(
    candidate: &Candidate,
    current: &BTreeMap<ContentId, Content>,
    meter: &mut Meter,
) -> Result<BTreeSet<ContentId>, Failure> {
    let mut new_ids = BTreeSet::new();
    if candidate.ensures.is_empty() {
        return Ok(new_ids);
    }
    for (id, value) in &candidate.ensures {
        meter.charge(1)?;
        if candidate
            .content_basis
            .get(id)
            .is_some_and(|old| old != value)
            || current.get(id).is_some_and(|old| old != value)
        {
            return Err(meter
                .failure(FailureKind::ContentIntegrity)
                .at(FailureSubject::Content(*id)));
        }
        if !current.contains_key(id) {
            new_ids.insert(*id);
        }
    }
    let basis_additions = candidate
        .ensures
        .keys()
        .filter(|id| !candidate.content_basis.contains_key(*id))
        .count();
    check_size(candidate.content_basis.len(), basis_additions, meter)?;
    check_size(current.len(), new_ids.len(), meter)?;
    super::content_closure::validate(&candidate.content_basis, &candidate.ensures, meter)?;
    Ok(new_ids)
}

fn check_size(existing: usize, additions: usize, meter: &Meter) -> Result<(), Failure> {
    existing
        .checked_add(additions)
        .filter(|size| *size <= MAX_CONTENT_NODES)
        .ok_or_else(|| meter.failure(FailureKind::ModelBound))?;
    Ok(())
}
