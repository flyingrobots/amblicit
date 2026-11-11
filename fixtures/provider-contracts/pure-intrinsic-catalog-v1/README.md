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
| `types` | Optional map from a provider-owned type coordinate to a text definition. Admission, not the schema, requires each definition to be a structural `Record<...>` and resolves every type a signature names. |
| pure intrinsic | Every pure intrinsic, required by a consumer or not: `typeParameters` is empty; `contractVersion`, `footprintTemplate` and `costTemplate` are non-empty text; `costRule` is required, with an unsigned `base` and an array of unsigned `perArgumentMax` coefficients. |

Effect intrinsics keep their v1 shape, so an effect-only v1 catalog becomes a
valid v2 catalog when its `apiVersion` changes. That edit changes the catalog's
bytes, so its pinned digest, and every target profile, package and piece of
evidence that pins it, must be regenerated. A generic pure constructor cannot be
listed in a v2 catalog at all; only the frozen v1 publications can carry one.

The two publications are disjoint for catalogs. This pack refuses a v1
catalog, and the bounded-state-fold pack refuses a v2 catalog. The compiler
admits pure intrinsics only from a v2 catalog, so a provider whose source
programs call pure intrinsics must select this pack.

CDDL establishes structural shape only. Admission
(`admit_target_pure_intrinsics`) owns the checks a schema cannot state: the
trusted catalog digest, exact contract versions, namespace ownership, type
resolution, exactly one cost coefficient per argument, and a zero coefficient for
every argument that is not `Bytes` or `List`. Selecting this schema is not
provider or runtime support for any intrinsic.

Admission reads only the pure intrinsics a consumer requires. It does not
inspect the catalog's other entries, so it can admit facts from a catalog this
pack refuses, for example one that also lists an uncalled generic constructor or
a malformed effect intrinsic.
Such a catalog cannot be published under any pack. Validate a catalog against
this pack before pinning it.

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
