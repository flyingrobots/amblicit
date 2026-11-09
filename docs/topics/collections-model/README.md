# Collections reference model

This shelf owns the collections reference contract and its conformance plan.
The contract and executable model are under development in
[issue #290](https://github.com/flyingrobots/amblicit/issues/290). The standalone
`amblicit-collections-model` crate currently checks whole-candidate capacity,
rejects duplicate exclusive mutations, and corroborates slot/range observations
against current state. Reads use the starting snapshot, including empty and
filtered traversals. It has no production compiler consumers.
The remaining laws, ownership support, and model-wide bounds are not yet
implemented. This checkpoint is not a conforming model or a compiler/runtime
collections capability.

Read the [draft contract](../../design/collections-reference-v1.md) for place
identity, ownership, snapshot reads, complete candidates, and admission.
The [test plan](test-plan.md) records the required independent oracles and
keeps unfinished cases planned until their executable evidence exists.

The intended model is deterministic and separate from production compilation
and Echo execution. It must distinguish payload equality from write evidence,
and model conformance from production and concurrency claims.
