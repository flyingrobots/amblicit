# Bounded-state-fold provider contract publication

This publication adds `fold-expr` to the shared Core expression grammar while
preserving the [structured-control publication](../structured-control-v1/README.md).
The new form is also available in Target IR fields that reference `core-expr`.
This is structural schema acceptance, not provider verification or runtime support.
Capability, state identity, helper authority, cost and execution remain semantic
checks owned by their respective consumers.

The pack coordinate remains `edict.provider-contract-pack.cddl@1` and the manifest
API version remains `edict.provider-contract-pack/v1`. Consumers select the exact
CDDL/manifest pair and verify its schema digest; the same coordinate does not make
different publication bytes interchangeable.

This CDDL/manifest pair is now frozen. The generator writes the separate
[pure-intrinsic-catalog publication](../pure-intrinsic-catalog-v1/README.md).
This pair's `intrinsics-document` root admits only
`edict.target-profile.intrinsics/v1` catalogs, and its Core schema has no
function `preconditions`; tests reproduce the pair from the frozen target-profile
schema in
[`fixtures/target-profile/pre-catalog-v2-target-profile.cddl`](../../target-profile/pre-catalog-v2-target-profile.cddl)
and the frozen Core schema in
[`fixtures/core/pre-precondition-core.cddl`](../../core/pre-precondition-core.cddl).
The v1, source-function, source-module and structured-control CDDL/manifest pairs
remain byte-identical. Tests reproduce the earlier pairs using their frozen Core,
Target IR and target-profile inputs, including `fixtures/core/pre-fold-core.cddl`.
