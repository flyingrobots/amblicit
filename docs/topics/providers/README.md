# Providers Topic

Status: current HEAD contract.

This shelf describes the provider manifest boundary, built-in lowerer
compatibility seam, external component transport ABI, pure invocation-envelope
validator, concrete schema registry, and capability-denied component host that
exist today, including deterministic replay and cross-invocation isolation. A
target provider package is an assembled collection of generated artifacts plus
provider-owned components. Edict can validate provider manifests and invocation
values, invoke the existing in-tree Echo and git-warp lowerers through an
explicit migration adapter, and execute resolver-supplied lowerer or verifier
component bytes through the frozen WIT contract. It does not resolve provider
packages or interpret runtime-specific semantics.

## Current Contract

The `amblicit_syntax` crate exposes:

- `TargetProviderManifest`;
- `ProviderArtifactRef`;
- `ProviderArtifactKind`;
- `ProviderArtifactSource`;
- `ProviderSchemaBinding` and `ProviderSchemaFormat`;
- `validate_target_provider_manifest`;
- `bind_target_provider_manifest` and its opaque validated proof;
- `select_provider_component` and its opaque selected component identity;
- `ProviderManifestValidationStatus`;
- `ProviderManifestValidationFailureKind`;
- `BuiltinTargetLowerer`;
- `BuiltinLowererRequest`;
- `BuiltinLowererCompatibilityFailure`;
- `BuiltinLowererCompatibilityFailureKind`;
- `BuiltinLoweringResult`;
- `lower_with_builtin_lowerer`.

The `provider_invocation` module mirrors the WIT request and result values and
exposes:

- `ProviderLoweringInvocationContract` and
  `ProviderVerificationInvocationContract` for host-authored input bindings;
- `ProviderArtifactSchemaValidator` for an explicitly injected, deterministic
  owning-schema capability that is safe to share across concurrent stores;
- `ProviderLoweringRequest` and `ProviderVerificationRequest` plus opaque
  validated request wrappers;
- `validate_provider_lowering_request` and
  `validate_provider_verification_request`;
- `validate_provider_lowering_result` and
  `validate_provider_verification_result`;
- pairwise lowerer and verifier limit-independence validators;
- structured validation reports and stable failure kinds; and
- `ValidatedProviderOutcome` plus `ProviderOutputManifest` for results that have
  crossed the complete validation boundary.

The private `amblicit-provider-schema` crate supplies the production
`ProviderArtifactSchemaValidator`. Its constructor accepts only an opaque
validated manifest, explicit in-memory schema bytes keyed by manifest role, and
the host-authored required-domain closure. It verifies raw schema digests,
rejects missing, duplicate, or manifest-unbound resolved roles, compiles all
bound CDDL documents, rejects missing roots and unresolved external rule
references reachable from selected roots, and returns one immutable registry
with a sorted binding receipt.
Selected roots use a deliberately conservative non-generic, structurally
guarded CDDL subset. A recursive rule back-edge is accepted only after schema
evaluation has descended into a proper map key/value or array element; direct
or mutual aliases and choice-only cycles therefore remain invalid. Structurally
unusable roots reject during construction, and every repeated array or map
member must still be provably input-consuming. Before component execution,
`amblicit-provider-schema` audits recursive roots for structural progress and
validates canonical values under the exact nesting gate, preventing
zero-progress validation independently of any external execution budget. When
competing alternatives can re-enter recursive validation, every alternative
must expose one common,
required, singleton text discriminator with pairwise-distinct values. Same-shape
recursive arrays, missing or non-singleton discriminators, and overlapping
discriminator values reject before a registry exists. Recursive optional or
repeated members also route through specialization so the native validator
cannot retry one recursive child through multiple cardinality assignments.
Construction admits a specialized root only when every reachable shape is in
the specializer's exact subset: maps have either pairwise-distinct literal keys
or one bounded scalar key predicate, and arrays have only fixed members or one
final variable member. Unsupported assignment shapes reject at construction
rather than silently narrowing the admitted CDDL language.

A choice of tagged families may first select the family, then apply its own
required-key dispatch (for example, ordered versus phased Target IR, followed
by closed versus closure-free shape). A family exposes a literal tag only when
every alternative requires the same key/value pair. Discriminator discovery
memoizes rule/depth results and stops at the validation depth boundary, so
shared schema rule graphs do not expand into an exponential path walk.
[PROVIDERS-REQ-031]

For a specialized root, Edict looks up each required discriminator directly in
the current map before any whole-value walk, without depending on CDDL
declaration order or encoded map-entry order. Missing, duplicate, unknown, or
mismatching dispatch therefore rejects without recursively visiting an
unselected child. Map-key predicates are checked as finite scalar rules by the
exact pinned native validator, preserving controls such as `.regexp`; only an
admitted key is literalized into the ephemeral rule. The specialized rule must
contain no remaining rule reference. The selected path and then the complete
value cross Edict's exact 50-container nesting gate, canonical encoding rejects
duplicate keys, and only then does exact `cddl-cat 0.7.1` validation begin.
For a fixed admitted schema, specialization is input-linear over the explicit
value and has no private content-dependent validity cutoff. The registry
performs no schema discovery or lazy loading.

The same crate exposes deterministic `ProviderContractPack` assembly for
runtime-owned generators. Callers supply the common, Core, lawpack,
target-profile, authority-facts, and Target IR CDDL fragments plus all five
validated target-profile contract resources as explicit bytes. Assembly checks
UTF-8, CDDL compilation, complete root closure, exact resource bytes, raw and
domain-framed digests, and reviewed provenance before returning any authority.
Controls whose nested rule graph is opaque through the schema dependency, such
as `.cbor`, reject with a stable unsupported-control failure rather than being
accepted without a complete closure proof.
The checked Rust-neutral artifacts live under
[`fixtures/provider-contracts/structured-control-v1/`](../../../fixtures/provider-contracts/structured-control-v1/):
one self-contained Apache-2.0 CDDL file and one deterministic manifest mapping
logical contracts and provider artifact domains to exact roots. The prior
[function-free v1 publication](../../../fixtures/provider-contracts/v1/),
[source-function publication](../../../fixtures/provider-contracts/source-functions-v1/)
and [source-module publication](../../../fixtures/provider-contracts/source-modules-v1/)
remain byte-identical for explicitly pinned consumers; tests reassemble them
from the frozen pre-v3 Target IR schema in
`fixtures/target-ir/pre-structured-target-ir.cddl`. The structured-control
publication adds the `echo.span-ir/v3` variant of the `target-ir-artifact`
root, with recursive `for` and `branch` order entries discriminated by their
literal `kind`; it accepts actual v3 encoder output and refuses missing or
downgraded orders. Schema acceptance is not provider or runtime support. The
source-module publication adds the `source-closure` root and `edict.source-closure/v1` domain.
It validates the closed envelope and typed module digests, including actual
source-closure encoder output; it refuses a legacy source byte string under
that domain. Import pins, root membership, reachability, topology and aggregate
byte/depth limits remain separate semantic checks of the source-closure API.

Assembly selects that extension only when `source_closure_cddl` is explicitly
supplied. Omitting it reproduces the prior source-function publication exactly.
Consumers select exact schema/manifest bytes; schema selection does not grant
executable backend support or select a new provider automatically. Neither
assembly nor validation discovers repository files, registries, or networks;
the `xtask` command is the explicit repository adapter that reads source files
and writes or checks the reviewed artifacts. Pack instance validation is
generation-time conformance evidence, not a substitute for the production
manifest-bound schema registry. The trusted Edict pack's productive recursive
Core rules satisfy the same guarded structural-descent policy applied to every
provider schema. [PROVIDERS-REQ-030, PROVIDERS-REQ-031]

The private `amblicit-provider-host-wasmtime` crate supplies the component host.
Its input is a selected component proof plus resolver-supplied bytes; it performs
no discovery, fetching, mutable-name lookup, or cache lookup. Preparation
recomputes the selected manifest digest before decoding, requires exactly one
matching top-level digest-covered `edict:target-provider-contract` section,
checks the exact export closure and frozen generated WIT types, and rejects every
callable or unknown import. The generated component's exact type-only
`edict:target-provider/protocol@1.0.0` instance import is accepted because it
conveys types rather than a host capability. No WASI linker or callable host
function is installed.

Source `use capability` declarations establish digest-locked module-import
closure resources. Core external-action request nodes carry the canonical
request fields. Both pass through Core and Target IR without becoming component
imports, host functions, or provider-owned execution authority.

The host owns one immutable Wasmtime engine and creates a fresh store for every
invocation. The prepared component, opaque validated request proof, and concrete
schema registry must share one manifest and validator authority, and a prepared
component can be invoked only by the engine that created it. The store receives
only explicit fuel and resource limits. A result crosses the boundary only after
typed lifting, host diagnostic/output limits, canonical decoding,
schema-instance validation, exact response-envelope validation, and host digest
construction all succeed. A typed provider refusal remains provider evidence;
engine, transport, containment, and admission failures remain stable host-owned
failure kinds.

`replay_lowerer` and `replay_verifier` execute the identical prepared component,
opaque validated request, concrete schema authority, and invocation limits
twice. Each execution receives a distinct fresh store. Completed observations
compare the complete sealed outcome, including every provider-authored artifact
byte and diagnostic. Rejected observations compare only stable host identity:
failure kind, phase, and any structured pure-validation report. Bounded opaque
Wasmtime diagnostics are deliberately excluded. A completed/rejected
disposition change, completed-outcome change, or stable-failure-identity change
returns its own replay mismatch kind and exposes neither run as authoritative.

Provider manifests use API version `edict.provider-manifest/v1`.
This unreleased alpha contract is completed in place by the provider-host slice:
host-ready v1 manifests require exact `providerAbi` and `schemaBindings` fields.
There is no legacy hostable v1 shape with omitted authority bindings.

The manifest validator checks that:

- the manifest API version is current;
- the provider reference is digest-locked;
- `providerAbi` is exactly `edict:target-provider@1.0.0`;
- at least one artifact or component is present;
- artifact role slots are non-empty and unique;
- every artifact resource is digest-locked;
- generated metadata artifacts carry digest-locked semantic-source and
  generator provenance;
- generation-provenance documents use the generic `generationProvenance`
  artifact kind and remain generated metadata rather than executable
  components;
- lowerer and verifier artifacts carry digest-locked component provenance;
- component provenance equals the artifact resource identity rather than
  naming an independently selectable component;
- generated metadata roles are not represented as executable components;
- component roles are not represented as generated metadata;
- artifact schemas are generated artifacts with digest-locked provenance; and
- schema bindings are nonempty, sorted by exact domain bytes, unique by domain,
  and name an existing artifact-schema role plus a nonempty CDDL root rule.

After manifest validation, component selection is explicit by unique role and
expected invocation kind. The exact provider ABI and artifact kind determine
the frozen lowerer or verifier contract; the selected artifact resource is the
authorized component digest. The selected identity borrows the manifest, not
the temporary validation-proof handle. Selection performs no file, cache,
registry, or network lookup.

Digest review strings on this boundary are strict artifact references:
`sha256:<64 lowercase hex>`.

The compatibility adapter requires the caller to select either the Echo or
git-warp built-in lowerer explicitly and pass an already-built `CoreModule` plus
`TargetIrLoweringFacts`. A selected lowerer whose target-profile coordinate does
not match the facts rejects with `TargetProfileMismatch`. Once the profile
matches, the adapter returns the existing `TargetLoweringReport` unchanged,
including structured target refusal. Direct and adapter paths produce identical
Target IR artifacts, canonical bytes, and digests. The adapter does not consume
or resolve a `TargetProviderManifest`, invent component identity, or define the
external provider ABI. Its structured compatibility failure implements
`Display` and `std::error::Error` for standard Rust error propagation.

The external component contract is
[`edict:target-provider@1.0.0`](../../abi/edict-target-provider.wit). It replaces
the previously shipped but unhosted `edict:target-profile@1.0.0` WIT direction
with a distinct ABI identity rather than assigning incompatible meanings to the
old package version.

The `lowerer` and `verifier` worlds each expose one function over an explicit
versioned request. Every request carries the semantic protocol version. Inputs
are opaque canonical artifacts wrapped in digest-locked resource references.
Semantic inputs carry generic roles and kinds; requested outputs carry an
expected role, kind, and domain. Lowering and verification use distinct output
types so neither world can claim the other's evidence roles. Deterministic
response limits cross the ABI. The component host separately enforces
WIT-logical input bytes, the request-authorized logical output ceiling, provider
diagnostic bytes, Wasmtime guest-to-host lifting fuel, linear memory, tables,
instances, memories, Wasm fuel, and bounded engine diagnostics. Component
outputs carry role-tagged bytes and an optional logical path, but no digest. The
pure validator checks canonicality and logical-path syntax and computes
authoritative output identity before the host exposes it. Typed provider refusal
remains distinct from denied imports, traps, exhaustion, malformed lifting,
invalid envelopes, and replay mismatch.

Semantic inputs, requested outputs, and returned outputs use non-empty, unique
roles in strict ascending UTF-8 byte order. A success matches every requested
`(role, kind, domain)` exactly once and returns no undeclared output. Present
logical paths are unique by exact case-sensitive UTF-8 bytes. Diagnostic lists
use the WIT-declared lexicographic order. Response limits apply to success and
refusal: the aggregate byte cap is the checked sum of every provider-authored
byte list and UTF-8 string in the selected result arm. Overflow or any exceeded
cap is a host-owned limit failure. Limits cannot change the canonical result;
when it fits two limit sets, the entire result arm is byte-identical.

## Pure Invocation Validation

The request validators accept only semantic protocol version `1.0.0`. A caller
must supply a complete host-authored invocation contract and a
`ProviderArtifactSchemaValidator` separately from the WIT-shaped request. That
trusted capability's contract requires deterministic, side-effect-free
validation over in-memory values. The concrete registry now satisfies this
contract with compiled, construction-checked CDDL over canonical CBOR.
Validation requires the request resource references, domains, semantic-input
closure, canonical bytes, and decoded values under their owning schemas to
reproduce that contract before it returns an opaque
`ValidatedProviderLoweringRequest` or
`ValidatedProviderVerificationRequest`. The wrapper privately retains the
schema validator for result validation.

Core, target-profile, lawpack, authority-fact, and verifier Target IR inputs use
the fixed domains `edict.core.module/v1`, `edict.target-profile/v1`,
`edict.lawpack/v1`, `edict.authority-facts/v1`, and
`edict.target-ir.artifact/v1`. The host-authored contract supplies the domains
for lowerability facts and auxiliary inputs. Artifact identity is SHA-256 over
the canonical-CBOR frame:

```text
["edict.digest/v1", "<artifact-domain>", <decoded canonical artifact>]
```

The shared canonical decoder rejects malformed or noncanonical CBOR and pins a
maximum nesting depth of 128. After decoding, the explicit host validator must
accept the value as an instance of the immutable schema named by its domain.
Unsupported domains and schema mismatches remain distinct structured host
failures. Edict does not implement Echo, lawpack, target-profile, generated
artifact, review, or verifier-report semantics inside this generic envelope
module; the host-supplied validator owns those schema checks. The trait contract
requires total, deterministic, side-effect-free, in-memory behavior. The
generic boundary cannot enforce the effects of an arbitrary implementation;
the concrete registry supplies executable construction and instance-validation
evidence for the production path.

Result validation preserves the separate lowerer and verifier output
vocabularies. It enforces exact requested outputs, role and diagnostic order,
logical paths, canonical bytes, owning-schema compatibility, and the checked
success/refusal limits. Requested output domains are preflighted before a
validated request can authorize invocation. A valid provider refusal remains
target-owned refusal evidence rather than becoming a host failure. The pairwise
validators compare otherwise-identical requests with different sufficient
limits and reject a changed complete result.

A fully valid success yields a host-authored `ProviderOutputManifest`. It binds
the invocation kind and protocol version, validated inputs, requested outputs,
and host-computed output digests. Limits and diagnostics remain validation and
review data outside that authoritative manifest. An invalid success yields no
partial manifest, and a valid refusal yields no output manifest.

## Authority Boundary

Providers own runtime-specific semantics. The manifest validator checks the
provider envelope and digest-locked reference syntax without receiving artifact
bytes. The invocation validator receives opaque bytes and proves their
canonicality, declared domains, owning-schema compatibility through an explicit
trusted host validator, digest bindings, ordering, and envelope contracts. It
does not prove the runtime-specific semantic meaning or correctness of those
bytes. The built-in compatibility adapter delegates to the existing target
lowerers without adding semantic interpretation.

For example, an Echo provider may supply an Echo lawpack, Echo target profile,
Echo authority facts, Echo lowerer, and Echo verifier. Edict can validate that
those artifacts are explicitly identified and digest-locked. Edict does not
decide whether Echo graph DPO semantics are correct.

## Fixtures

The provider manifest fixture is:

```text
fixtures/providers/echo-generated/provider-manifest.json
```

The fixture uses Echo-shaped coordinates to exercise the provider envelope. It
is not an Echo semantics claim and does not imply Echo provider loading.

Invocation validation reuses the reviewed canonical artifacts:

```text
fixtures/core/canonical/bounded-hello.core.cbor
fixtures/target-ir/canonical/echo-effectful.target-ir.cbor
```

`crates/amblicit-syntax/tests/provider_invocation.rs` binds those bytes into typed
lowering and verification requests, then exercises accepted envelopes and
structured negative mutations without loading a component.

The external host fixture corpus is checked in under:

```text
fixtures/providers/components/lowerer.component.wasm
fixtures/providers/components/verifier.component.wasm
fixtures/providers/components/malformed-lowerer.component.wasm
fixtures/providers/components/instantiation-failure-lowerer.component.wasm
fixtures/providers/components/instantiation-fuel-lowerer.component.wasm
fixtures/providers/components/inventory.json
```

The Rust guests and hand-authored canonical-ABI WAT modules remain beside those
artifacts as reviewed sources. `cargo xtask provider-component-fixtures --write`
rebuilds them with Rust 1.94.0 and `--locked --offline`; check mode verifies the
source digest and every component digest without requiring a Wasm toolchain.
Host tests cover conforming lowerer/verifier execution, refusal, denied callable
imports, infinite work, memory pressure, response and diagnostic flooding,
schema-invalid and noncanonical output, domain and role substitution, undeclared
output, path traversal, explicit traps, instantiation failure,
instantiation-time fuel exhaustion, malformed canonical-ABI lifting, replay,
concurrency, failure recovery, and reviewed Target IR parity across independent
processes.

## Deferred

The following are not implemented:

- manifest-backed provider resolution, file discovery, or package loading;
- lawpack generation through Wesley;
- Echo-owned provider implementation;
- target runtime execution;
- admission or registration;
- a browser-compatible component host; and
- out-of-process containment for a native Wasmtime or host implementation fault.

The current host has no compiled-component cache. Repeated preparation proves
the cache-free path retains the same observation. The modeled provider failure
classes remain contained and leave the compiler test process usable, but the
native Wasmtime host runs in-process and does not claim operating-system process
isolation from a bug in the engine or trusted host itself.

The verification matrix is tracked in [test-plan.md](./test-plan.md).
The enforced component authority flow and limit units are detailed in
[architecture.md](./architecture.md).
