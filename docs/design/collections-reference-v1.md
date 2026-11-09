# Collections reference contract v1

Status: **draft contract and implementation plan** for
[issue #290](https://github.com/flyingrobots/amblicit/issues/290). The executable
model currently checks capacity, duplicate exclusive mutations, and snapshot
slot/range observations; the complete conformance corpus is not implemented yet.
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
