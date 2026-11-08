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

Run `cargo xtask provider-contract-pack --write` to regenerate this current pair.
`cargo xtask provider-contract-pack --check` and `cargo xtask verify` check it.
The v1, source-function, source-module and structured-control CDDL/manifest pairs
remain byte-identical. Tests reproduce the earlier pairs using their frozen Core
and Target IR inputs, including `fixtures/core/pre-fold-core.cddl`.
