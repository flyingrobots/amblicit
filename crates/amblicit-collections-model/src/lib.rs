//! Draft standalone collections model. Production crates do not depend on it.
//!
//! Capacity, exclusive mutations, snapshot observations, and ownership are
//! implemented, with shared operation work/scan limits and a finite state/history
//! envelope. Typed immutable ensures preserve closure and atomic publication.
//! Refusals carry logical subjects with explicit phase precedence. Contract
//! agreement checks metadata only. Production wire codecs and provider adoption
//! are downstream work; this model does not establish their correctness.
//!
//! All APIs are synchronous and operate only in memory. They perform no I/O,
//! persistence, or cryptographic authentication. Places and content IDs are
//! symbolic fixture identities. Collections use ordered maps; requests and
//! snapshots allocate within the fixed model envelope. Work units describe the
//! reference cost schedule, not machine instructions or elapsed time.
//!
//! # Observe one snapshot, then admit the whole proposal
//!
//! Proposed writes do not change readings taken during preparation. Admission
//! may still refuse if the world changed or the complete proposal is invalid.
//!
//! ```
//! use amblicit_collections_model::{prepare, Action, Limits, Payload, Place, Reading, World};
//! # fn main() -> Result<(), amblicit_collections_model::Failure> {
//! let mut world = World::new(1, [])?;
//! let place = Place::root(0, 7);
//! let requests = [Action::Add(place.clone(), Payload(42)), Action::Get(place.clone())];
//! let candidate = prepare(&world.snapshot(), &requests, Limits::default())?;
//! assert_eq!(candidate.readings(), &[Reading::Get(None)]);
//! world.admit(candidate)?;
//! assert_eq!(world.snapshot().entries().get(&place), Some(&Payload(42)));
//! # Ok(())
//! # }
//! ```
//!
//! # Ensure a complete typed content closure
//!
//! A parent can precede its child because admission checks the complete proposal.
//! These IDs are explicitly chosen fixture values, not hashes of content.
//!
//! ```
//! use amblicit_collections_model::{
//!     prepare, Action, Content, ContentId, ContentKind, ContentRef, Limits, Payload, World,
//! };
//! # fn main() -> Result<(), amblicit_collections_model::Failure> {
//! let mut world = World::new(0, [])?;
//! let parent = Content::Branch(vec![ContentRef { id: ContentId(2), kind: ContentKind::Leaf }]);
//! let child = Content::Leaf(Payload(7));
//! let requests = [
//!     Action::Ensure(ContentId(1), parent.clone()),
//!     Action::Ensure(ContentId(2), child.clone()),
//! ];
//! let candidate = prepare(&world.snapshot(), &requests, Limits::default())?;
//! assert_eq!(world.admit(candidate)?.content_added, 2);
//! assert_eq!(world.snapshot().contents(), &[(ContentId(1), parent), (ContentId(2), child)].into());
//! # Ok(())
//! # }
//! ```

#![deny(missing_docs)]

use std::collections::BTreeMap;

mod admission;
mod bounds;
mod content;
mod content_closure;
pub mod contract;
mod failure;
mod origin;
mod preparation;

pub use content::{Content, ContentId, ContentKind, ContentRef};
pub use failure::{Failure, FailureKind, FailureSubject};
pub use preparation::prepare;

/// A tuple path: each component is an owning-collection field and storage key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Place {
    collection: Collection,
    key: u8,
}

/// A named collection at the root or under a stable owning place.
///
/// Its identity contains the owner path and field, independent of payloads and
/// current membership. Constructing a name does not establish that its owner exists.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Collection {
    owner: Vec<(u8, u8)>,
    field: u8,
}

impl Collection {
    /// Name a root collection by field, with an empty owner path.
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

/// A logical observation whose version must still match at admission.
///
/// Canonical ordering follows the variants below, then their place/collection
/// identity. These model facts are neither persisted proofs nor wire capabilities.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Support {
    /// Presence, absence, or payload at one place; same-value writes invalidate it.
    Slot(Place),
    /// Membership of a collection, including an empty traversal's negative evidence.
    Range(Collection),
    /// Lifetime of an owning place; payload replacement preserves this fact.
    Owner(Place),
    /// Membership of collections directly owned by a place, used for emptiness.
    Children(Place),
}

/// Pure payload selection; excluded entries still consume inspection work.
#[derive(Clone, Copy, Debug)]
pub enum Predicate {
    /// Select every inspected entry.
    All,
    /// Select only entries with this scalar payload.
    Equal(Payload),
}

/// A successful snapshot reading, independent of buffered mutations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    /// Optional payload at a place; `None` is established absence, not unavailable access.
    Get(Option<Payload>),
    /// Selected entries in canonical place order, using authoritative storage keys.
    Entries(Vec<(Place, Payload)>),
}

impl Place {
    /// Name a slot in a root collection without observing or creating membership.
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
pub struct Payload(
    /// Scalar fixture value, with no interpretation as a storage key or authority.
    pub i64,
);

/// An immutable, owned copy of one world's membership, content, and evidence.
///
/// Cloning preserves the private world origin and copies bounded state. It does
/// not refresh observations after later admissions or create a persistent snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    origin: origin::Origin,
    capacity: u16,
    entries: BTreeMap<Place, Payload>,
    contents: BTreeMap<ContentId, Content>,
    epoch: u64,
    versions: BTreeMap<Support, u64>,
}

impl Snapshot {
    /// Immutable content established in this snapshot.
    #[must_use]
    pub fn contents(&self) -> &BTreeMap<ContentId, Content> {
        &self.contents
    }

    /// Borrow the snapshot's mutable members in canonical place order.
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
                return Err(meter.failure(kind).at(FailureSubject::Place(place.clone())));
            }
        }
        Ok(())
    }
}

/// A request evaluated against one fixed snapshot to form a complete candidate.
///
/// Request construction itself neither observes nor mutates a world. Exclusive
/// mutations of the same place refuse even when their payloads are identical.
#[derive(Clone, Debug)]
pub enum Action {
    /// Propose exact immutable content presence.
    Ensure(ContentId, Content),
    /// Evaluate nested requests within the same operation.
    Group(Vec<Action>),
    /// Propose insertion into an absent slot of an already established collection.
    Add(Place, Payload),
    /// Propose removal of a present member whose owned collections start empty.
    Remove(Place),
    /// Propose a payload write to a present member, preserving its owned collections.
    Replace(Place, Payload),
    /// Read a slot's presence or payload from the starting snapshot.
    Get(Place),
    /// Inspect a whole bounded collection and retain selection evidence for all entries.
    Iter(Collection, Predicate),
}

impl Action {
    fn mutation_place(&self) -> Option<&Place> {
        match self {
            Self::Add(place, _) | Self::Remove(place) | Self::Replace(place, _) => Some(place),
            Self::Get(_) | Self::Iter(_, _) | Self::Group(_) | Self::Ensure(_, _) => None,
        }
    }
}

/// Shared operation limits, carried from preparation through admission.
///
/// Defaults are 10,000 work units and 256 inspections. Structural limits are
/// separate; increasing these values cannot enlarge the fixed model envelope.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum reference work units; a failed reservation consumes no units.
    pub work: u32,
    /// Maximum entries inspected across all traversals, including excluded values.
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
            subject: FailureSubject::Operation,
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

/// In-memory result of one successful admission, including read-only corroboration.
///
/// This is not an authenticated, durable, or production execution receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    /// Total reference work consumed by preparation and admission together.
    pub work_used: u32,
    /// Newly published immutable nodes; identical existing ensures add none.
    pub content_added: usize,
}

/// An inert, bounded proposal with readings and support from one fixed basis.
///
/// Only [`prepare`] constructs it. Cloning copies the proposal and retains its
/// original evidence; it does not authorize replay or refresh stale observations.
#[derive(Clone, Debug)]
#[must_use]
pub struct Candidate {
    origin: origin::Origin,
    actions: Vec<Action>,
    readings: Vec<Reading>,
    support: BTreeMap<Support, u64>,
    meter: Meter,
    ensures: BTreeMap<ContentId, Content>,
    content_basis: BTreeMap<ContentId, Content>,
}

impl Candidate {
    /// Coalesced immutable obligations; repeated work remains charged.
    #[must_use]
    pub fn ensures(&self) -> &BTreeMap<ContentId, Content> {
        &self.ensures
    }

    /// Work consumed during preparation, before any admission work is attempted.
    #[must_use]
    pub fn work_used(&self) -> u32 {
        self.meter.used
    }

    /// Borrow readings in authored evaluation order, with nested groups flattened.
    #[must_use]
    pub fn readings(&self) -> &[Reading] {
        &self.readings
    }

    /// Borrow canonical support facts and their captured model version numbers.
    ///
    /// A number has meaning only within this candidate's private world origin.
    #[must_use]
    pub fn support(&self) -> &BTreeMap<Support, u64> {
        &self.support
    }
}

/// One bounded mutable reference world with append-only validated content.
///
/// Each instance has a distinct live origin. All state changes occur through
/// [`World::admit`]; there is no I/O, durable storage, or concurrent host adapter.
#[derive(Debug)]
pub struct World {
    state: Snapshot,
}

impl World {
    /// Copy the complete bounded state while retaining this world's origin.
    ///
    /// Allocates for maps and owner paths; later admissions do not change the copy.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.state.clone()
    }
}
