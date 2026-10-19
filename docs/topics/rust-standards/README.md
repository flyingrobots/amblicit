# Rust Standards Topic

Status: current workflow contract.

Edict's Rust standard protects claim integrity: compiler and validation code
must make deterministic, inspectable claims through structured APIs and
executable evidence. Strictness is useful only when it keeps public behavior,
release artifacts, and documentation from drifting apart.

## Current Gates

The workspace forbids unsafe code, denies Clippy `all` and `pedantic` lints, and
denies missing `Debug` implementations. The local gate is still:

```text
cargo xtask verify
```

That gate runs formatting, Clippy with warnings as errors, workspace tests,
workspace doctests, Core golden checks, topic contract checks, and whitespace
checks. [RUST-REQ-001]

Every workspace package inherits the minimum supported Rust version `1.96`, so
published Cargo metadata and local resolver behavior agree with the workspace
policy. The pinned Bunny 0.6.0 numeric dependency requires this minimum. CI runs
the complete format, Clippy, test, and provider-component fixture matrix on exact Rust `1.96.0` and on stable; an xtask guard checks Cargo
metadata and the CI toolchain declarations together. [RUST-REQ-009]

## Safety

- `unsafe` is forbidden in workspace crates.
- Panics in library code are bugs unless the function is explicitly documented
  as panicking or the panic is an internal proof assertion with no caller input.
- Public APIs return structured errors or validation failures, not diagnostic
  strings.
- Stable error kind enums are part of the contract.
- Tests assert error kinds and structured fields, not prose.

[RUST-REQ-001] [RUST-REQ-003]

## Determinism

Library and compiler paths must not depend on hidden process state. They must not
read the environment, current time, random numbers, network state, current
working directory, or filesystem traversal order unless the function name and
topic shelf explicitly define that I/O boundary.

Use deterministic collections or explicit sorting anywhere order can escape into
public reports, digests, encoded artifacts, fixtures, validation failures, or
test oracles. [RUST-REQ-002]

## Error Design

Public validators should expose failures with machine-usable structure:

```rust
pub struct ExampleFailure {
    pub kind: ExampleFailureKind,
    pub field: String,
    pub coordinate: String,
}
```

Human diagnostic prose may be added later, but callers must not need to parse it
to understand the failure. Adding, removing, or renaming a public failure kind
requires the owning topic shelf, tests, and changelog or release notes when the
change affects a public contract. [RUST-REQ-003]

## Public API

Raw `String` fields are acceptable in parser internals and alpha fixtures. Once
validation rules stabilize, public contract structs should move toward validated
newtypes for coordinates, digests, API versions, package coordinates, artifact
coordinates, and requirement identifiers. [RUST-REQ-003]

## I/O Boundaries

Core compiler and validation paths stay mostly pure. Allowed I/O is limited to
explicit file loaders, `xtask`, the `edict` CLI boundary, and tests. Any public
function that reads files should say so in its name, such as `load_*_file` or
`load_*_from_paths`. Hidden directory discovery, registry fetches, writes,
network calls, and cwd-sensitive behavior do not belong in library compiler
paths. [RUST-REQ-002]

## Serialization

Normative JSON, TOML, and manifest inputs should use exact API versions,
structured unsupported-version failures, and strict fields. Silent defaults are
allowed only for repeated fields where the ABI defines omission as empty.
[RUST-REQ-003]

## Testing

Tests must assert software behavior and stable contract artifacts. Do not test
implementation details, documentation prose, repository shape, incidental
stdout/stderr, or live GitHub state. Workflow tests may inspect checked-in
workflow contracts because release automation is project behavior.

Every public validator should have positive and negative tests. Every public
error kind should be produced by at least one deterministic test before it is
treated as implemented. [RUST-REQ-004]

## Documentation Impact

Every contract-bearing change includes one of:

- affected documentation updated;
- affected documentation verified unchanged;
- `docs-impact: none` with rationale.

Topic `README.md` files describe HEAD truth only. Future work belongs in test
plans, roadmap entries, design notes, or issues. [RUST-REQ-005]

## Dependency Policy

New dependencies require PR-body rationale. Runtime/network/plugin dependencies
in library compiler crates require architecture review. Dependency updates must
state contract impact.

CI pins cargo-deny 0.18.9 and checks both the root workspace lockfile and the
provider fixture guest lockfile. `deny.toml` rejects RustSec advisories, yanked
crates, and unknown registry or Git sources; allowlists reviewed licenses; and
reports duplicate versions. The equivalent local gates are:

```text
cargo +stable deny --locked check
cargo +stable deny --locked --manifest-path fixtures/providers/components/guests/Cargo.toml check
```

[RUST-REQ-007]

## Generated Artifacts

Generated artifacts must say how they are generated, should not be hand-edited,
and must have check commands. Golden artifacts are reviewed evidence, not a
substitute for executable encoder tests. Regeneration changes must include the
command used and explain why output changed. [RUST-REQ-005]

## Planned Ratchets

The destination policy is to deny `unwrap`, `expect`, `panic`, `todo`,
`unimplemented`, debug macros, and direct stdout/stderr in library code, with
local allowances for tests and `xtask` where loud fixture failure or tool output
is appropriate. This is not yet globally enforced because tests and tool code
still need scoped allowances. [RUST-REQ-006]

Parser, lexer, decoder, and authority-facts fuzz targets are planned as the
language surface grows. Fuzzing proves no panic or resource explosion; it does
not replace structured semantic tests. [RUST-REQ-008]

## Review Checklist

For behavior pull requests:

- identify the owning topic shelf;
- add or update a test-plan row;
- record the RED command;
- record the GREEN command;
- use stable error kinds where callers need to depend on failure;
- update docs or state `docs-impact: none`;
- run `cargo xtask verify`;
- make no public claim without executable evidence.

[RUST-REQ-004] [RUST-REQ-005]
