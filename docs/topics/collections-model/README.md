# Collections reference model

This shelf owns the collections reference contract and its conformance plan.
The contract and executable model are under development in
[issue #290](https://github.com/flyingrobots/amblicit/issues/290); no model or
compiler/runtime collections capability is implemented by this checkpoint.

Read the [draft contract](../../design/collections-reference-v1.md) for place
identity, ownership, snapshot reads, complete candidates, and admission.
The [test plan](test-plan.md) records the required independent oracles and
keeps every case planned until its executable evidence exists.

The intended model is deterministic and separate from production compilation
and Echo execution. It must distinguish payload equality from write evidence,
and model conformance from production and concurrency claims.
