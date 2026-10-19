# Public Rust API

Status: current HEAD contract.

The `flyingrobots-edict` package exposes the Rust library name `edict` as a
curated facade over Edict's implementation crates. The facade is the supported
Rust entry point for source checking, stable diagnostic kinds, and canonical
artifact identity operations. It does not expose the implementation crate's
module tree as an accidental public API.

The package remains `publish = false`. This topic defines a reversible release-
engineering boundary; it does not authorize or claim crates.io publication.

The `edict` CLI remains the stable process boundary for complete application
builds. The Rust facade does not duplicate the CLI's JSONL protocol, provider
host, filesystem publication, or application-build orchestration.

Release preparation advances the facade package version, its exact
`edict-syntax` requirement, and both lockfile package entries together. The
prepared workspace remains resolvable with offline, locked Cargo metadata.
[PUBRUST-REQ-004]

The artifact namespace exports the value models needed to construct Core,
Target IR, and result-projection inputs and to inspect decoded canonical values
and verified projections. Diagnostic spans are available under `diagnostic`.
These are explicit type exports; implementation modules remain private to the
facade boundary. [PUBRUST-REQ-001]

The `numeric` namespace exposes the checked Bunny Q32.32 foundation described
in its [owning shelf](../numeric-foundation/README.md). It evaluates raw values
without exposing saturating traits or claiming fixed-point source/IR support.
[PUBRUST-REQ-005]

## Decision relationships

This current public-API boundary depends on the contracts implemented by its
explicit exports. It adds a curated entry point; the implementation crate
continues to serve repository consumers.

| Relationship | Targets |
| --- | --- |
| `refines` | none |
| `supersedes` | none |
| `depends_on` | [Syntax](../syntax/README.md), [Semantic validation](../semantic-validation/README.md), [Core IR](../core-ir/README.md), [Target IR](../target-ir/README.md), [Result projections](../result-projections/README.md), [Numeric foundation](../numeric-foundation/README.md) |
| `related` | [Rust standards](../rust-standards/README.md), [Release process](../release-process/README.md), [CLI](../cli/README.md) |

The relationship table follows the
[durable-decision policy](../documentation/README.md#durable-decision-discipline).
Packaging and registry-publication work remain explicitly planned in the
[test plan](./test-plan.md); they are not claims of current publication.
