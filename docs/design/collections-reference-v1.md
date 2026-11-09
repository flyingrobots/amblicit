# Collections reference contract v1

Status: **draft contract and implementation plan** for
[issue #290](https://github.com/flyingrobots/amblicit/issues/290). The executable
model currently checks capacity, duplicate exclusive mutations, snapshot
slot/range observations, ownership, and operation work/scan limits. The complete
conformance corpus is not implemented yet.
This document does not
add a compiler capability or claim that an Echo provider accepts collections.

## Purpose and boundary

This contract gives subsequent compiler and runtime work one independently
testable meaning for places, ownership, snapshot observations, and atomic
candidates. The reference model will be a deterministic Rust workspace crate,
with no dependency from the production compiler or provider. It is an oracle
for this bounded contract, not a replacement execution engine.

The design input is *Edict collections v2: places, payloads and witnessed
optics* (2026-10-22), SHA-256
`fbc898f3237264f25fad1132aee7b05b9c23403cf72a7b64d5cdcb5f3b5aac28`.
This page and its conformance plan retain the required semantics without
requiring access to the original document.

## Representation decisions

1. **Lawpack meanings, neutral Core machinery.** Membership, ownership,
   content interpretation, permitted changes, and domain obstructions belong
   to a versioned lawpack contract. The generic Core concepts are bounded
   observation, pure values, candidate construction, basis binding, and
   costs. This issue specifies their correspondence; it does not introduce
   Core nodes, source syntax, a new provider ABI, or general optic composition.
2. **Three distinct artifacts.** A description names a bounded focus and its
   permitted operations, but carries no authority. A reading binds observed
   values and support to a basis. A candidate binds proposed changes and
   complete support to that basis. Passing a reading as data does not make
   its observations current.
3. **Operation-level candidate failures.** Capacity, owner survival, duplicate
   mutations, and content closure are checked on the complete candidate.
   Their failures belong to the operation outcome. An individual `add` site
   cannot catch a candidate-wide capacity failure. A stable logical subject
   identifies the offending place, collection, or content obligation; source
   adapters may attach a location without changing that attribution.
4. **Versioned semantic capability.** `amblicit.collections/v1` names this
   bounded semantic contract. It is not advertised by the current compiler
   or target profile. Future consumers must explicitly negotiate a supported
   artifact version; unknown versions must refuse before authority or writes
   are granted. Future wire encodings must bind the version, basis, complete
   support, declared limits, ordered requests, and normalized candidate.
   The reference model's values are not executable provider packages.

## Identity and state

A canonical place is the tuple of owner identity, owning-collection field,
and canonical key. Namespaces and type identity are part of the tuple's
interpretation. String concatenation is not a canonical encoding. Two access
descriptions that resolve to the same tuple designate one mutation target.

A place identifies a location. Removing and later re-adding a member reuses
that place; it does not prove occupant continuity. Incarnation-sensitive
references are outside this version. Stored names are weak data: resolution
against mutable state is an explicit read that may establish absence.

Each model world owns a private live-origin handle. Snapshots and candidates
retain that handle; admission first refuses `BasisMismatch` when it belongs to
another world, even if the candidate is empty or both worlds have identical
membership and version numbers. Snapshot cloning preserves the handle. Worlds
cannot be cloned into independently mutable copies. Comparing handles uses
object identity without exposing an address, serializing it, or ordering by it.
This is the model's in-process authority boundary, not a production world ID,
content digest, or wire basis encoding. Bounds and origin checks are constant
preflight checks outside the reference work-unit schedule.

An enduring object has a stable owner location. Moving it changes its location
payload and membership records in one candidate; it does not relocate the
object's owning collections. A member's payload excludes its owning
collections. Payload copy or replacement cannot copy, erase, or transfer
structural ownership.

Immutable semantic content identity, target storage identity, and root/path
occurrence identity remain separate. The model may use frozen symbolic
content identities to exercise closure and immutability; such evidence does
not establish a digest algorithm or Echo's `ContentAddressV1` law, owned by
[issue #260](https://github.com/flyingrobots/amblicit/issues/260).

## Observation and proposed changes

Evaluation fixes a basis. Every read and traversal uses that snapshot,
including reads after buffered writes. `add` followed by `get` still sees
absence; `remove` followed by `get` still sees the original payload.

| Operation | Snapshot meaning | Required support |
| --- | --- | --- |
| `get` | Observe a member's payload or established absence | Owner existence and slot observation |
| `add` | Propose filling an absent slot | Live owner, slot absence, collection cardinality |
| `replace` | Propose a payload-only update | Live owner, member observation, every input read |
| `remove` | Propose removing a member | Presence, emptiness of owned collections, owner guards |
| `iter` | Observe entries in canonical key order | Range, including empty ranges, and every inspected payload |
| `ensure` | Propose immutable content presence | Exact typed content, complete proposed content closure |

Entry keys are authoritative storage keys, independent of payload fields.
A filtered traversal preserves support for inspected entries excluded from
the result. Scan bounds cover physical inspections, including nonmatching
entries; a result limit alone is insufficient. Nested evaluation consumes
one shared operation budget. A failed or partial observation never becomes
an absence witness.

The observation result distinguishes established absence, unavailable basis
or authority, incomplete observation due to bounds, and conflict. Expected
domain obstructions are distinct from integrity failures.

## Normalize, validate, admit

Normalization permits at most one exclusive mutation per canonical place.
Two identical replacements are still two exclusive mutations and refuse.
Equal immutable ensures coalesce, while already spent work remains charged.
Conflicting typed content under one content identity is an integrity failure.
Derived cardinality updates aggregate once per collection.

Candidate validation checks the complete normalized proposal:

- Presence and absence agree with the observed basis.
- A removed owner has empty owning collections in the basis. Removing its
  children earlier in the same candidate does not satisfy `NotEmpty`.
- Every member left in the candidate has a live owner in that candidate.
- Adding a member requires its owner to exist in the basis. Creating a new
  owner and populating it in the same candidate is unsupported in v1.
- Capacity uses `basis_count - removals + additions`. A full collection may
  exchange a member in one candidate. Intermediate grouping does not change
  this result.
- The complete proposed immutable graph is typed and acyclic. A new parent
  may refer to a new child in that candidate. Mutable names remain inert
  data and cannot be traversed as immutable links.
- All observation, normalization, validation, and copying work stays within
  the declared operation limits.

Admission corroborates the recorded support against current state. The
initial contract serializes membership changes per collection. A different
key is not proof of independence: capacity and range support may conflict.
Unrelated state changes need not invalidate a candidate, but a stale slot,
range, owner, or invariant observation cannot be admitted as current.

Only a fully admitted candidate changes application state. Failure leaves the
application state untouched while retaining the attempt's outcome and work
evidence. Same-value replacement still produces write evidence. The model's
receipt demonstrates its own transition only; it is not an Echo Tick, WAL,
durability, recovery, or production-authority witness.

## Deterministic outcomes

### Structural envelope

The reference model uses a fixed small-state envelope: at most 64 live members
across all collections, 64 requests in one operation, four components in an
owner path, and 128 successful mutating admissions in one world. These numbers
bound the conformance model, not the compiler language or an Echo resource limit.
Initial membership and snapshots are constructed outside operation work
accounting; their sizes are bounded independently.

`World::new` returns a structured failure instead of publishing invalid initial
state. It consumes no more than 65 input members, rejects duplicate places,
requires every immediate owner to exist, and checks collection capacity after
collecting membership. A child may appear before its owner in the input.
`Place::child` and `Place::children` refuse depth five before copying the path.
Request count is checked before evaluation. Admission validates the final live
member count, so remove-plus-add remains possible at the limit.

Every successful mutating admission advances history, including same-value
replacement. Refusals and read-only corroboration do not consume that history.
After 128 mutations the world still admits read-only candidates, but refuses
another mutation with `ModelBound` before changing state. Retaining at most four
initial support tokens per member and four changed tokens per request gives a
conservative bound of 33,024 retained version entries: `64 * 4 + 128 * 64 * 4`.
The model retains tombstones within that envelope; it does not evict evidence
and accidentally make an old reading current. This is a count bound, not a
claim about allocator bytes or production retention policy.

### Reference work units

The fixtures specified these costs before the meter was implemented. These are
bounded model steps, not an Echo gas schedule or a wall-clock prediction.
One unit covers a bounded request copy, a support observation (including a
repeat), a copied payload, an inspected traversal entry, an exclusive-request
normalization, a corroborated support token, a mutation's domain check, a
copied baseline member, a proposed mutation, a resulting member's invariant
check, or one distinct updated version token. Removal additionally pays for
every member inspected to establish owned-collection emptiness. Read-only
admission corroborates support without copying application state.

For an add to an empty root collection, preparation costs four units: request,
slot observation, range observation, and normalization. Admission costs eight:
two corroborations, domain check, proposed mutation, final-member check, and
three version updates (slot, range, owner lifetime). The whole operation costs
12; budget 11 must refuse before changing application state.

Scanning three nonmatching root entries costs eight preparation units (request,
range, three inspections, three slot observations) and four corroborations.
A result filter does not reduce that work. Two traversals of a one-member
owned collection spend 12 preparation units and corroborate three distinct
tokens, totaling 15. Their physical scan count is two against one shared limit.
A scan-limit refusal is incomplete observation, never established absence.
Successful model receipts and failures retain the work actually spent; a
refused step does not execute or consume additional work. The structural
envelope bounds initial state, request count, owner depth, and retained history.
Nested evaluation and immutable content remain outside the current scaffold.

### Failure ordering

The model will name each failure category and its boundary. Tests must settle
the precedence explicitly before implementation: unavailable observations;
work exhaustion at the performed step; invalid/ambiguous candidate shape;
stale observation support; domain obstructions and owner survival; capacity;
content integrity. Within a phase, a canonical subject order selects the
failure, independent of hash-map iteration or equivalent patch grouping.

This ordering is a proposed decision, not implemented behavior. Its fixture
oracle must cover multiple simultaneous failures, including failures across
phases. Integrity failures must not be relabeled as ordinary absence or
capacity errors.

## Evidence required to finish this issue

The [conformance plan](../topics/collections-model/test-plan.md) retains all
16 original rows plus snapshot-ownership and observation distinctions.
Independent fixtures will supply initial state, operations, expected outcome,
expected support, and expected resulting state before the implementation.

The corpus must detect three deliberately broken interpretations: omitted
capacity checks, omitted empty-range support, and omitted owner-survival
checks. Bounded generated traces supplement these fixtures and retain a
minimal counterexample. Neither generated agreement nor model conformance
establishes compiler/provider/runtime conformance or concurrency precision.

The implementation must pass the repository's required checks and independent
review before this contract is considered the completed #290 deliverable.
