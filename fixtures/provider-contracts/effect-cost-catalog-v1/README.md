# Effect-cost-catalog provider contract publication

This cumulative publication is the
[typed-failures publication](../typed-failures-v1/README.md) with one root
changed: `intrinsics-document`, bound to the `target-profile-intrinsics`
contract, admits `edict.target-profile.intrinsics/v3` catalogs as well as
`/v2` catalogs (issue #341). Every other root is unchanged.

A v3 catalog differs from v2 in three ways:

| Field | v3 rule |
| --- | --- |
| `apiVersion` | Exactly `edict.target-profile.intrinsics/v3`. |
| effect intrinsic | The v2 shape plus a required `effectCostRule`: `{ steps, bytes }`, each a cost expression (`const`, `sum` of 2 to 16 terms, binary `product`, or `measure`). |
| `parameters` | Optional map from a non-empty name to `{ path: [1*16 non-empty text], kind: "uint" / "helper" }`, where the template parameter lives in the operation configuration. |

Pure intrinsics and `types` keep their v2 rules. A v2 catalog is admitted
unchanged; a `/v2` catalog that carries `effectCostRule` or `parameters` is
refused.

CDDL fixes shape only. `admit_target_effect_costs` owns the rest: the pinned
catalog digest, expression depth (at most 16), checked `u64` arithmetic, type
paths resolved against the effect's signature, `then` only after an item step,
and parameter values read from the operation configuration.

The pack coordinate remains `edict.provider-contract-pack.cddl@1` and the manifest
API version remains `edict.provider-contract-pack/v1`. Consumers select the exact
CDDL/manifest pair and verify its schema digest; the same coordinate does not make
different publication bytes interchangeable.

Run `cargo xtask provider-contract-pack --write` to regenerate this current pair.
`cargo xtask provider-contract-pack --check` and `cargo xtask verify` check it.
The typed-failures pair and every earlier pair remain byte-identical; tests
reproduce the typed-failures pair from
`fixtures/target-profile/pre-effect-cost-target-profile.cddl` and show that it
refuses Echo's vendored `/v3` catalog
(`fixtures/echo-target-intrinsics-v3/`), which this pair admits.
