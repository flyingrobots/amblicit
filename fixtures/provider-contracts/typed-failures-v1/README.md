# Typed-failures provider contract publication

This cumulative publication retains the catalog-v2 and function-precondition
contracts from the preceding publications, then adds typed function failures.
Core gains an optional `failures` list on a source function, the `fail`
expression and the `attempt` node. Target IR pure bindings in the ordered
(`echo.span-ir/v2`) and structured (`echo.span-ir/v3`) roots gain an optional
`failureArms` map. These are structural schema changes only. Exact failure
sets, `fail` placement, map keys and the `core.failure/v1` capability are
semantic checks owned by Core validation and its consumers. A provider runs
none of this until it implements `core.failure/v1`.

The pack coordinate remains `edict.provider-contract-pack.cddl@1` and the manifest
API version remains `edict.provider-contract-pack/v1`. Consumers select the exact
CDDL/manifest pair and verify its schema digest; the same coordinate does not make
different publication bytes interchangeable.

Run `cargo xtask provider-contract-pack --write` to regenerate this current pair.
`cargo xtask provider-contract-pack --check` and `cargo xtask verify` check it.
The v1, source-function, source-module, structured-control, bounded-state-fold,
function-preconditions and pure-intrinsic-catalog CDDL/manifest pairs remain
byte-identical. Tests reproduce the function-preconditions and pure-catalog
pairs from `fixtures/core/pre-typed-failure-core.cddl` and
`fixtures/target-ir/pre-typed-failure-target-ir.cddl`, using the frozen
catalog-v2 target-profile schema where required.
