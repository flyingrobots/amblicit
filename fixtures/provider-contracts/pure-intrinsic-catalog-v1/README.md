# Pure-intrinsic-catalog provider contract publication

This publication changes one root of the
[bounded-state-fold publication](../bounded-state-fold-v1/README.md):
`intrinsics-document`, bound to the `target-profile-intrinsics` contract, now
admits `edict.target-profile.intrinsics/v2` catalogs. Every other root is
unchanged.

A v2 catalog differs from v1 in three ways:

| Field | v2 rule |
| --- | --- |
| `apiVersion` | Exactly `edict.target-profile.intrinsics/v2`. |
| `types` | Optional map from a provider-owned type coordinate to its structural `Record<...>` definition. |
| pure intrinsic | `typeParameters` is empty, and `contractVersion` and `costRule` (`base` plus one `perArgumentMax` coefficient per argument, all unsigned) are required. |

Effect intrinsics keep their v1 shape. A provider that publishes only effect
intrinsics moves to this pack by changing the catalog's `apiVersion`.

The two publications are disjoint for catalogs. This pack refuses a v1
catalog, and the bounded-state-fold pack refuses a v2 catalog. The compiler
admits pure intrinsics only from a v2 catalog, so a provider whose source
programs call pure intrinsics must select this pack.

CDDL establishes structural shape only. Admission
(`admit_target_pure_intrinsics`) owns the checks a schema cannot state: the
trusted catalog digest, exact contract versions, namespace ownership, type
resolution, one cost coefficient per argument, and a zero coefficient for every
argument that is not `Bytes` or `List`. Selecting this schema is not provider or
runtime support for any intrinsic.

The pack coordinate remains `edict.provider-contract-pack.cddl@1` and the manifest
API version remains `edict.provider-contract-pack/v1`. Consumers select the exact
CDDL/manifest pair and verify its schema digest; the same coordinate does not make
different publication bytes interchangeable.

Run `cargo xtask provider-contract-pack --write` to regenerate this current pair.
`cargo xtask provider-contract-pack --check` and `cargo xtask verify` check it.
The v1, source-function, source-module, structured-control and
bounded-state-fold CDDL/manifest pairs remain byte-identical. Tests reproduce
them from frozen inputs, including
[`fixtures/target-profile/pre-catalog-v2-target-profile.cddl`](../../target-profile/pre-catalog-v2-target-profile.cddl).
