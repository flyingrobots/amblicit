# Lawpacks Topic

Status: current HEAD contract.

This shelf describes the lawpack boundary that exists today. A lawpack is an
external, digest-locked source of portable Edict semantics. Edict can parse
lawpack imports, carry lawpack references through bundle and lowerability
contracts, validate the direct declarative adapter ABI, load first compiler
context facts from authority-facts documents whose source kind is `lawpack`,
and decode exact canonical lawpack manifests and export surfaces into an opaque
validated bundle. Complete supplied dependency sets are checked for exact
digest binding, missing dependencies, and cycles before use.

Provider manifests can describe lawpacks as generated provider artifacts with
digest-locked semantic-source and generator provenance. This validates only the
provider envelope and provenance lock; it does not load or interpret the
lawpack manifest. [LAWPACKS-REQ-007]

## Public Surface

The source syntax accepts lawpack imports of the form:

```text
use lawpack hello.optics@1 digest "sha256:<64 lowercase hex>" as hello;
```

The parser preserves the import as a lawpack import with the package coordinate,
version label, alias, and digest review string. [LAWPACKS-REQ-001]

The import digest identifies the canonical manifest in the `edict.lawpack/v1`
domain. It is SHA-256 of the canonical CBOR tuple
`["edict.digest/v1", "edict.lawpack/v1", manifest]`, where `manifest` is the decoded
canonical value. It is not SHA-256 of the raw `manifest.cbor` file. Copy the
review string from `manifest.sha256` beside a generated lawpack; the public
`ValidatedLawpackBundle::manifest_digest_review_string` API returns the same
identity. A mismatch retains `SourceImportMismatch` and the import digest path,
and its obligation identifies the required domain and exact digest.
[LAWPACKS-REQ-026]

For the shipped portable capability, run this command from the repository root
to print a source import with the matching generated manifest identity:

```sh
printf 'use lawpack causal.cell@1 digest "%s" as cells;\n' "$(cat fixtures/lawpack/causal-cell/manifest.sha256)"
```

The output is one `use lawpack` declaration with the sidecar's full `sha256:`
review string. Regenerate the closure through its owned generator when changing
its manifest or adapter; copy the resulting manifest digest into the source.
An exports, adapter, or target-configuration sidecar identifies that artifact,
and cannot replace the manifest digest in a source import.

The machine-readable lawpack manifest and export surface are specified in
[`docs/abi/edict-lawpack.cddl`](../../abi/edict-lawpack.cddl), with explanatory
reference material in
[`docs/SPEC_edict-lawpack-abi-v1.md`](../../SPEC_edict-lawpack-abi-v1.md).
`decode_lawpack_bundle` enforces the closed canonical shape and local semantic
obligations. `validate_lawpack_dependency_graph` validates the complete supplied
dependency set independent of input order. [LAWPACKS-REQ-005]

The current executable Rust surfaces touching lawpacks are:

- parser support for `ImportKind::Lawpack`;
- typed application-owned authoring through `author_lawpack` and the public
  JSONL `build` route, documented by the
  [lawpack-authoring guide](../lawpack-authoring/README.md);
- canonical manifest/export loading through `ValidatedLawpackBundle`;
- complete dependency-set validation with exact manifest-digest edges;
- canonical direct-adapter loading with exact target selection, adapter digest
  corroboration, complete callable profile/effect/budget coverage, and
  effect-free profiles whose exact budget and target configuration confer no
  callable effect authority;
- compiler and Target IR fact derivation from the exact
  module/lawpack/adapter closure;
- compiler projection of exported pure-helper signatures and numeric loop-bound
  constants under canonical export identities;
- reproducible canonical Core and Target IR artifacts for the standalone Hello
  Echo crossing;
- reproducible request-only workspace-snapshot closure and public application
  build artifacts with one external request and zero callable Target IR steps;
- authority-facts loading for budget and effect write-class facts whose source
  identity is a digest-locked lawpack reference;
- target-profile validation for the exact `edict.lawpack-adapter/v1` ABI;
- lowerability checks for digest-locked, one-hop direct adapter support;
- contract-bundle manifest validation that can carry lawpack artifact
  references as participant-neutral resources.
- provider manifest validation that can carry a generated lawpack artifact
  reference and provenance without interpreting lawpack semantics.

## Current Contract

- Lawpack source imports require lexically valid digest review strings when a
  digest is present. Invalid digest strings reject at the parser boundary.
  [LAWPACKS-REQ-001]
- Canonical manifests and export surfaces reject non-canonical bytes, missing
  or unknown fields, malformed typed digests, substituted exports, invalid
  discriminants, duplicate identities, unmappable failure names, opaque Edict
  helper bodies, signature-incompatible parameters, locals, bindings, calls or
  results, recursive pure-helper call graphs, unbounded executable components,
  and runtime effects with no target-adapter descriptor. The successful wrapper
  exposes typed values but cannot be fabricated or mutated by callers.
  [LAWPACKS-REQ-005] [LAWPACKS-REQ-014]
- Dependency validation resolves the complete supplied set by `(id, version)`,
  detects cycles independent of input ordering, then corroborates every edge
  against the exact resolved manifest digest. Public application builds
  additionally require every supplied lawpack to be reachable from the
  manifest's first/root lawpack. [LAWPACKS-REQ-005]
- v1 target profiles accept the exact `edict.lawpack-adapter/v1` identifier.
  Unknown and duplicate declarations reject. [LAWPACKS-REQ-003]
- `decode_lawpack_adapter` accepts only canonical adapter bytes selected by one
  exact digest-locked target descriptor. It requires exact operation-profile,
  runtime-effect, budget, footprint, cost, and named-failure coverage before
  returning an opaque validated adapter. Each callable effect carries one
  typed, digest-locked target-configuration reference. A profile with no
  semantic effects may govern a pure executable program or a request-only
  program and must carry its own exact budget obligation and target
  configuration. Application provider inputs select that profile-owned
  configuration even when the adapter has no effects. For an effectful profile,
  application configuration selection includes only effect coordinates present
  in compiled Core whenever at least one is invoked, so another advertised effect
  cannot introduce an unused target configuration. If the selected effectful
  profile invokes none, its full advertised effect set must instead converge on
  one exact target configuration; an ambiguous set rejects. Compilation
  preserves the profile-to-budget association and rejects source that selects
  another profile's budget. Direct-adapter closure also requires every authored
  profile to map to a distinct Core profile, preventing source-profile
  configuration authority from collapsing after compilation erases the source
  coordinate. Application configuration selection resolves that exact profile
  for each compiled intent and verifies that every invoked semantic effect is
  advertised by it before applying the zero-invocation fallback. An effect
  advertised only by another profile cannot contribute an intrinsic or target
  configuration. The exact adapter budget set also covers every exported pure
  helper's cost template. Edict
  preserves those references but does not interpret their target-owned
  semantics. [LAWPACKS-REQ-019]
  `prepare_lawpack_compilation` then derives compiler and Target IR facts
  through every matching source import's exact alias and manifest digest.
  Linked source modules may import the same lawpack through distinct aliases.
  Every matching import must authenticate against the exact manifest; a valid
  first alias cannot hide a later substituted digest. Canonical pure-helper
  Target IR facts remain unique across aliases.
  [LAWPACKS-REQ-008] [LAWPACKS-REQ-017] [LAWPACKS-REQ-018]
- The same preparation resolves every non-generic semantic effect's input and
  output plus every declared failure payload through its exact exported bounded
  type closure. Imported definitions
  may use bounded scalars, lists, aliases, nominals, and records such as
  `Record<key:String<max=64,canonical=raw-utf8>>`. Source compilation checks the
  argument and result annotation for early diagnostics. A private Target fact
  separately carries the canonical effect export and exact reachable named-type
  map. Inline structural constructors are parsed and traversed but never enter
  that map or claim lawpack ownership,
  so caller-authored Core cannot substitute a value, binding type, or type
  definition while retaining a trusted coordinate. Missing or unresolvable
  signature types refuse during preparation. Each source obstruction binder is
  typed from the exact authenticated failure-payload root; a failure name never
  becomes a synthetic `effect.failure` Core identity. [LAWPACKS-REQ-020]
  [LAWPACKS-REQ-023]
- The same preparation boundary projects each exported pure-helper signature
  through the source alias. Source calls type-check against that signature and
  lower under the canonical exported coordinate, while the exact imported
  manifest digest continues to bind the helper implementation. The corresponding
  Target fact is externally immutable and privately carries the exact named
  closure reachable from every parameter and the return. Target reconstructs
  that closure through the same structural-transparent judgment used for effect
  signatures. Each root first passes the shared expansion-height law; the
  subsequent named-set collection is depth-free, so root order cannot change
  validity or authority. Missing, extra, foreign, or substituted named
  definitions refuse; an empty closure remains valid for signatures built only
  from intrinsic and structural forms. [LAWPACKS-REQ-012] [LAWPACKS-REQ-021]
  [LAWPACKS-REQ-022] [LAWPACKS-REQ-024]
- The preparation boundary also projects exported `U32` and `U64` constants
  through the source alias as numeric bound facts. Static loop checks consume
  the value, while Core preserves the canonical exported coordinate.
  [LAWPACKS-REQ-013]
- Before an inline Edict helper becomes a compiler fact, its closed Core body
  is checked against the exported signature. Parameters, locals, bindings,
  calls, integer domains, bounded strings and bytes, record fields, variant
  cases and payloads, bounded collection entries, and the final result must all
  have the declared types. Helper calls must also form an acyclic graph no
  deeper than 128 helpers on any directed path, including the root and terminal
  helper. This is a graph-height bound, not a limit on the total number of
  exports or call occurrences. Canonical coordinate order, export-array order,
  and shared suffixes do not change acceptance. Iterative validation caches each
  completed helper's longest suffix height and includes that height in every
  caller; cycle detection remains separate. A cycle or a path of 129 helpers
  rejects with `InvalidPureFunctionBody` before compiler facts are exposed.
  [LAWPACKS-REQ-014] [LAWPACKS-REQ-025]
- Byte constants preserve the complete exported interval during that check:

  | Exported byte form | Accepted length |
  | --- | --- |
  | `Bytes<exact=N>` | Exactly `N` bytes |
  | `Bytes<min=M,max=N>` | From `M` through `N`, inclusive |

  [LAWPACKS-REQ-016]
- The Hello Echo golden generator compiles the exact source and lawpack closure,
  lowers the resulting Core module, and pins canonical Core and Target IR bytes
  under their native domain-framed identities. [LAWPACKS-REQ-009]
- The workspace-snapshot generator binds a requestable capability to the exact
  lawpack manifest, compiles one request through a request-only profile, and
  pins canonical Core and Target IR with zero callable steps.
  [LAWPACKS-REQ-011]
- Lowerability may classify an operation as adapted when exactly one
  digest-locked direct adapter satisfies the required semantic effect, write
  class, and guard facts. Floating, chained, or ambiguous adapter claims reject
  with stable failure kinds. [LAWPACKS-REQ-002]
- Contract bundles may reference lawpacks as digest-locked participant-neutral
  artifacts, but validation does not load, rehash, or execute lawpack manifests.
  [LAWPACKS-REQ-004]
- Authority-facts documents may identify a `lawpack` source and provide the
  first compiler facts consumed by `CompilerContext`, such as budgets and effect
  write classes. This is not full lawpack manifest validation.
  [LAWPACKS-REQ-006]
- Provider manifests may identify lawpacks as generated artifacts. The provider
  validator checks lowercase digest locks for the artifact, semantic source, and
  generator, and rejects component provenance for lawpack metadata roles.
  [LAWPACKS-REQ-007]

## Deferred

The following are not implemented:

- executable target-adapter component loading; v1 currently specifies and
  implements the direct declarative adapter class only;
- target-owned configuration interpretation;
- Echo admission or execution of compiler-emitted external-action requests;
- lawpack conformance fixtures and two-lowerer differential trials.

The verification matrix is tracked in [test-plan.md](./test-plan.md).
