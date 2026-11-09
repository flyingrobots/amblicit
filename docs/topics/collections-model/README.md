# Collections reference model

This shelf owns the collections reference contract and its conformance plan.
The contract and executable model are under development in
[issue #290](https://github.com/flyingrobots/amblicit/issues/290). The standalone
`amblicit-collections-model` crate currently checks whole-candidate capacity,
rejects duplicate exclusive mutations, and corroborates slot/range observations
against current state. Reads use the starting snapshot, including empty and
filtered traversals. Owner lifetime and children-range evidence guard parent
removal and child insertion. Payload replacement preserves structural ownership;
whole-candidate validation rejects missing owners and orphaned children.
Observation, normalization, admission, and copying share an operation work
budget. Traversals share a scan limit; incomplete coverage cannot establish
absence. It has no production compiler consumers.
Snapshots and candidates retain a private world origin. Admission refuses
cross-world evidence even when numeric versions match; snapshot copies keep
their origin. This is an in-process model boundary, not a persisted identity.
Initial membership, request count, owner depth, and mutating history have fixed
model limits. Constructors refuse invalid membership; exhausted history permits
read-only corroboration. Nested request groups share the snapshot, work/scan
meters, and complete-candidate validation. Their depth and total node count are
bounded before copying. Immutable ensures coalesce exact content while retaining
work spent, validate typed acyclic closure against the captured basis plus the
complete proposal, and publish jointly with mutable changes. Weak mutable names
remain inert data. Refusals identify their logical subject and use declared
phase/identity precedence. Regrouping a complete candidate does not change the
selected duplicate place; all normalization work remains charged. A pure contract
negotiator checks version, semantic digest, and required/offered features. Its
agreement carries metadata and grants no authority. A bounded generated corpus
compares mutable schedules with an independent array/history oracle. Deliberate
capacity, empty-range, and owner-survival omissions each produce a reduced
counterexample; the three minimal traces also run as permanent regression cases.
Seventeen selected content faults also fail their designated behavior assertions,
including three partial-publication faults caught by exact snapshot comparisons.
Broader assertion calibration and the full acceptance gates remain incomplete.
This checkpoint is not a conforming model or a compiler/runtime collections
capability.

Read the [draft contract](../../design/collections-reference-v1.md) for place
identity, ownership, snapshot reads, complete candidates, and admission.
The [test plan](test-plan.md) records the required independent oracles and
keeps unfinished cases planned until their executable evidence exists.
The [artifact contract](../../design/collections-wire-v1.md) fixes the wire
records and consumer rejection obligations. Production codecs, schema
registration, and capability advertisement belong to downstream compiler and
provider issues; the metadata checker does not implement them.

The intended model is deterministic and separate from production compilation
and Echo execution. It must distinguish payload equality from write evidence,
and model conformance from production and concurrency claims.
