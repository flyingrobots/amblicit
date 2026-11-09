//! Typed acyclic closure over one captured basis and its complete ensure proposal.

use std::collections::{BTreeMap, BTreeSet};

use super::{Content, ContentId, Failure, FailureKind, FailureSubject, Meter};

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
    let roots: BTreeSet<_> = basis.keys().chain(proposed.keys()).copied().collect();
    for id in roots {
        closure.visit(id)?;
    }
    Ok(())
}

impl Closure<'_> {
    fn content(&self, id: ContentId) -> Result<&Content, Failure> {
        self.proposed
            .get(&id)
            .or_else(|| self.basis.get(&id))
            .ok_or_else(|| {
                self.meter
                    .failure(FailureKind::ContentMissing)
                    .at(FailureSubject::Content(id))
            })
    }

    fn visit(&mut self, id: ContentId) -> Result<(), Failure> {
        if self.complete.contains(&id) {
            return Ok(());
        }
        if self.active.contains(&id) {
            return Err(self
                .meter
                .failure(FailureKind::ContentCycle)
                .at(FailureSubject::Content(id)));
        }
        self.meter.charge(1)?;
        let edge_count = self.content(id)?.references().len();
        self.meter.charge(edge_count)?;
        let mut references = self.content(id)?.references().to_vec();
        references.sort_unstable_by_key(|reference| (reference.id, reference.kind));
        self.active.insert(id);
        // At most 64 nodes and four outgoing edges each, checked before entry.
        for reference in references {
            if self.content(reference.id)?.kind() != reference.kind {
                return Err(self
                    .meter
                    .failure(FailureKind::ContentType)
                    .at(FailureSubject::Content(reference.id)));
            }
            self.visit(reference.id)?;
        }
        self.active.remove(&id);
        self.complete.insert(id);
        Ok(())
    }
}
