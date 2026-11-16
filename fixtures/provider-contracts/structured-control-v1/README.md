# Structured-control provider contract publication

This publication adds the `echo.span-ir/v3` variant of the `target-ir-artifact`
root to the [source-module publication](../source-modules-v1/README.md). Its
`executionOrder` is a tree: instruction ids as text, plus `for` and `branch`
maps whose nested arrays use the same entry rule. This CDDL/manifest pair is now
frozen. The generator writes the separate
[pure-intrinsic-catalog publication](../pure-intrinsic-catalog-v1/README.md); tests
reproduce this pair from the pre-fold Core schema and the frozen target-profile
schema in [`fixtures/target-profile/pre-catalog-v2-target-profile.cddl`](../../target-profile/pre-catalog-v2-target-profile.cddl).

The source-module, source-function and v1 pairs remain byte-identical. Their
Target IR schema bytes are preserved in
[`fixtures/target-ir/pre-structured-target-ir.cddl`](../../target-ir/pre-structured-target-ir.cddl),
and tests reassemble each prior publication from those bytes, its frozen Core
schema (`fixtures/core/pre-fold-core.cddl`, or
`fixtures/lang/functions/legacy-core.cddl` for v1) and the frozen
target-profile schema. The source-module
publication refuses a v3 artifact rather than reinterpreting it.

CDDL establishes structural shape only. The Target IR encoder owns the
exactly-once instruction cover, unique control ids and loop/arm scoping, and
Core validation owns types and authority. The generic contract-pack API/domain
coordinates remain v1; consumers select these exact bytes explicitly. Selecting
this schema is not provider support: Echo's provider checks the Target IR
domain exactly and does not accept v3, and no runtime loop or branch execution
is claimed.
