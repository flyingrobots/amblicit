# Feedback on Edict, from authoring a board-history operation for Echo

To: the author of Edict. From: a worker that spent one afternoon (2026-10-06) writing Edict for Echo, against Edict `origin/main` **`01161c1745baad0d713234ba1a671b9a26923aa4`** (`edict-cli` 0.11.0-alpha.1) and Echo `origin/main` `a93e9d82e89455ed1fa0b63447c88de544b9da26`. The design note is `/Users/james/git/synapse-echo-experiments/docs/synapse-echo.md`; raw outputs are under `/Users/james/git/synapse-echo-experiments/results/`.

**How to read an item.** *Tried* is the exact source or command. *Happened* is the output. *Label*: `ran` (raw output named), `read` (file and line named), `inferred` (with what would settle it). Severity is one of bug, footgun, missing feature, docs gap, perf, praise. Every item traces to something I ran or read.

The application under test, `edict-app/src/synapse_board.edict`:

```edict
package examples.synapse_board@1;

use lawpack causal.cell@1 digest "sha256:6877b7b3a0b79cb56c06a9eb070310683e7a57bffdf8eccec7cabdac9db201d9" as cell;

type RecordFactInput = { basis: String<max=128>, key: String<max=64>, value: String<max=256> };
type CellCreateReceipt = { key: String<max=64> };
type FactRecorded = { key: String<max=64>, descriptor: String<max=256> };

intent recordFact(input: RecordFactInput) returns FactRecorded
  profile cell.createIfAbsent
  basis input.basis
  budget <= cell.smallCreateBudget
{
  let receipt: CellCreateReceipt = cell.createIfAbsent(input)
    else { alreadyExists(existing) => cell.AlreadyExists };
  return { key: receipt.key, descriptor: input.value };
}
```

(Comments omitted here; the file has them.) Built with `cd edict-app && .../target/edict/release/edict < build-request.jsonl`, where the request is `{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","application":"edict.application.json"}`.

---

## 1. The compiler accepts a return type that cannot fit the declared output budget; it fails at runtime instead

- **Severity:** bug.
- **Tried:** a widened `causal.cell` lawpack (generator in a second export of `01161c17` changed to `maxReplacementBytes` 4096, `writeBytes` 4160, `value:String<max=4096>`; diff `results/e3-wide-generator.diff.txt`; regenerated with `cargo xtask lawpack-goldens --write`, exit 0), and the intent above with `value` and `descriptor` widened to `String<max=4096>` (`edict-probes/p4-wide-lawpack/src/synapse_board.edict`). `budget <= cell.smallCreateBudget` is unchanged; that budget has `maxOutputBytes` 512 (`xtask/src/lawpack_goldens.rs:963-967`, read).
- **Happened (ran):** `edict build` exit 0 (`results/e3-wide-build.jsonl`). At runtime in Echo, values up to 448 bytes commit and 480 and above end in obstruction `ResultProjectionInvalid` (`results/e3-pad480.stderr.txt`), which is Echo's projection check refusing a result larger than the compiler-declared output bound (`vendor/echo/crates/warp-core/src/echo_operation.rs:1261-1273`, read). Returning only `{key}` (`edict-probes/p5-wide-key-only`) makes 3,900 and 4,096-byte values commit (`results/e1-keyonly-value-*.meta.json`).
- **Expected:** the README's promise that "If the cost bounds aren't satisfiable, it doesn't compile" (`README.md`, "Enter Edict"). The worst-case canonical size of `FactRecorded` is statically known from the `max=` bounds, and it exceeds 512.
- **Read:** the compiler compares `cost.output_bytes > intent.budget.max_output_bytes` (`crates/edict-syntax/src/compiler.rs:1153`); inferred: `cost.output_bytes` is not derived from the return type's bounds for this shape. Settles it: print the computed `cost.output_bytes` for this intent.
- **Suggested fix:** compute the worst-case canonical-CBOR size of the return type from its `max=` bounds (header bytes included) and reject at type-check with a diagnostic such as `return type FactRecorded can encode to 4,140 bytes; budget cell.smallCreateBudget allows 512 (maxOutputBytes)`.

## 2. Diagnostics: one root cause, three errors, and a misleading first message

- **Severity:** footgun (error messages).
- **Tried (ran, `scripts/e0-probes.sh`):**
  - p3: removed the failure mapping, `let receipt: CellCreateReceipt = cell.createIfAbsent(input);`
  - p2: called an undeclared capability, `cell.update(input)`
  - p1: widened `value: String<max=4096>` against the stock lawpack
- **Happened (ran, `results/e0-p*.stderr.jsonl`):**
  - p3: `UnresolvedFunction: "pure helper \`cell.createIfAbsent\` has no compiler context fact"`, then `"identifier \`receipt\` has no typed binding"`, then `"intent body must return a value"`.
  - p2: `MissingContextFact: "effect \`cell.update\` has no compiler context fact"`, plus the same two cascades.
  - p1: `TypeMismatch: "effect \`cell.createIfAbsent\` call does not match its exported signature"`, plus the same two cascades.
- **Expected:** p3 to say the effect needs an `else { alreadyExists(..) => .. }` mapping. It instead treats an effect call without `else` as a call to a pure helper and reports a missing context fact, which sent me looking at lawpack loading. p2 to say "lawpack causal.cell@1 exports no effect `update`; it exports: createIfAbsent". p1 to name the mismatching field (`value: String<max=4096>` vs `String<max=256>`). In all three, the second and third errors are consequences of the first.
- **Suggested fix:** (a) when a bare call resolves to an exported *effect*, emit `EffectWithoutFailureMapping` naming the effect's declared failures; (b) list the lawpack's exports in the "no context fact" diagnostic; (c) report the first differing field in a signature mismatch; (d) suppress dependent errors once a binding failed (mark `receipt` as poisoned).

## 3. Diagnostics arrive as a Rust `Debug` string inside one JSON message, with byte-offset spans

- **Severity:** footgun (tooling).
- **Happened (ran):** every build failure above is one record of kind `ApplicationCompilationFailed` whose `message` is `Edict application did not compile to Core: [CompilerError { stage: TypeCheck, kind: TypeMismatch, message: "...", span: Span { start: 920, end: 1039 } }, ...]`.
- **Expected:** the `check` workflow's structured records (`edict.cli.diagnostic/v1` with stable `kind`, README Build & Run, read) for `build` too: one record per error, `kind` as a field, file path plus line and column.
- **Suggested fix:** map each `CompilerError` to its own diagnostic record with `path`, `line`, `column`, `kind`, `stage`, `message`; keep the byte span as an extra field.

## 4. README says `build` does not exist; it does, and it is the path that matters

- **Severity:** docs gap.
- **Read:** `README.md:639` lists under "What doesn't exist yet": "Compiler CLI workflows beyond JSONL `check`, including compile, lower, explain, bundle, admission, ...", and `README.md:17-19` says the alpha implements "stage 1 and the front half of stage 2". `docs/topics/cli/README.md:24` says "The implemented operations are `build`, `check`, and `project`."
- **Ran:** `operation: "build"` produced `executable-operation-package.cbor` and `verification-report.cbor`, which Echo then executed (`results/e1-fact.stdout.json`).
- **Read:** `README.md:649` lists "Target-runtime execution, Echo verifier reports" as not existing; Echo's README describes a standalone Edict operation crossing into Echo execution (`vendor/echo/README.md:194-199, 277-281`). Both can be true from each repo's own scope, but a reader starting at Edict's README concludes that Edict-to-Echo execution is not possible today.
- **Suggested fix:** update "Current Status" to name `build` (application and lawpack), and add a three-line "Run it on Echo" pointer to `cargo xtask run-edict-operation` and the hello-echo template.

## 5. The only executable Echo capability is create-if-absent, so an application cannot express "update"

- **Severity:** missing feature.
- **Read:** `fixtures/lawpack/` holds `causal-cell`, `hello-echo`, `workspace-patch`, `workspace-snapshot`; `causal.cell@1` exports one effect, `createIfAbsent` (`fixtures/lawpack/causal-cell/README.md:4-14`; strings in `exports.cbor`). Echo's runtime has an update-only compare-and-set program kind (`vendor/echo/README.md:180-182, 274`; `warp-core/src/echo_operation.rs:50`), but `grep -rl compare-and-set` finds nothing in Edict or in Echo's Edict provider lowerer (ran).
- **Ran:** `cell.update(input)` is refused (item 2, p2).
- **Why it matters:** a board needs "current revision of block X". With only create-if-absent, every revision is a new key and "current" must be derived elsewhere.
- **Suggested fix:** a `causal.cell@1.compareAndSet(key, expected, replacement) else { mismatch(current) => .. }` effect lowered to Echo's existing CAS program kind, with a fixture and golden like `createIfAbsent`.

## 6. Value bounds are fixed by a generator constant, and widening them means forking the generator

- **Severity:** missing feature.
- **Read:** the 256-byte value bound, 64-byte key bound, `maxReplacementBytes` 256, `writeBytes` 320 and `maxOutputBytes` 512 are literals in `xtask/src/lawpack_goldens.rs:973-1043` and `:963-967`. The authoring guide (`docs/topics/lawpack-authoring/README.md`) builds lawpacks from JSON, but its example has no `targetAdapters`, and it says Edict does not "build an executable package, admit an operation, or create a runtime receipt on this path" (`:338-340`), so I could not tell how to author an Echo-executable lawpack that way.
- **Ran:** I widened the generator (item 1). Two things had to change together that the generator does not keep together: the export types and the target configuration, and the generator's own witness source (`message: String<max=256>` at line 1121), whose failure was `compile causal.cell application witness: [... "return value does not match declared output type" ...]` (`results/e3-lawpack-goldens.log`, first run).
- **Suggested fix:** parameterize `causal.cell` (for example `causal.cell@1` with `valueMax` and `keyMax` as lawpack parameters, or a family `causal.cell.v4k@1`), derive `maxReplacementBytes`, `writeBytes` and the witness types from one number, and document the Echo-executable lawpack recipe in the authoring guide.

## 7. The import digest is the domain digest from `manifest.sha256`, not the file hash

- **Severity:** docs gap (minor).
- **Ran:** `shasum -a 256 fixtures/lawpack/causal-cell/manifest.cbor` is `d38f1012...`; the digest the source must import, and which compiled, is `6877b7b3...` from `manifest.sha256`. Echo's runner computes it with `digest_canonical_value_bytes_v1("edict.lawpack/v1", manifest)` (`vendor/echo/xtask/src/run_edict_operation.rs:837`, read).
- **Expected:** I found the right value only because the sidecar exists. I did not find a sentence saying which digest `use lawpack ... digest` means (search frontier: Edict `README.md` and `fixtures/lawpack/causal-cell/README.md`; I did not read the full language spec).
- **Suggested fix:** one sentence in the language README, and a diagnostic that, on a digest mismatch, says "expected the edict.lawpack/v1 domain digest; the file's sha256 is not it".

## 8. The hello-echo template pins old producers

- **Severity:** docs gap.
- **Read:** hello-echo `origin/main` (`ee45716e`) pins Edict `df80f92a` and Echo `490134c0` in `producers.lock.json`, and `tests/producer-lock.sh` refuses any other `HEAD`. Its `src/hello_echo.edict` imports `causal.cell@1` at `sha256:42d96b95...`, not the `6877b7b3...` at Edict `01161c17`.
- **Ran:** replicating `tests/build.sh` by hand against the two `main`s worked (item 10).
- **Suggested fix:** advance the pins with each Edict release, or say in Edict's README which hello-echo commit matches.

## 9. Possible inconsistency: Edict types are not enforced at Echo ingress

- **Severity:** footgun (cross-repo; Echo documents it).
- **Ran:** a 65-character key against `key: String<max=64>` committed in Echo (`results/e1-key-65.stdout.json`). Echo states it does not validate codec-owned input against the operation schema at ingress (`vendor/echo/README.md:168-170`).
- **Suggested fix (Edict side):** emit the key bound into the Echo operation-lowering configuration beside `maxReplacementBytes`, so the runner can enforce it as it already enforces the value bound.

## 10. What worked well

- **Praise (ran):** a new application compiled first time and ran on Echo with no host code from me: the 37-line source above, the shipped Echo provider package, `edict build`, then `xtask run-edict-operation` with every recovery and duplicate witness true (`results/e1-fact.stdout.json`).
- **Praise (ran):** reproducible builds: the same build from a copied directory gave byte-identical `executable-operation-package.cbor` (`4769b89d...`) and `verification-report.cbor` (`65abfdff...`) (`results/e0-p0-rebuild.sha256.txt`).
- **Praise (ran):** the safety properties held where they exist: widening a type against the lawpack, calling an undeclared capability and omitting a failure mapping were all refused at compile time (item 2), and the value bound is enforced before submission.
- **Praise (read):** the language is small and readable; the A-normal-form effect rule and typed obstructions made the board's "never overwrite a revision" rule a property of the operation (`AlreadyExists`) rather than a convention.
- **Build (ran):** `cargo build --release -p edict-cli` clean in 1m50s on rustc 1.96.0.
