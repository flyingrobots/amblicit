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
| `parameters` | Optional map of at most 64 entries (the decoder's limit) from a non-empty name to `{ path: [1*16 non-empty text], kind: "uint" / "helper" }`, where the template parameter lives in the operation configuration. |

Pure intrinsics and `types` keep their v2 rules. A v2 catalog is admitted
unchanged; a `/v2` catalog that carries `effectCostRule` or `parameters` is
refused.

The schema requires `effectCostRule` on every effect intrinsic of a v3
catalog; the compiler reads it only for the effects a program uses.

**Depth limit: level 10.** Expressions, measures and types share one nesting
level: `steps` and `bytes` are level 0, and each nested expression, measure,
type or record field is one level deeper. Every expression, measure and type
must sit at level 10 or shallower (`TARGET_EFFECT_COST_MAX_DEPTH`). The provider
artifact-schema registry refuses recursive choices it cannot dispatch to one
arm per value, as these cost rules were, so the schema writes them out level by
level to level 10. Effect-cost admission (`admit_target_effect_costs`)
uses the same limit from the same constant. The schema applies it to every rule
in the catalog; the compiler applies it to every rule it reads, which are
those of the effects the program uses, so an over-deep rule on an unused
effect is refused only by the schema. Echo's `/v3` catalog nests to
level 6.

Why 10 and not 16: the registry's construction work roughly doubles with each
level. Measured on an Apple Silicon laptop with a temporary, uncommitted test
that timed `ProviderArtifactSchemaRegistry::from_manifest` over this root
(`cargo test`, Rust 1.96.0): 12 to 21 ms (release) and 77 to 105 ms (debug)
across three runs at depth 10, against one run each of about 750 ms (release)
and 5.6 s (debug) at depth 16. The previous pack's roots build in about 1 ms.

CDDL fixes shape only. Admission (`admit_target_effect_costs`) checks the
pinned catalog digest, nesting depth (at most level 10, as above), path syntax (`then` only
after an item step) and reads parameter values from the operation
configuration. A `then` without an item step is refused by both. Evaluation of a charge, not admission, resolves type paths
against the effect's signature and refuses `u64` overflow.

The pack coordinate remains `edict.provider-contract-pack.cddl@1` and the manifest
API version remains `edict.provider-contract-pack/v1`. Consumers select the exact
CDDL/manifest pair and verify its schema digest; the same coordinate does not make
different publication bytes interchangeable.

This pair was regenerated in place after #351 published it: the first bytes
wrote `effect-cost-expr` and `effect-cost-type` as recursive rules, which the
provider artifact-schema registry refuses (`SchemaCompileFailed`), so no
provider package carrying them could load. No consumer could depend on those
bytes.

Run `cargo xtask provider-contract-pack --write` to regenerate this current pair
and the generated effect-cost block of `docs/abi/edict-target-profile.cddl`.
`cargo xtask provider-contract-pack --check` and `cargo xtask verify` check it.
The typed-failures pair and every earlier pair remain byte-identical; tests
reproduce the typed-failures pair from
`fixtures/target-profile/pre-effect-cost-target-profile.cddl` and show that it
refuses Echo's vendored `/v3` catalog
(`fixtures/echo-target-intrinsics-v3/`), which this pair admits.
