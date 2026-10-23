# Edict Architecture

Status: current workspace map for HEAD. This page describes what exists in the
current branch; it is not a future package plan.

## Workspace Shape

The Rust workspace has six members:

```text
flyingrobots-amblicit  ->  amblicit-syntax
amblicit-cli  ->  amblicit-syntax
xtask      ->  amblicit-syntax
amblicit-provider-schema  ->  amblicit-syntax
amblicit-provider-host-wasmtime  ->  amblicit-provider-schema  ->  amblicit-syntax
amblicit-provider-host-wasmtime  ->  amblicit-syntax
```

`amblicit-syntax` has no dependency on the CLI or `xtask`. The CLI owns the public
JSONL process boundary and delegates language work to `amblicit-syntax`. `xtask`
owns repository maintenance checks, reviewed golden regeneration, release
process guards, and topic-shelf contract checks.
`amblicit-provider-schema` owns the immutable digest-bound CDDL registry used to
validate provider artifact instances without a component-runtime dependency.
`amblicit-provider-host-wasmtime` owns the capability-denied external component
runtime without exposing Wasmtime types through Edict contracts.

## Crates

### `flyingrobots-amblicit`

`flyingrobots-amblicit` exposes the Rust library name `amblicit`. It is the curated
public facade for source checking, stable diagnostic classifications, and
canonical Core, Target IR, and result-projection artifact identity operations.
The [public Rust API topic](docs/topics/public-rust-api/README.md) owns this
boundary. The facade deliberately does not re-export the implementation crate's
module tree.

The package remains `publish = false`. Its presence defines and tests the
intended public Rust boundary; it neither authorizes nor claims crates.io
publication. The CLI remains the complete application-build and JSONL process
boundary.

### `amblicit-syntax`

`amblicit-syntax` is the implementation crate for more than syntax. Its public name
is historical and currently too narrow for its responsibilities. The crate
exports:

- lexical analysis and parsing;
- source/surface semantic validation;
- compiler context facts and authority-fact loading;
- source-to-Core compiler spine for the current supported subset;
- Core IR data structures, including non-callable typed external-action request
  values;
- depth-bounded canonical-CBOR encoding/decoding plus canonical Core, Target IR,
  bundle-layer encoders and digest helpers;
- target-profile conformance checks;
- lowerability checks;
- Echo and git-warp Target IR artifact lowering;
- provider manifest/provenance validation, pure invocation-envelope validation,
  and explicit built-in lowerer compatibility adapters;
- contract-bundle assembly and validation;
- Gate C admission-boundary request/receipt validation;
- editor/highlighting support.

That breadth is deliberate as current branch truth, not an endorsement of the
name. The crate-scope decision is recorded in
[`docs/design/crate-scope-v0.11.md`](./docs/design/crate-scope-v0.11.md):
eventual layered split behind an umbrella crate, not a simple rename. Until that
split lands, new code should preserve the existing dependency direction and keep
layer boundaries explicit inside the crate.

Module map:

| Module | Responsibility |
| --- | --- |
| `token` | Lexer tokens, spans, integer suffixes, and lexing failures. |
| `parser` | Source grammar parser producing AST modules. |
| `ast` | Source-level syntax tree types. |
| `semantic` | Surface validation that does not require import resolution or target facts. |
| `authority_facts` | File-backed compiler context facts for profiles, budgets, write classes, and source identity. |
| `compiler` | Resolve, type-check, and lower the supported source subset to Core IR, including typed external-action request construction without execution authority. |
| `core_ir` | Runtime-neutral Core module, intent, expression, budget, import, obstruction, and external-action request data. |
| `canonical` | Canonical value model, depth-bounded canonical CBOR encoder/decoder, digest frames, and reviewed golden digest helpers. |
| `target_profile` | Runtime-neutral target-profile manifest conformance. |
| `lowerability` | Checks whether Core requirements can be satisfied natively, by a direct adapter, or not at all. |
| `provider` | Runtime-neutral provider manifest and generated/component provenance envelope validation. |
| `provider_invocation` | Pure host-contract, explicitly injected owning-schema, WIT-shaped request/result, canonical artifact, limit, and sealed output-manifest validation. |
| `provider_lowering` | Explicit in-process compatibility adapters over the current built-in target lowerers. |
| `target_ir` | Current Echo and git-warp Target IR artifact construction from Core plus lowering facts, preserving external requests outside callable target steps. |
| `contract_bundle` | Participant-neutral bundle assembly, bundle digest preimages, validation, and assurance evidence binding. |
| `admission` | Edict-owned Gate C request/receipt shape and binding validation without participant policy execution. |
| `highlight` | Lexical highlight roles consumed by editor tooling. |
| `lib` | Public API facade and re-exports for the current alpha surface. |

### `amblicit-cli`

`amblicit-cli` provides the `amblicit` binary and its small support library. The binary
is JSONL-only today:

- it accepts compiler settings and compiler input records on stdin;
- it supports `check`;
- it emits typed JSONL result, diagnostic, status, and info records;
- it enforces a bounded stdin size;
- it can confine path, path-list, directory, and glob inputs to an optional
  `inputRoot`;
- it delegates parse and surface validation to `amblicit_syntax::check`.

The CLI does not compile to Core, lower to Target IR, assemble bundles, admit
bundles, or execute runtime behavior. It should remain a stream-contract and
local input-boundary owner unless a future topic shelf expands its public
surface.

### `amblicit-provider-schema`

`amblicit-provider-schema` is the concrete, runtime-independent provider artifact
schema authority. It consumes a validated provider manifest plus explicit
in-memory schema bytes, verifies their raw SHA-256 identities, compiles
self-contained CDDL, proves required-domain closure, and implements the pure
`ProviderArtifactSchemaValidator` contract over canonical CBOR values. It has
no provider resolver, filesystem or network loader, Wasmtime dependency, or
mutable registry API.

### `amblicit-provider-host-wasmtime`

`amblicit-provider-host-wasmtime` consumes selected manifest component proofs,
resolver-supplied bytes, opaque validated requests, the concrete schema
registry, and explicit invocation limits. It verifies component bytes against
the selected digest before decoding, enforces a digest-covered exact contract
attestation independently from structural WIT type compatibility, rejects every
callable or unknown import, and installs no WASI linker. One configured engine
is reused, while every lowerer or verifier call receives a fresh bounded store.
Only a result admitted by the pure canonical/schema/envelope validator becomes a
sealed outcome. Replay invokes identical authority twice through distinct stores
and compares either the complete sealed outcomes or stable host-failure
identities. Wasmtime types remain private implementation details.

### `xtask`

`xtask` is the repository contract harness. Current commands include:

- `cargo xtask verify`;
- `cargo xtask contract-check`;
- `cargo xtask core-goldens --check/--write`;
- `cargo xtask target-ir-goldens --check/--write`;
- `cargo xtask bundle-goldens --check/--write`;
- `cargo xtask cli-goldens --check/--write`;
- `cargo xtask provider-component-fixtures --check/--write`;
- `cargo xtask provider-runtime-dependencies`;
- `cargo xtask release-prep <version>`.

`verify` runs formatting, clippy, workspace tests, doctests, golden checks, topic
contract checks, and diff hygiene. `contract-check` validates topic-shelf tables,
evidence references, and local links. Golden commands regenerate or check the
reviewed byte/digest/stream fixtures. `release-prep` writes mechanical release
scaffolding but does not decide scope or create GitHub state.

The workspace also owns a parser-checked external provider WIT transport
contract under `docs/abi/`. `xtask` verifies its resolved package and world
graph. `amblicit-syntax` mirrors its values for pure invocation validation, while
the private Wasmtime host performs typed external lowerer and verifier dispatch.

The `xtask` implementation is split by responsibility:

- `main.rs` owns command dispatch and the `verify` sequence;
- `contract_check.rs` owns topic-shelf, evidence, and local-link validation;
- `goldens.rs` owns Core, Target IR, bundle, and CLI golden check/write paths;
- `provider_components.rs` owns checked component fixture generation and digest
  inventory verification;
- `provider_dependencies.rs` owns the pinned Wasmtime dependency and feature
  boundary check;
- `release_prep.rs` owns mechanical release-prep scaffolding;
- `util.rs` owns repository walking, command execution, and git-base helpers;
- `tests.rs` keeps the contract-harness regression tests out of dispatch code.

This split is structural only; command behavior remains guarded by
`cargo test -p xtask` and `cargo xtask verify`.

## Layer Flow

The language and artifact flow is:

```text
source text
  -> token / parser / ast
  -> semantic surface validation
  -> compiler context facts
  -> compiler spine
  -> Core IR (including typed external-action request data)
  -> canonical Core bytes and digest
  -> lowerability + target facts
  -> direct lowering or built-in lowerer compatibility adapter
  -> Target IR artifact
  -> canonical Target IR bytes and digest
  -> contract-bundle assembly and validation
  -> admission-boundary request/receipt validation
```

The CLI currently exercises only the front end through surface validation. Tests
and `xtask` exercise deeper layers directly.

The external-provider lane accepts an already selected manifest component and
resolver-supplied bytes, verifies digest and exact contract identity, invokes the
typed lowerer or verifier in a fresh capability-denied store, and stops at a
sealed result admitted by the immutable concrete schema registry and pure
response validator. It can replay the identical invocation through two fresh
stores and return a sealed equal observation or structured mismatch. Package
resolution and target runtime execution remain outside this lane.

## Dependency Rules

Use these rules when placing new code:

- Language model, validation, compiler, canonicalization, target, bundle, and
  admission-boundary behavior belongs in `amblicit-syntax` until the crate-scope
  decision changes the package layout.
- CLI stream parsing, process exit codes, stdin/path trust boundaries, and JSONL
  record production belong in `amblicit-cli`.
- Repository checks, golden regeneration, release scaffolding, workflow guards,
  and topic-shelf validation belong in `xtask`.
- `amblicit-syntax` must not depend on `amblicit-cli` or `xtask`.
- `amblicit-provider-schema` may depend on `amblicit-syntax`; `amblicit-syntax` must not
  depend on the concrete registry.
- `amblicit-provider-host-wasmtime` may depend on the schema registry and syntax
  contracts. Neither dependency may depend on the host, and Wasmtime types must
  not cross its public boundary.
- `amblicit-cli` may depend on `amblicit-syntax`; it should not duplicate language
  semantics that the library already owns.
- `xtask` may depend on `amblicit-syntax` and may inspect repository files, but its
  checks must remain deterministic and avoid live GitHub state unless a workflow
  explicitly owns that boundary.

## Current Non-Claims

This workspace does not yet implement:

- target runtime execution;
- external-action request admission, adapter execution, or settlement
  resumption;
- participant admission execution;
- participant policy evaluation;
- trusted lawpack or target-profile authorship;
- manifest-backed provider resolution, discovery, fetching, or mutable cache
  lookup;
- an out-of-process boundary for native Wasmtime or trusted-host faults;
- a browser-compatible provider component host;
- general target plugin dispatch;
- canonical `ContractBundleManifest` bytes;
- crates.io publication.

Those boundaries are intentionally not hidden in the crate map. They are current
non-claims and should stay explicit in release notes, topic shelves, and pull
request bodies until the owning behavior lands.
