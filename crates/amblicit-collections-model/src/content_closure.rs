//! Typed acyclic closure over one captured basis and its complete ensure proposal.

use std::collections::{BTreeMap, BTreeSet};

use super::{Content, ContentId, Failure, FailureKind, Meter};

struct Closure<'a> {
    basis: &'a BTreeMap<ContentId, Content>,
    proposed: &'a BTreeMap<ContentId, Content>,
    active: BTreeSet<ContentId>,
    complete: BTreeSet<ContentId>,
    meter: &'a mut Meter,
}

pub(crate) fn validate(
    basis: &BTreeMap<ContentId, Content>,
    proposed: &BTreeMap<ContentId, Content>,
    meter: &mut Meter,
) -> Result<(), Failure> {
    let mut closure = Closure {
        basis,
        proposed,
        active: BTreeSet::new(),
        complete: BTreeSet::new(),
        meter,
    };
    // Both maps are bounded; each effective node is charged and traversed once.
    for id in basis.keys().chain(proposed.keys()) {
        closure.visit(*id)?;
    }
    Ok(())
}

impl Closure<'_> {
    fn content(&self, id: ContentId) -> Result<&Content, Failure> {
        self.proposed
            .get(&id)
            .or_else(|| self.basis.get(&id))
            .ok_or_else(|| self.meter.failure(FailureKind::ContentMissing))
    }

    fn visit(&mut self, id: ContentId) -> Result<(), Failure> {
        if self.complete.contains(&id) {
            return Ok(());
        }
        if self.active.contains(&id) {
            return Err(self.meter.failure(FailureKind::ContentCycle));
        }
        self.meter.charge(1)?;
        let edge_count = self.content(id)?.references().len();
        self.meter.charge(edge_count)?;
        let references = self.content(id)?.references().to_vec();
        self.active.insert(id);
        // At most 64 nodes and four outgoing edges each, checked before entry.
        for reference in references {
            if self.content(reference.id)?.kind() != reference.kind {
                return Err(self.meter.failure(FailureKind::ContentType));
            }
            self.visit(reference.id)?;
        }
        self.active.remove(&id);
        self.complete.insert(id);
        Ok(())
    }
}
