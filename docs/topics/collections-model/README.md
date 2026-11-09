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
Initial membership, request count, owner depth, and mutating history have fixed
model limits. Constructors refuse invalid membership; exhausted history permits
read-only corroboration. Immutable content, outcome precedence, and nested
evaluation are not yet implemented. This checkpoint is not a conforming model or a compiler/runtime
collections capability.

Read the [draft contract](../../design/collections-reference-v1.md) for place
identity, ownership, snapshot reads, complete candidates, and admission.
The [test plan](test-plan.md) records the required independent oracles and
keeps unfinished cases planned until their executable evidence exists.

The intended model is deterministic and separate from production compilation
and Echo execution. It must distinguish payload equality from write evidence,
and model conformance from production and concurrency claims.
