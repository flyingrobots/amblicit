# F01 Validation Evidence

This index separates passed runs, failed runs, and superseded observations.
Logs retain their original bytes. A file's location in this directory does not make it passing evidence.

| Artifact | Observed result | Scope and limit |
| --- | --- | --- |
| [F01-red.txt](F01-red.txt) | RED: 3 failed | Initial tests existed before implementation. This scope remains historical evidence. |
| [f01-red-parent.txt](f01-red-parent.txt) | RED: 5 failed | Final original regression functions on planning parent 313442b. It does not supersede the initial three-test scope. |
| [f01-partial-syntax-missing-clippy.txt](f01-partial-syntax-missing-clippy.txt) | Partial: syntax passed, clippy unavailable | Failed setup for the strict check. Not full-gate evidence. |
| [f01-verify-superseded-cache-sensitive.txt](f01-verify-superseded-cache-sensitive.txt) | Exit 0, superseded | This run lacked explicit workspace artifact invalidation. Do not use it as fresh-source proof. |
| [f01-admitted-final.txt](f01-admitted-final.txt) | GREEN | Fresh default-feature regressions and full cargo xtask verify on cfcf61e. Does not approve later review fixes. |
| [pr241-width-red.txt](pr241-width-red.txt) | RED | Imported I8 output incorrectly reports UnsupportedSourceShape. |
| [pr241-width-green.txt](pr241-width-green.txt) | GREEN | All eight accepted imported integer widths fit/reject at their encoding boundary. |
| [pr241-guard-red.txt](pr241-guard-red.txt) | RED | Optimized Python removes assertions and admits a bad free-space condition. |
| [pr241-guard-green.txt](pr241-guard-green.txt) | GREEN | Optimized guard refuses all six invalid resource measurements. |
| [pr241-oracles-interrupted-measurement.txt](pr241-oracles-interrupted-measurement.txt) | Interrupted | The resource guard stopped a measurement race. Not GREEN evidence. |
| [f01-goldens.txt](f01-goldens.txt) | Generated fixtures | Regeneration alone is not checking. The admitted full gate subsequently checks these outputs. |

[F01-validation.json](F01-validation.json) records exact commands, source versions, final-run relationships, and measured resource use.
[guard-runner.py.txt](guard-runner.py.txt) retains the project helper. It is not an installed general runner.
[guard-optimization-regression.py.txt](guard-optimization-regression.py.txt) executes refusal behavior under optimized Python in Docker.

The original study is source-preserved separately. Its runtime claims are not inferred from these compiler logs.

The guard reserves 128 MiB within the existing build budget for Cargo's transient metadata journal.
A missing journal is expected after deletion; all other measurement failures still stop the worker.

Literal command output retains terminal blank lines through a scoped Git attribute.
Ordinary source retains the existing EOF whitespace check.
[literal-evidence-regression.py.txt](literal-evidence-regression.py.txt) verifies both behaviors in an isolated Git fixture.

The [reviewed full gate](pr241-reviewed-full-gate.txt) passes all thirteen focused compiler tests, both Python behavior probes, and cargo xtask verify.
It supersedes intermediate verification for that review round; later corrections have separately named evidence below.

The snapshot-deletion correction is covered by
[snapshot-deletion-regression.py.txt](snapshot-deletion-regression.py.txt).
Its real Git fixture found that staged removals and old rename paths survived
the overlay. [pr241-snapshot-deletion-red.txt](pr241-snapshot-deletion-red.txt)
retains that failure; [pr241-snapshot-deletion-green.txt](pr241-snapshot-deletion-green.txt)
retains the corrected file-set and byte comparison. The helper now combines
HEAD-relative deletions with missing index paths. Earlier logs remain unchanged;
the correction does not retroactively change their source snapshots.

[pr241-snapshot-full-gate.txt](pr241-snapshot-full-gate.txt) retains the passing
snapshot, optimized-guard and whitespace probes followed by `cargo xtask verify`.
The topic plan records the Python evidence-registration limitation explicitly;
this run does not claim Rust-test coverage for the Python probe. Two earlier
full-gate attempts failed on the new topic row metadata, not on snapshot behavior.
