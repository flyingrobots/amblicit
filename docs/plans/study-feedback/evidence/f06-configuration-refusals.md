# Configurable-cell configuration refusal evidence

Reader job: distinguish observed rejection boundaries from intended parameter consistency.

The `configurable_cell_real_provider_accepts_both_variants` integration test in
`crates/edict-cli/tests/configurable_causal_cell.rs` uses the actual public Edict
binary and the Echo package pinned by the how-to. For each 256-byte and 4,096-byte
variant, in two distinct directories, it authors a modified configuration and
regenerates the source manifest pin before application build.

| Configuration | Observed structured refusal | Boundary |
| --- | --- | --- |
| `maxReplacementBytes = 0` | `InvalidProviderInvocation` | Owning provider schema admission |
| `replacementField = nodeKeyField = key` | `ProviderLowererRefused` | Pinned Wasm lowerer |

Each failure leaves the previously published executable package and verification
report byte-identical. Reauthoring with the original document restores the
lawpack closure before stale-pin and output-budget checks run.

Executed characterization command, with the pinned package copied into the
bounded worker:

```text
EDICT_F06_PROVIDER=/data/f06-provider cargo test -p edict-cli --test configurable_causal_cell configurable_cell_real_provider_accepts_both_variants -- --ignored
```

Observed result: one passed, six filtered out. The first attempt expected the
zero cap to reach the lowerer; schema admission rejected it earlier, and the
expected kind was corrected. This is characterization of existing behavior,
not evidence of a new behavior fix or a claimed RED/GREEN cycle.

At this witness snapshot the broader LAUTH-TP-020 scalar/type-to-byte-cap
consistency witness remained planned; the subsequent independent-cap experiment
and correction are recorded in F06.md. These two refusals do not establish arbitrary metadata consistency,
independent verifier rejection of a substituted executable package, or runtime
execution. The default suite does not run this ignored provider test.
