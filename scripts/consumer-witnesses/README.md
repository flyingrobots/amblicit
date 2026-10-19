# Jim state-read boundary witness

This Docker-only integration witness builds the current Edict source and
fetches two exact public inputs: Jim's authored state-read probe at
`19edb6fba94a8fea2dea63aa2f05cffc3e084f97` and Echo's existing provider at
`49e9efb68001dfd78563d18bac9359a87671e431`.

Reuse a compatible COPY-based worker and its existing Cargo cache first. Copy
this revision and the pinned consumer/provider source into that worker, then
run `sh scripts/consumer-witnesses/run-jedit-state-read.sh` from the copied
repository root under its validation lock. `CARGO_TARGET_DIR` must be absolute;
the entrypoint selects the binary from that directory. Do not mount a host
repository, Git directory, or recovery checkout.

If no compatible worker exists, the following standalone invocation builds a
source/toolchain image and compiles only at container runtime. Its read-only
root and size-limited temporary filesystems cap generated build storage at
8 GiB and scratch data at 256 MiB. These caches disappear when the container
exits; use the existing worker for repeated audits. Check aggregate project
cache usage against the 20 GiB budget and host free space against the 50 GiB
floor before running.

From the repository root:

```sh
docker build -f scripts/consumer-witnesses/jedit-state-read.Dockerfile \
  -t edict-consumer-witness .
docker run --rm --read-only --cpus=4 --memory=12g --pids-limit=512 \
  --tmpfs /build-cache:rw,size=8g --tmpfs /tmp:rw,size=256m \
  edict-consumer-witness
```

No Cargo output is baked into the image. Docker compiles Edict at runtime and
runs both public JSONL
lawpack/application builds in temporary container storage. The first build
retains Jim's original v1 adapter and reproduces `TargetLoweringFailed`. The
second explicitly selects `echo.span-ir/v2` in a disposable lawpack copy,
binds its contract reference to the current CDDL bytes, republishes the lawpack,
and updates only the source's lawpack import digest.

The authored read/guard body is unchanged. It passes Core and Target lowering,
canonical encoding, and result-projection verification, then reaches the old
provider's schema gate: `InvalidProviderInvocation`. Neither attempt publishes
application output. Successful verification prints
`ORDERED_PUBLIC_BUILD_REACHED_OLD_PROVIDER_GATE`.

The v2 contract reference is experimental; this is not evidence of a released
Echo v2 provider. No executable artifact is handwritten, no native edit planner
is called, and no runtime state read occurs. Original Jim application locks and
producer pins are not edited. Echo #684 owns subsequent provider/interpreter
support; Jim #296 owns complete ReplaceRange behavior.


## Source-function compatibility witness

`jedit-source-functions.py` uses the same checked-in Jim range-assembly source
with and without a called pure function. It takes explicit compiler source,
source hash manifest/revision, original Jim application/vendor tree and old
provider package/manifest digest arguments. It builds neither dependencies nor
providers and modifies only two disposable application copies under the caller's
bounded `--data-root`.

Run it only inside the existing admitted guarded Docker worker; no new image,
cache or worker is required. See `--help` for required paths. Supply Jim's
original `edict/replace-range` application, whose lawpack digest matches the
range-assembly fixtures, rather than the later atom-read probe. The old provider
must support the function-free byte-assembly control.

The combined vendor/provider input must be at most 16 MiB; there are two copies.
Each compiler child runs in its own process group with a 120-second timeout and
TERM/KILL cleanup, disabled core dumps and a hard 1 MiB limit on each written
file, including stdout, stderr and application artifacts. The outer shared
guard must still enforce aggregate build/data/log budgets and stop the container
if monitoring fails. A file cap, timeout or process failure is not accepted as
the expected semantic refusal.

Success prints `JIM_SOURCE_FUNCTION_OLD_PROVIDER_REFUSAL_CONFIRMED`. The
function-free control must publish a package/report pair. The function-bearing
source must return `InvalidProviderInvocation` with `ArtifactSchemaMismatch`
and no application artifacts. `evidence.json` records the input selections,
compiler/source hashes, source hashes, diagnostics, outputs and log hashes.
The harness neither updates old provider schema bytes nor claims new runtime
support. Retain unique evidence before recycling its disposable copies.

The child-lifecycle regression runs in the same admitted guarded worker:

```sh
python3 -B scripts/consumer-witnesses/test_jedit_source_functions.py
```

It uses a compiler stand-in that exits successfully while a descendant ignores
TERM; the witness must stop that descendant before returning artifact hashes.
The test also kills its owned process group on failure. This is separate from
`cargo xtask verify` and from the real compiler compatibility witness.
