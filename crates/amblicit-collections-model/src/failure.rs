//! This module owns structured refusal identity for the standalone model.

use super::{Collection, ContentId, Place, Support};

/// The semantic obstruction, separate from its logical subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// Final membership exceeds a collection's declared capacity.
    Capacity,
    /// More than one exclusive mutation names a canonical place.
    DuplicateMutation,
    /// A recorded support fact changed after the fixed basis was observed.
    Conflict,
    /// A candidate belongs to a different live model world.
    BasisMismatch,
    /// A required owning collection cannot be observed because its owner is absent.
    Unavailable,
    /// A fixed structural, state, or history limit would be exceeded.
    ModelBound,
    /// An owner selected for removal has children in the starting snapshot.
    NotEmpty,
    /// Initial membership or a proposed addition lacks an established owner.
    OwnerMissing,
    /// The complete proposal leaves a child without its owner.
    OwnerSurvival,
    /// An initial or proposed insertion names an occupied slot.
    AlreadyPresent,
    /// A removal or replacement names an absent slot.
    NotFound,
    /// The next work reservation does not fit the shared operation budget.
    WorkExhausted,
    /// The next physical inspection exceeds shared scan coverage.
    Incomplete,
    /// One immutable identity is associated with different typed values.
    ContentIntegrity,
    /// A closure edge names a target absent from the captured basis plus proposal.
    ContentMissing,
    /// An immutable target's kind differs from the edge's expected kind.
    ContentType,
    /// Canonical closure traversal re-enters an active node.
    ContentCycle,
}

/// Logical attribution within a whole-operation refusal.
///
/// This value confers no authority and is not a source location or catch site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureSubject {
    /// The whole attempt, for work, history, structure, and world-origin limits.
    Operation,
    /// A member or missing owner identified by its canonical place.
    Place(Place),
    /// A capacity or coverage obligation for one collection.
    Collection(Collection),
    /// The exact stale observation fact.
    Support(Support),
    /// A conflicting identity, failed edge target, or re-entered cycle node.
    Content(ContentId),
}

/// A refusal of construction, preparation, or atomic admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// Stable semantic category; callers must not parse diagnostic strings.
    pub kind: FailureKind,
    /// Logical subject, still attributed to the enclosing operation.
    pub subject: FailureSubject,
    pub(crate) work_used: u32,
}

impl Failure {
    pub(crate) fn at(mut self, subject: FailureSubject) -> Self {
        self.subject = subject;
        self
    }

    /// Work completed before refusal; unsuccessful reservations are not charged.
    #[must_use]
    pub fn work_used(&self) -> u32 {
        self.work_used
    }
}
