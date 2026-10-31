---
title: "SPEC - Edict Language v1"
legend: "SPEC|TRANSMUTE|PLATFORM"
lane: "design"
packet: "0021-continuum-yolo-runtime-neutral-edict-sha-lock-assurance"
issue: "https://github.com/flyingrobots/wesley/issues/611"
status: "draft"
owners:
  - "@flyingrobots"
created: "2026-06-17"
updated: "2026-09-03"
---

<!-- markdownlint-disable MD025 -->

# SPEC - Edict Language v1

<!-- markdownlint-enable MD025 -->

## Linked Issue

- [Issue #611 - runtime-neutral Continuum target profiles](https://github.com/flyingrobots/wesley/issues/611)

## Source Material

This specification distills the Edict language plan from:

- [Design Baseline](./DESIGN_runtime-neutral-edict-sha-lock-assurance.md)
- `~/git/blog/continuum-yolo-edict-runtime-neutral-design.md`
- `~/git/blog/continuum-yolo-edict-shalock-holmes-design.md`
- `~/agy-readings/observer-geometry-overview.md`
- `~/agy-readings/append-only-log-of-comprehension.md`
- `~/agy-readings/aion-overview.md`
- [PR #610](https://github.com/flyingrobots/wesley/pull/610)
- [Issue #611](https://github.com/flyingrobots/wesley/issues/611)

The two blog drafts are byte-identical as of this writing. PR #610 landed the
runtime-neutral packet. Issue #611 is the active implementation tracker.

## Normative Surface Map

This packet now splits the Edict proposal into focused specifications. This
document is only the language and Core IR specification. The adjacent specs own
the boundaries that must not leak back into language meaning:

- [SPEC - Edict Language v1](./SPEC_edict-language-v1.md): source syntax, type
  system, effect rules, Core IR, and language-level canonical value semantics.
  Canonical schemas: `abi/edict-common.cddl`, `abi/edict-core.cddl`.
- [SPEC - Edict Lawpack ABI v1](./SPEC_edict-lawpack-abi-v1.md): lawpack
  manifest and dependency graph, exported types/constants, pure helper and
  semantic effect signatures, proof-only vs runtime-materialized classification,
  typed obstruction payloads, footprint/cost obligations, and target adapters.
- [SPEC - Edict Target Profile ABI v1](./SPEC_edict-target-profile-abi-v1.md):
  intrinsic signatures (pure vs effect), effect signatures, target lowering,
  application model, verifier ABI, footprint algebra, and cost algebra. Canonical
  schemas: `abi/edict-target-profile.cddl`, `abi/edict-target-provider.wit`.
- [SPEC - Continuum Contract Bundle v1](./SPEC_continuum-contract-bundle-v1.md):
  participant-neutral contract bundle identity, artifact graph, provenance
  references, canonical CBOR/hash framing, and attestation roles.
- [SPEC - Continuum Admission v1](./SPEC_continuum-admission-v1.md):
  participant descriptors, policy epochs, admission requests, admission
  receipts, capability receipts, and participant-specific decisions.
- [GUIDE - Edict Assurance And Transparency](./GUIDE_edict-assurance-transparency.md):
  HOLMES, Watson, Moriarty, transparency logs, nutrition labels, profile diffs,
  relapse fuzzing, the hash ladder, the Aperture Ledger, and the two-lowerer
  trial.
- [REQUIREMENTS - Fixture Constitution](./REQUIREMENTS.md): the requirement-ID
  registry binding every normative rule to its owner spec and fixtures.

## Specification Maintenance

This document is Edict's formal language specification. Its language and Core
semantics are normative even while the v1 design status remains `draft`;
`draft` permits reviewed evolution and does not permit implementation behavior
to supersede the written contract silently.

A contract-bearing change to source syntax, static semantics, compiler-visible
language behavior, Core type identity, or Core IR MUST update this specification
in the same pull request. When the wire shape changes, the coupled
`abi/edict-common.cddl` or `abi/edict-core.cddl` schema MUST move with it. The
owning requirement and test-plan evidence MUST also move when their current
claim changes. If an implementation change leaves all of those contracts
unchanged, the pull request may instead record `docs-impact: none` with a
specific rationale. Tests are conformance evidence; they do not become a second
language specification.

## Decision Summary

Edict v1 is a restricted deterministic source language for lawful optics over
witnessed causal history. An Edict intent is an optic-shaped operation
specification: it declares a bounded aperture, typed input and output, imported
law and target authorities, inferred effects, proof obligations, obstruction
mappings, support posture, and target-owned application requirements.

Edict compiles to Edict Core IR, a runtime-neutral canonical optic IR with
explicit target imports and analyzable effects. Edict Core does not contain
graph, commit, SQL, KV, event-log, filesystem, network, clock, random, or host
callback primitives. Runtime target profiles provide target intrinsics,
footprint algebra, target IR lowering, verifier rules, obstruction taxonomy,
and atomic application semantics.

Wesley should compile Edict through a source-profile adapter and canonical IR
pipeline, but Wesley must not become the owner of Continuum participant policy,
Echo DPO, or target admission. The primary Edict language implementation should
eventually live in its own repository, with Wesley consuming it as a compiler
component and with Echo, Continuum, and other runtimes owning their target
profiles.

## Observer Geometry And WARP Optics

Edict inherits its motivating shape from Observer Geometry and WARP optics:

- observation is a structured projection, not a value read;
- every projection has a basis, aperture, accumulation posture, and degeneracy;
- distributed reconciliation is suffix transport across frontier-indexed
  observers under footprint and policy checks;
- evidence has a support ledger: carried, lost, blocked, or refuting;
- path-sensitive evidence loss creates witness debt;
- a WARP optic places a focused aperture over a bounded slice of causal history,
  lowers it under an admissibility law, and retains a holographic boundary
  shell.

Therefore an Edict intent is not only a typed function from input to output. It
is an authored optic candidate:

```text
basis + aperture + law + target profile + support obligations
  -> reading | receipt | proposed suffix | obstruction
```

Edict v1 expresses that optic shape through typed inputs and outputs, bounded
footprints, budgets, imported lawpacks, imported target profiles, atomic guards,
obstruction mappings, canonical Core IR, and bundle evidence. Future syntax may
make basis, aperture, support ledgers, degeneracy posture, and holographic
witness shells more explicit, but v1 must not contradict those concepts.

Intent classes:

- **revelation optic:** read-only or observer intent that projects a bounded
  aperture into a reading without authoring history;
- **affect/reintegration optic:** write intent that proposes effects against a
  basis and carries the guards, support obligations, and obstruction vocabulary
  needed for participant admission;
- **semantic lawpack optic:** storage-neutral intent that lowers through a
  digest-locked lawpack into a concrete target profile.

Target profiles interpret Edict Core optics into runtime-owned execution
categories. A valid interpretation must preserve the Observer Geometry
structure: basis, aperture, projection or affect boundary, footprint
independence, support posture, atomic guards, cost bounds, obstruction classes,
and canonical artifact identity.

## Autonomous Operation Lane Terminology

The formal Edict/Continuum assurance lane for checked autonomous execution is
`lawful-autonomous`. The canonical identifier is:

```text
continuum.lane.lawful-autonomous/v1
```

Some design packets and human-facing materials may use the codename **YOLO**,
expanded as **You Only Lawfully Operate**, to contrast Edict's model with common
agent "YOLO mode" behavior. The codename is non-authoritative. Provenance
records, contract bundles, admission receipts, target profiles, canonical
coordinates, and canonical hashes use the formal lane identifier, not the
codename.

A lawful-autonomous operation does not imply approval bypass, ambient host
authority, raw filesystem or network access, unchecked callbacks, or unsandboxed
execution. It means the operation may run autonomously only after its source,
Core IR, target IR, lawpacks, target profile, verifier reports, generated
artifacts, contract bundle, and participant admission evidence are locked and
accepted by participant policy.

Canonical contract bundle metadata uses formal identifiers only:

```json
{
  "assuranceLane": "continuum.lane.lawful-autonomous/v1"
}
```

Optional human-facing display metadata is stored in a sidecar keyed by the
authoritative digest, not inside the hashed manifest:

```json
{
  "subject": "sha256:<contract-or-profile-digest>",
  "display": {
    "codename": "YOLO",
    "expansion": "You Only Lawfully Operate"
  }
}
```

Codename terms must not appear in hash-significant canonical coordinates. For
example, `continuum.yolo.bundle/v1`, `edict.yolo.profile/v1`, and
`target.yolo.safe/v1` are invalid canonical coordinates; use names such as
`continuum.bundle.lawful-autonomous/v1` or
`edict.assurance.checked-autonomous/v1` instead.

The historical packet slug contains `yolo` because it is a design locator. It
is not a canonical runtime coordinate, target profile identifier, bundle profile
identifier, or hash input, so it does not violate the codename invariant.

## Recommendation

### Primary Home

Create a dedicated Edict repository for the language specification, parser,
AST, Edict Core IR, canonicalization rules, conformance fixtures, and
runtime-neutral compiler tests.

Recommended eventual repository:

```text
flyingrobots/edict
```

### Wesley Role

Wesley should host the current design packet and may temporarily host bootstrap
adapters while the language hardens. Its durable role should be:

- GraphQL SDL to Shape IR.
- `weslaw` to Law IR.
- `graphql-wesley@1` source-profile facts for Edict compilation.
- Source-profile adapter plumbing, canonical hashing, manifests, and generic
  compiler evidence.
- Integration tests proving Edict source can consume Wesley Shape IR and Law IR
  without making Wesley own runtime behavior.

Wesley must not own Echo operation IDs, Echo DPO semantics, Continuum agent
policy, participant registration policy, or target-specific verifier semantics.

### Continuum Role

Continuum should own participant protocol:

- participant descriptors;
- source-profile catalogs;
- lawpack catalogs;
- runtime-target catalogs;
- capability catalogs;
- bundle preflight and registration APIs;
- admission receipts and obstruction classes.

Continuum should not own the Edict compiler as hidden runtime policy. It should
accept SHA-locked Edict bundles and verify them against participant policy.

### Echo Role

Echo should own `echo.dpo@1`:

- Echo target profile manifest;
- Echo target intrinsics;
- Echo graph footprint algebra;
- Echo Span IR lowering;
- Echo DPO verifier;
- Echo operation IDs and registration artifacts.

Echo must not define Edict Core. Making Echo the language home would relapse
into graph-shaped Continuum.

### Why Not Put Edict Primarily In Wesley

Wesley is the GraphQL-to-compiler-evidence substrate. Edict is a new lawful
optic language that may be authored without GraphQL and may target runtimes that
do not use Wesley Shape IR. Putting Edict primarily in Wesley would blur the
domain-empty boundary and pressure Wesley core to own Continuum and runtime
target semantics.

### Why Not Put Edict Primarily In Continuum

Continuum is the participant protocol and lawful self-extension workflow.
Bundling the compiler into Continuum would make participant policy and language
semantics harder to version independently. Edict should be a participant-
discoverable source profile, not the Continuum protocol itself.

### Why Not Put Edict Primarily In Echo

Echo is the first concrete target profile, not the universal storage model.
Putting Edict in Echo would make graph DPO doctrine look like the semantic
core and would contradict the packet rule: no universal store, universal law
shape.

## Language Layers

Edict has four separable layers.

### 1. Edict Source

Human and agent authored syntax. It declares packages, imports, types, pure
functions, intents, assertions, and target intrinsic calls. Migration and
projection syntax is reserved for future versions; v1 expresses migrations as
ordinary intents with a migration profile.

### 2. Edict Core IR

Canonical runtime-neutral IR produced from Edict Source or another accepted
source profile. It records typed declarations, normalized expressions,
effectful operation bodies, imports, law/profile references, and inferred
abstract effects.

### 3. Target IR

Runtime-specific IR produced by a target profile. Examples include Echo
Span IR, git-warp Commit/Reducer IR, KV Transaction/CAS IR, and EventLog
Append/Projection IR.

### 4. Contract Bundle

Participant-neutral SHA-locked release artifact binding source artifact
digests, Core IR digest, target IR digest, lawpack digests, target profile
digest, verifier reports, generated artifact hashes, and compiler evidence
references. Admission requests and receipts are external artifacts that
reference a `bundleSubject` (`{ kind, digest }`); they are not part of either
bundle digest.

## Partial Lowerability

Edict does not guarantee that every Core module lowers to every target profile.
Target lowering is a **partial, semantics-preserving relation**
(`EDICT-LOWERABILITY-PARTIAL-001`).

For a Core operation, target profile, and digest-locked lawpack adapter set,
lowering succeeds only when every required type, pure operation, semantic effect,
guard, obstruction, footprint obligation, cost obligation, atomicity
requirement, optic-preservation requirement, and postcondition is either
natively supported by the target or lawfully discharged by a direct adapter.

A v1 lowering is classified as:

- `native`: the target profile directly realizes the operation;
- `adapted`: one or more lawpack adapters lower semantic effects directly into
  target intrinsics;
- `unsupported`: at least one obligation cannot be discharged.

Unsupported lowering is a compiler/lowering error. The compiler must not silently
approximate semantics, weaken guards, collapse obstruction classes, widen
authority, erase evidence loss, or fall back to ambient host execution. This is a
distinct failure class from admission rejection (a participant decision) and from
a runtime obstruction (a domain result at execution time); see I-012.

General `composite` / chained adapter classification is future v2 design work.
Edict v1 lowerability accepts native target support or exactly one direct
adapter for a semantic effect; unresolved adapter chains remain `unsupported`.

## Threat Model

Edict exists because agents and applications should be able to author lawful
operations without receiving raw runtime authority. The spec must therefore
defend against semantic substitution, supply-chain substitution, and authority
laundering before it defends syntax comfort.

Minimum attack classes:

- target profile substitution;
- stale lawpack downgrade;
- digest mismatch;
- source-profile confusion;
- target-lowering plugin nondeterminism;
- canonicalization ambiguity;
- JSON integer ambiguity;
- FIDLAR laundering through lawpack or native helpers;
- footprint underclaiming;
- closure-read underclaiming;
- read-only claims hiding writes, appends, or audit records;
- obstruction misclassified as compiler or admission error;
- host callback injection;
- plugin dependency confusion;
- replay of an old bundle under new participant policy;
- source-map spoofing;
- generated artifact tampering;
- conformance fixture forgery.

Threat response is layered:

- Core has no storage-shaped nouns.
- All effects are imported effects.
- Canonical values hash through one authoritative typed encoding.
- Target lowerers and verifiers are digest-locked executable components.
- Contract bundles carry source artifact, Core IR, target IR, compiler, lowerer,
  verifier, lawpack, target profile, generated artifact, and assurance evidence
  references.
- Admission requests and receipts carry participant descriptor, catalog
  snapshot, policy digest, policy epoch, requested capabilities, and admitted
  ceilings outside the bundle digests, referencing a `bundleSubject`.
- Read-only, footprint, and cost claims are theorems checked from inferred
  effects and budgets, not trusted labels.

## Canonical Value Model

Grammar is not the authority boundary. Canonical value semantics are. The
compiler must define:

- integer width and signedness;
- enum coordinate encoding;
- option encoding;
- variant tag encoding;
- record field order;
- map key order;
- bytes encoding;
- string normalization policy;
- digest domain separation;
- absent versus null;
- imported type identity;
- source-profile type alias identity.

Edict v1 should separate abstract value semantics from review rendering:

```text
edict.canonical-value/v1  # abstract typed value model
edict.canonical-cbor/v1   # authoritative hash input
edict.canonical-json/v1   # review/debug rendering
```

`edict.canonical-cbor/v1` is the recommended authoritative hash input because
Edict needs typed cryptographic material, deterministic map ordering, explicit
integer widths, bytes, and a way to avoid JSON number ambiguity. JSON may still
be emitted as a canonical review form, but full-width `I64` and `U64` values
must not rely on plain JSON number semantics.

The exact CBOR profile and hash preimage are pinned by
[SPEC - Continuum Contract Bundle v1](./SPEC_continuum-contract-bundle-v1.md).
Language-level canonical values must preserve scalar type in the hash input:
`U32(1)` and `U64(1)` are distinct typed values even if their numeric payload is
the same. Canonical CBOR uses definite lengths, shortest valid integer encodings
inside type-tagged values, duplicate-key rejection, deterministic map ordering
by encoded key bytes, UTF-8 labels without implicit normalization, rejected
floats, rejected unknown tags, and no self-digest field in the artifact
preimage.

Every digest has a domain-separated label:

```text
hash(domain: "edict.core.module/v1", bytes: canonical-cbor(module))
hash(domain: "edict.target-ir/echo.span-ir/v1", bytes: canonical-cbor(target))
hash(domain: "edict.bundle/continuum.contract-bundle/v1", bytes: canonical-cbor(bundle))
```

Conceptually this is framed as:

```text
SHA-256(canonical-cbor([
  "edict.digest/v1",
  "<artifact-domain>",
  <typed artifact value without self digest>
]))
```

Artifact hashing is an internal canonicalization operation over typed canonical
bytes. Source-level `hash(label, value...)` is a deterministic prelude helper
with stricter ergonomics: `label` must be a string literal, not a dynamic value,
and the call lowers to a domain-separated artifact hash over the canonical
encoding of `{ label, values }`.

## Authority Model

Core contains laws of physics, not furniture. Storage-shaped nouns enter the
program only through imports.

All effects are imported effects:

- target effects come from target profile imports;
- semantic effects come from lawpack imports;
- source-profile facts come from source-profile imports;
- pure helper functions come from Core, source, or digest-locked lawpack code.

There are no ambient Core effects. In particular, `record history.foo { ... }`
is not a Core event-store primitive. It is sugar for a lawpack semantic effect:

```amblicit
record history.textRangeReplaced {
  historyId: input.historyId,
};
```

desugars to:

```amblicit
let _ = history.textRangeReplaced.record({
  historyId: input.historyId,
});
```

The resulting Core effect records:

```text
authority: lawpack jedit.structural_history@1 digest sha256:...
intrinsic: jedit.structural_history@1.textRangeReplaced.record
effectKind: semantic.emit
```

If a semantic effect lowers to a durable event append, graph create, KV write,
or other runtime mutation, it is not read-only.

## Effect Evaluation Order

Edict v1 uses A-normal effect form. Effectful calls may appear only as:

```amblicit
let x = importedEffect(...);
importedEffect(...);
```

Effectful calls are rejected inside:

- function arguments;
- conditions;
- record literals;
- list literals;
- return expressions;
- pure helper bodies;
- other effect call arguments.

Rejected v1 example:

```amblicit
let z = hash(foo.read(), bar.create({ id: input.id }));
```

Required v1 form:

```amblicit
let fooValue = foo.read()
  else domain.FooMissing;
let barValue = bar.create({ id: input.id })
  else domain.BarConflict;
let z = hash(fooValue, barValue);
```

This rule keeps Core lowering simple: source lowers to typed, ANF/SSA-shaped
Effect Core with ordered effect nodes, structured branches, explicit guard
nodes, and loop cardinality bounds.

### Conditional Effect Values

A-normal effect form still needs a source construct for optional and conditional
effects. Edict v1 permits effectful branch expressions only in `let` binding
position:

```amblicit
let initialBlob = if len(initialBytes) == 0 {
  yield none<shape.TextBlob>();
} else {
  let blobRef = echo.ref<shape.TextBlob>(rope.textBlobId(initialBytes));
  let blob = blobRef.ensure({
    blobId: rope.textBlobId(initialBytes),
    encoding: shape.TextEncoding.UTF8,
    byteLength: len(initialBytes),
    contentHash: hash("TextBlob", initialBytes),
  }) else rope.TextBlobHashConflict;
  yield some(blob);
};
```

Rules:

- an effectful branch expression is legal only as the right-hand side of a
  `let`;
- each branch is an ANF block;
- each branch ends with `yield expr`;
- all branches yield the same type;
- effect calls inside branches obey normal A-normal rules;
- Core lowers the construct to a structured branch with a result binding;
- effects inside branches retain ordering and branch structure. Flattened path
  predicates are derived analysis, not hash-significant Core fields.

## Lawpack Semantic Effects

Portable semantic Edict is still effectful when it emits semantic facts. A
lawpack semantic effect may lower to different target effects on different
profiles, but Core must record the imported lawpack authority and the semantic
effect coordinate.

Semantic effects must declare:

- identity and digest of the owning lawpack;
- effect coordinate;
- input type;
- output type;
- `executionClass` (`proofOnly` or `runtime`), orthogonal to the resolved
  `writeClass`;
- footprint summary or required target lowering obligations;
- cost summary or required target lowering obligations;
- obstruction taxonomy if the effect can fail at runtime.

Read-only claims must inspect semantic effects. A proof-only semantic fact may
be compatible with read-only; a semantic fact that lowers to an append, create,
replace, delete, audit write, or projection mutation is not.

## Target Symbolic References

Target source values must not be live runtime authority. They are symbolic plan
terms.

Recommended Echo-shaped model:

```amblicit
echo.ref<T>(id) -> EchoRef<T>
echo.ref<T>(id).read() -> T
echo.ref<T>(id).exists() -> Bool
echo.ref<T>(id).create(value: T) -> T
echo.ref<T>(id).ensure(value: T) -> T
echo.ref<T>(id).replace(value: T) -> T
echo.ref<T>(id).delete() -> Unit
echo.edge<A, B, E>(from: EchoRef<A>, to: EchoRef<B>).create(value: E) -> E
```

`EchoRef<T>` is inert, canonical, and hashable as a plan term. It cannot read,
write, or mutate unless an explicit imported effect is called.

Content-addressed immutable entities should usually use `ensure`, not
`create`:

- `create(value)` requires absence;
- `ensure(value)` creates if absent and otherwise requires the existing value
  to equal `value`;
- `replace(value)` requires presence and replaces the value;
- `delete()` requires presence;
- `upsert(value)` is not in Core v1 and should be target-specific if needed.

Prefer direct effect obstruction mapping over time-of-check/time-of-use
prechecks. For example, `create(...) else DomainAlreadyExists` is the preferred
v1 spelling when the only purpose of `exists()` would be to guard the create.

### Capability References

`CapabilityRef<T>` is a first-class, inert Core value that carries a
capability-receipt **digest** only (`EDICT-LANG-CAPREF-001`). It names a
delegated authority without embedding the receipt. Like `EchoRef<T>`, it is
canonical, hashable, and powerless on its own: it grants nothing until an
imported effect that accepts it is called, and the participant has admitted the
referenced capability receipt. The full capability receipt (scope, basis,
bounds, revocation, expiry, policy epoch) is a Continuum Admission artifact and
stays external to Edict Core (see
[SPEC - Continuum Admission v1](./SPEC_continuum-admission-v1.md)).

The carried receipt digest is the value's canonical payload and is
**hash-significant** wherever a `CapabilityRef<T>` value appears — in Core IR,
predicates, and artifact identity — exactly as a `Digest` scalar would be. Only
the external receipt body stays out of the Core hash; the digest that names it
does not (`EDICT-LANG-CAPREF-001`).

## Effect Failure Obstructions

Target and lawpack effects can fail at runtime even when source typechecks.
Source authors map effect failure classes to domain obstructions with an
effect-level `else` clause:

```amblicit
let worldline = worldlineRef.read()
  else rope.WorldlineMissing;

let worldline = worldlineRef.create({ ... })
  else rope.BufferWorldlineAlreadyExists;

let blob = blobRef.ensure({ ... })
  else {
    mismatch => rope.TextBlobHashConflict,
  };

worldlineRef.replace(nextWorldline)
  else rope.StaleBaseHead;
```

Obstruction mapping is exhaustive variant matching over the effect's declared
domain-mappable failure **coordinates** (`EDICT-LANG-OBSTRUCT-EXHAUST-001`). It
reuses the same machinery as `match`: every domain-mappable failure coordinate
must be handled exactly once. A mapping arm may **bind the low-level failure** and **construct a
typed obstruction payload** from it. A bare coordinate is sugar for an empty
payload:

```amblicit
let blob = blobRef.ensure(candidate)
  else {
    mismatch(fault) => rope.TextBlobHashConflict({
      expected: candidate.contentHash,
      observed: fault.existingContentHash,
    }),
    boundExceeded(fault) => rope.ReadBoundExceeded({
      limit: fault.declaredMax,
    }),
  };
```

Runtime guards construct payloads the same way:

```amblicit
require currentHead == expectedHead
  else rope.StaleBaseHead({
    expected: expectedHead,
    observed: currentHead,
  });
```

The target profile owns the low-level failure taxonomy and the typed,
**bounded** payload schema for each obstruction (see
[SPEC - Edict Lawpack ABI v1](./SPEC_edict-lawpack-abi-v1.md) and the Target
Profile ABI). An obstruction payload whose fields are not typed and bounded is
rejected — "typed obstruction payload" is not ceremonial paperwork. Exhaustiveness is counted by **`domainMappable` failure coordinate**, not by
authority class (an effect can have several `domainMappable` failures — distinct
coordinates — that share the `domainMappable` class). The single-obstruction
`else Obstruction` shorthand is legal only when the number of unmapped
`domainMappable` failure **coordinates** is **exactly one**; any other count
rejects. With **zero** unmapped domain-mappable coordinates (the effect declares
none, or all are already handled) the `else` clause is omitted entirely; writing
one is rejected as dead handling. Effects with **two or more** unmapped
domain-mappable coordinates must use the full `else { failure => ... }` mapping
form, one arm per coordinate (`EDICT-LANG-OBSTRUCT-EXHAUST-001`).

Every declared effect failure payload type is an authenticated root of the
same exact named-type closure as the effect input and output. A source
obstruction binder has that resolved payload type; the failure name selects an
arm but does not synthesize a new `effect.failure` Core type identity. Missing
or unresolved payload roots reject before compiler or Target facts exist.

Low-level failure classes are classified before source mapping:

- `domainMappable`: author may translate to a typed domain obstruction;
- `participantOwned`: participant policy or capability failure, never
  author-mapped;
- `integrityFault`: digest, signature, verifier, or tamper failure, never
  author-mapped;
- `resourceFault`: budget, fuel, memory, output, or target resource failure;
- `internalFault`: compiler, lowerer, verifier, host, or plugin defect.

Authors may map only target-profile-declared `domainMappable` classes.
Participant-owned, integrity, resource, and internal faults retain
platform-owned coordinates unless the target profile explicitly classifies a
bounded resource failure as a domain obstruction. Domain obstructions may carry
typed payload schemas declared by the lawpack or target profile.

Core effect nodes carry an obstruction map keyed by each domain-mappable
**failure coordinate** (not by authority class, so two `domainMappable` failures
on the same effect such as `mismatch` and `boundExceeded` stay distinct) to an
obstruction coordinate plus a (possibly empty) typed payload constructor, with an
optional binder for the low-level failure:

```text
obstructionMap:
  missing      -> rope.WorldlineMissing {}
  conflict     -> rope.BufferWorldlineAlreadyExists {}
  mismatch(f)  -> rope.TextBlobHashConflict { observed: f.existingContentHash }
  staleGuard   -> rope.StaleBaseHead { expected, observed }
  boundExceeded(f) -> rope.FootprintExceeded { limit: f.declaredMax }
```

`require predicate else Obstruction` remains useful for source-level invariants.
When the predicate protects mutable target state, lowering must attach it to the
relevant target effect as an atomic runtime guard rather than treating it as only
a local read-time assertion.

## Atomic Application

Lawful-autonomous v1 operations apply as one target-owned atomic application
unit. Runtime reads observe one target-defined application snapshot. Runtime
`require` guards and precommit `guarantee` checks are evaluated in that same
unit. Writes become visible only if all reads, guards, effects, budgets,
resource checks, and guarantees succeed.

Any obstruction or platform-owned failure aborts the application without
externally visible partial writes. If an intent appears to need effects across
multiple physical systems, it must target a composite target profile that owns
the coordination and atomicity semantics.

## Bounded Closure Reads

Closure reads are not safe merely because they are reads. Every target or
lawpack closure intrinsic must declare finite bounds or be rejected before a
locked bundle is produced.

Examples of required bound dimensions:

- maximum nodes;
- maximum bytes;
- maximum depth;
- maximum anchors;
- maximum branches;
- maximum leaves;
- maximum proof cost.

`textWindow` is a good positive fixture because its input naturally bounds
viewport lines, context lines, and bytes. A full-buffer `worldlineSnapshot`
without a type-level maximum or input maximum is a negative fixture until the
target/lawpack proves a finite bound.

## Cost Model

Footprint and cost are separate.

Footprint answers:

```text
What state may this touch?
```

Cost answers:

```text
How much work, memory, target IO, and output may this consume?
```

Every operation must have an inferred cost model. Cost is checked against
declared ceilings, never against an admitted participant budget at compile/lower
time (`EDICT-LANG-BUDGET-SPLIT-001`):

- Core cost is checked against the declared Core ceiling.
- Target cost is checked against the declared target ceiling.
- Admission may impose a stricter ceiling afterward; it is external and never
  seen by the lowerer/verifier.

Cost is split into three layers so the portable Core does not own
target-specific cost dimensions:

```text
coreEvaluationBudget:        # portable, owned by Edict Core
  maxSteps
  maxAllocatedBytes
  maxOutputBytes

targetBudget:                # owned by the target cost algebra
  costAlgebra                # digest-locked resource reference
  ceiling                    # resolved canonical typed value

admittedBudget:              # participant ceiling over the target budget
                             # (Continuum Admission artifact, not Core)
```

Core budget units are pinned (`EDICT-LANG-BUDGET-UNITS-001`):

- `maxSteps`: maximum weighted Core evaluation steps under `edict.core-cost/v1`.
  A step is not a CPU instruction; expression/statement node evaluation, bounded
  loop iterations, pure-function invocation, and imported-helper execution are
  charged by the digest-locked Core cost schedule. `edict.core-cost/v1` is that
  schedule (the per-construct step weights); like the canonicalization profile it
  is pinned by digest and owned by the Phase 0 cost-model work, not redefined
  per operation.
- `maxAllocatedBytes`: maximum peak live canonical-value capacity during Core
  evaluation, excluding target-owned state, source maps, code, and host
  allocator bookkeeping.
- `maxOutputBytes`: maximum `edict.canonical-cbor/v1` encoded output size in
  octets.

`targetBudget` in semantic Core has exactly two **hash-significant** members
(`EDICT-LANG-TARGETBUDGET-HASH-001`): the digest-locked `costAlgebra` resource
reference and the resolved typed `ceiling`. **Both** are in the Core preimage —
the `ceiling` is meaningless without the `costAlgebra` that denominates it, so a
ceiling of `128` under two different cost algebras must hash differently. The
only non-hash part is an optional human-readable cost coordinate kept as display
metadata. This is a small, fixed cost vocabulary — not a gas economy.

Target reads, writes, closure reads, and generated-effect counts are
target-cost-algebra dimensions, not portable Core dimensions; they live in
`targetBudget`, owned by the Target Profile ABI. A portable lawpack may declare
an abstract `targetBudget` obligation that its adapter translates into the
selected target's cost algebra (see
[SPEC - Edict Lawpack ABI v1](./SPEC_edict-lawpack-abi-v1.md)). Participant
policy may only **lower** the admitted ceiling; it never supplies a missing Core
budget (see [SPEC - Continuum Admission v1](./SPEC_continuum-admission-v1.md)).

Pure lawpack helpers are also costed against `coreEvaluationBudget`. A
deterministic helper that can allocate unbounded memory or scan unbounded input
is not acceptable in the lawful-autonomous lane. Helper bounds are enforced in
two stages (`EDICT-LANG-HELPER-BOUNDS-001`): **lawpack import validation**
rejects a helper whose signature or component manifest lacks finite output and
cost bounds; **call-site type checking** then instantiates those bounds from the
concrete type arguments and argument refinements at each use. An opaque helper
without a conservative digest-locked bound is unavailable in the
lawful-autonomous lane.

## Development Mode Versus Locked Bundle Mode

Source may omit digests during development if a lockfile or local resolver can
resolve them. No contract bundle may contain floating imports.

Locked bundle production requires:

- source profile identity and digest;
- source artifact digest in bundle provenance, outside the Core hash preimage;
- source-profile canonical facts digest, when applicable;
- Shape IR digest for imported GraphQL shapes;
- Law IR digest for imported `weslaw`;
- every lawpack identity, version, and digest;
- every target profile identity, version, and digest;
- compiler identity and digest;
- target lowerer identity and digest;
- verifier identity and digest;
- compile options digest;
- conformance fixture digest set.

A path such as `contracts/jedit/rope.graphql` is a locator, not identity. The
bundle identity must use the content hash plus source-profile facts and
canonical Shape IR hash.

Participant descriptor digests, catalog snapshot digests, admission policy
digests, policy epochs, requested capabilities, admitted ceilings, and admission
receipts are Continuum Admission artifacts. They may reference a contract bundle
digest, but they must not be hashed into the participant-neutral contract bundle
identity.

## Bundle, Admission, And Transparency Boundary

Bundle provenance, attestation envelopes, signature roles, admission requests,
admission receipts, transparency readiness, capability receipts, explanation
artifacts, and nutrition labels are platform artifacts. They are intentionally
outside the Edict language and Core hash surface. See
[SPEC - Continuum Contract Bundle v1](./SPEC_continuum-contract-bundle-v1.md),
[SPEC - Continuum Admission v1](./SPEC_continuum-admission-v1.md), and
[GUIDE - Edict Assurance And Transparency](./GUIDE_edict-assurance-transparency.md).

## Query And Observer Operations

Read-only query operations are first-class lawful-autonomous operations. They
must carry the same source artifact digest, Core IR digest, target profile,
footprint, cost budget, verifier result, generated artifacts, admission posture,
and obstruction taxonomy as write operations.

Observer source sugar, if accepted by a frontend, must desugar to an `intent`
with a read-only source claim. The read-only claim remains inferential: writes,
appends, runtime-materialized semantic records, unbounded reads, or unchecked
costs reject the bundle.

Capability receipts are Continuum Admission artifacts, not Edict language
values. Explanation artifacts and nutrition labels are platform assurance
artifacts, not parser prerequisites; their fields are defined by the admission
spec and assurance guide.

## Language Invariants

### I-001 Runtime Neutral Core

Edict Core contains no storage-native primitives. The following names are not
core built-ins:

- graph node, edge, attachment, span, DPO rule;
- git commit, ref, tree, path, merge base, reducer;
- SQL table, row, transaction, query;
- KV key, range, compare-and-swap;
- event stream, append, projection;
- filesystem path, network socket, process, thread, scheduler tick.

Any such concept must come from a target profile import.

### I-002 Explicit Target Authority

Every effectful target operation must be reachable through an explicit target
import:

```amblicit
use target echo.dpo@1 as echo;
```

No target intrinsic may be visible through prelude, ambient namespace, dynamic
lookup, reflection, or host callback injection.

### I-003 Determinism

The same source, source profile, lawpack digests, target profile digest,
compiler version, and compile options must produce byte-identical Core IR,
target IR, generated artifacts, and manifests.

Forbidden in the lawful-autonomous lane unless represented as explicit input or
provenance:

- wall-clock time;
- randomness;
- ambient environment;
- filesystem or network IO;
- locale-dependent comparison;
- nondeterministic map/set iteration;
- hidden actor or machine identity;
- global mutable state;
- reflection or `eval`;
- unconstrained recursion;
- unbounded loops.

Hidden model state, prompt context, browser DOM state, filesystem state, network
responses, host callbacks, and scheduler state may influence candidate source
generation above the boundary. They may not influence compiled execution below
the boundary unless materialized as explicit canonical input, witnessed evidence,
admitted basis, or capability presentation.

### I-004 Total Bounded Evaluation

Pure Edict functions are total and cost-bounded over valid inputs. Loops must
be over finite collections with statically known or type-bounded maximum
cardinality. Recursive function calls are rejected in v1.

### I-005 Typed Effects

Every target intrinsic has a declared type and effect signature supplied by its
target profile. Every lawpack semantic effect has a declared type and effect
signature supplied by its lawpack. The compiler infers operation effects from
imported effects, not from author-declared footprint text.

### I-006 Footprint Honesty

Declared footprint bounds, when present, are policy ceilings:

```text
computedFootprint <= declaredMaxFootprint
```

Underclaiming rejects at compile time. Overclaiming may pass but reduces
admission and scheduling precision.

### I-007 FIDLAR Rejection

An Edict intent must not receive raw runtime mutation authority in the
lawful-autonomous lane. Privileged/native host extensions must use a separate
trust lane and must not claim compile-time footprint honesty unless they lower
to checked target IR with inferred effects.

### I-008 Lawpack And Profile Locking

All imported shapes, lawpacks, target profiles, lowerers, and verifiers must
resolve by identity, version where applicable, and digest before a locked
bundle is produced. A compile that resolves the same name to a different digest
must emit a mismatch diagnostic unless the author updates the lock.

### I-009 Canonical Names

Package, type, function, intent, field, profile, and import names normalize to
canonical coordinates. The canonical coordinate, not local alias spelling, is
used in Core IR hashes.

### I-010 Stable Core IR Hashes

Core IR hash input excludes source locations, comments, formatting, import
alias spelling, and nondeterministic map order. It also excludes derived indices
and packaging fields (see Edict Core IR): the flat `preconditions`/
`postconditions` arrays, `verifiedOperationMode`, `diagnosticPolicy`, and
lowerer/verifier component digests. It includes all authoritative semantics:
resolved coordinates, type signatures, expressions, operation bodies (including
the authoritative `require`/`guarantee` nodes from which preconditions and
postconditions are derived), the optic contract, `inputConstraints` predicate
trees, imports, profile references, and effect model references
(`EDICT-CORE-NODUP-PREPOST-001`).

### I-011 Target Lowering Is Profile-Owned

Wesley or the Edict compiler may orchestrate target lowering, but the meaning
of the lowering is owned by the target profile and lawpack. Echo Span IR is
valid only for profiles that adopt Echo DPO semantics.

### I-012 Failure Class Separation

Edict preserves three failure classes:

- compiler error: invalid source, invalid Core IR, invalid lowering;
- registration/admission error: unsupported, stale, tampered, unsigned, or
  policy-rejected bundle;
- runtime obstruction: valid registered operation cannot apply to current
  runtime state.

### I-013 Schema Evolution Is Explicit

New required fields, changed operation ABIs, target profile revisions, lawpack
semantic changes, and bundle hash changes are separate version axes. The
compiler must not silently reinterpret old target IR under a new shape.

### I-014 Assurance Is Evidence, Not Authority

HOLMES, Watson, and Moriarty produce SHA-locked evidence and findings. They do
not mutate runtime state and do not override participant admission policy.

### I-015 Ordered Effect Nodes

All effectful source expressions lower to ordered Core effect nodes. Core does
not preserve nested effect expressions.

### I-016 Structured Control Flow And Bounds

Hash-significant Core IR preserves structured branches, loop cardinality bounds,
closure bounds, and explicit guard nodes. Conditional creates, bounded loops,
and closure reads must remain visible to footprint, cost, and target lowering.
Flattened path predicates and effective cardinalities are derived effect
analysis, not a second authoritative Core representation.

### I-017 Imported Semantic Effects

Lawpack semantic records are imported semantic effects, not Core built-ins.
`record history.foo { ... }` is syntax sugar for a digest-locked lawpack
intrinsic.

### I-018 Read-Only Is Inferred

Read-only is a theorem proven from inferred effects. A source profile may claim
read-only posture, but the compiler must reject the claim if target or lawpack
effects include writes, appends, runtime-materialized semantic records, hidden
audit mutations, or other non-read effects.

### I-019 Bounded Closure Reads

Closure reads require finite profile-declared bounds. Unbounded closure reads
reject compilation or locked-bundle production.

### I-020 No Floating Bundle Imports

Development source may use floating imports only when a lockfile or local
resolver supplies exact digests. Contract bundles cannot contain floating
imports.

### I-021 Bounded Lawpack Helpers

Pure lawpack helpers must be deterministic, bounded, digest-locked, and
authority-free. Opaque helpers cannot contribute to precise compile-time
footprint honesty except through conservative declared bounds.

### I-022 Explicit Content-Addressed Creation

Content-addressed writes must choose `create`, `ensure`, `replace`, or
`delete` semantics explicitly. Duplicate content-addressed `create` is a
conflict unless the target profile defines an idempotent create profile.

### I-023 Runtime Guards For Mutable Assumptions

State assumptions that can change between read and apply must lower to runtime
guards in target IR. A source `require worldline.canonicalHeadId ==
baseHead.headId` is not only a local assertion; it must become a stale-basis
guard or equivalent target condition.

### I-024 Codenames Are Not Canonical Coordinates

Codename terms are non-authoritative display metadata. Hash-significant
coordinates, target profile identifiers, bundle profile identifiers, admission
receipts, and canonical hashes must use formal identifiers such as
`continuum.lane.lawful-autonomous/v1`, not codenames.

### I-025 Effect Failure Is Explicit

Imported effect failure classes must either map to typed domain obstructions or
be rejected by the compiler. Runtime effect failures must not collapse into
generic host exceptions.

### I-026 Conditional Effects Use Branch-Yield

Conditional effect values use `let x = if predicate { ... yield expr; } else {
... yield expr; };`. Nested effect calls inside pure expressions remain
rejected.

### I-027 Core Hashes Alpha-Normalize Locals

Local binder names are alpha-normalized in Core hash input. Source local names
may appear in review JSON or diagnostic sidecars but must not change the
hash-significant Core IR.

### I-028 Admission Evidence Is External

The `semanticBundleDigest` and `releaseBundleDigest` are computed before
admission. Admission requests and receipts reference a `bundleSubject`
(`{ kind, digest }`) but are not components of either bundle digest. A
distribution envelope may aggregate bundles, attestations, and receipts without
changing the identity of the enclosed contract bundle
(`CONTINUUM-BUNDLE-SUBJECT-001`).

### I-029 Atomic Application

A lawful-autonomous v1 intent lowers to one target-owned atomic application
unit. Runtime reads and guards observe one target-defined application snapshot.
All writes become visible atomically. Any obstruction, resource failure, budget
failure, failed runtime precondition, or failed precommit guarantee leaves
externally visible target state unchanged.

### I-030 Single Runtime Effect Domain

An intent may lower to effects owned by at most one runtime target profile.
Lawpack semantic effects may lower into that target. Cross-target application
requires a composite target profile that owns coordination, obstruction, and
atomicity semantics.

### I-031 Intent Optic Structure

Every Edict intent is an optic-shaped lawful operation specification. Core
carries a **minimal normative optic contract** — `opticKind`, `basis`,
`boundaryKind` (`projection`/`affect`), aperture/footprint requirement,
`supportPolicy`, and `lossDisposition` (see Optic Contract under Edict Core IR).
The compiler and target profile must preserve these fields plus cost bounds,
guards, obstruction mappings, and canonical artifact identity through lowering.

Richer Observer Geometry quantities — support carried/lost/blocked/refuting,
degeneracy findings, witness debt, and footprint overlap — are **derived
verifier evidence** (the Aperture Ledger), not Core syntax, until the support
algebra is pinned in a later version. A lowering that erases support loss,
degeneracy, footprint overlap, or witness debt must reject or record the loss
explicitly in that derived evidence (`EDICT-OPTIC-PRESERVE-001`). If the optic
contract is not encodable for an operation, the operation is rejected, not
silently lowered as a plain morphism. A theorem the IR cannot represent is a
wish.

## Syntax Overview

An Edict source file declares one package and zero or more imports and
declarations:

```amblicit
package graft.structural_history@1;

use shape "schemas/structural-history.graphql" as shape;
use lawpack history.optics@1 digest "sha256:..." as history;
use target echo.dpo@1 digest "sha256:..." as echo;

type RecordGitWarpImportBatchInput = {
  basisId: shape.ID,
  repo: String<max=512>,
  commit: Digest,
};

type RecordGitWarpImportBatchReceipt = {
  batchId: Digest,
};

intent recordGitWarpImportBatch(input: RecordGitWarpImportBatchInput)
  returns RecordGitWarpImportBatchReceipt
  profile echo.createOnly
  basis input.basisId
  budget <= history.recordBatchBudget
  where input.repo != ""
{
  let basisRef = echo.ref<StructuralBasis>(input.basisId);
  let basisValue = basisRef.read()
    else history.StructuralBasisMissing;
  let batchId = hash("GitWarpImportBatch", input.repo, input.commit);

  let batchRef = echo.ref<GitWarpImportBatch>(batchId);
  let batch = batchRef.create({
    repo: input.repo,
    commit: input.commit,
  }) else history.GitWarpImportBatchAlreadyExists;

  echo.edge<GitWarpImportBatch, StructuralBasis, BasedOn>(
    batchRef,
    basisRef
  ).create({}) else history.BasedOnEdgeConflict;

  return { batchId };
}
```

Portable semantic Edict can target a lawpack instead of a concrete runtime:

```amblicit
package graft.structural_history@1;

use lawpack history.optics@1 digest "sha256:..." as history;

intent recordGitWarpImportBatch(input: RecordGitWarpImportBatchInput)
  returns RecordGitWarpImportBatchReceipt
  implements history.recordEntry
  basis input.basisId
  budget <= history.recordBatchBudget
{
  let entry = history.entry.record({
    id: hash("GitWarpImportBatch", input.repo, input.commit),
    kind: "gitWarpImportBatch",
    repo: input.repo,
    commit: input.commit,
    basis: input.basisId,
  }) else history.EntryRecordObstructed;

  return { batchId: entry.id };
}
```

The portable form only compiles for a target when a lawpack supplies a lawful
lowering from the semantic operation to that target profile.

Source may also use `record history.entry { ... } else Obstruction;` sugar. That
sugar is not a Core primitive; it desugars to the lawpack semantic effect call
shown above.

## Names, Namespaces, And Shadowing

Local bindings must not shadow import aliases, package aliases, type names, or
prelude coordinates (`EDICT-LANG-NOSHADOW-001`). Clever shadowing saves four
keystrokes and spends a week in diagnostics court. A `let` whose name collides
with any in-scope import alias, type, or prelude function is rejected. (The
README's older `greeting`-alias-plus-`greeting`-local example is exactly the
pattern this rule forbids.)

Enum cases and variant constructors use distinct, non-overlapping syntax
(`EDICT-LANG-ENUMVARIANT-001`):

- An **enum case** is selected by field access on the enum type:
  `Qual.EnumType.CASE` (e.g. `shape.TextEncoding.UTF8`). Enum cases carry no
  payload.
- A **variant constructor** uses `::` and may carry a payload:
  `Qual.VariantType::Case(payload)`.

`match` arms name the bare case (`upper-ident`) for both enums and variants; the
matched type disambiguates.

## Lexical Rules

Edict source is UTF-8. Keywords and punctuation are ASCII. Identifiers use a
restricted portable subset in v1.

```ebnf
letter        = "A"..."Z" | "a"..."z" | "_" ;
digit         = "0"..."9" ;
hex           = digit | "A"..."F" | "a"..."f" ;
ident         = letter , { letter | digit } ;
upper-ident   = "A"..."Z" , { letter | digit } ;
qual-ident    = ident , { "." , ident } ;
version       = digit , { digit | "." | "-" | "_" | letter } ;
package-ref   = qual-ident , "@" , version ;
digest-lit    = '"' , "sha256:" , 64 * hex , '"' ;
string-lit    = '"' , { string-char | escape } , '"' ;
bytes-lit     = "b" , string-lit ;
int-lit       = digit , { digit | "_" } , int-suffix? ;
int-suffix    = "i32" | "i64" | "u8" | "u32" | "u64" ;
bool-lit      = "true" | "false" ;
comment       = "//" , { not-newline } | "/*" , { not-end-comment } , "*/" ;
```

Whitespace and comments are not semantic. Source locations are retained for
diagnostics but excluded from canonical Core IR.

Lexical conformance fixtures must pin these details before parser freeze:

- `bytes-lit` uses the same ASCII escape spelling as `string-lit`, but the
  decoded payload is bytes. Non-ASCII source characters inside `bytes-lit` are
  rejected unless escaped as explicit byte escapes.
- `string-lit` decodes to Unicode scalar values and does not normalize by
  default.
- String and byte literals accept `\"`, `\\`, `\n`, `\t`, `\r`, `\0` and
  `\xHH`. The hexadecimal form consumes exactly two ASCII hexadecimal digits
  and inserts one octet. A decoded string must remain valid UTF-8; a byte
  literal may contain any octet. `\u{...}` escapes are not supported in v1.
  Literal byte length is the decoded payload length, independent of escape
  spelling. Adjacent `b` and an opening quote form one byte literal token;
  a separated `b` remains an identifier.
- Integer underscores may appear only between digits, never at the beginning,
  end, or adjacent to another underscore.
- Integer width and signedness are hash-significant, so every integer value
  must resolve to exactly one of `I32`, `I64`, `U8`, `U32`, `U64`. A literal resolves
  by explicit suffix (`1u64`, `64_000i64`) or by an unambiguous expected type
  propagated from its context. Expected-type propagation reaches: record fields,
  variant payloads, `Option` constructors, `List`/`Map` literal elements,
  function and intrinsic arguments, return expressions, explicit type
  annotations, and **binary equality/relational/arithmetic operands** (a bare
  literal takes the resolved type of the typed opposite operand, so `len(x) == 0`
  and `input.maxBytes >= 0` resolve `0` to the operand's width). It does **not**
  propagate through unconstrained variadic calls such as `hash("x", 1)`, nor when
  both operands are bare literals. A bare literal with no suffix and no unambiguous
  expected type is rejected (`EDICT-LANG-INTLIT-001`); `hash("domain", 1)` is
  rejected because `1` has no resolvable width. If an explicit suffix disagrees
  with the contextual type (e.g. `1i32` where `U64` is required), the literal is
  rejected with `EDICT-LANG-INTLIT-002`.
- `digest-lit` hex is lowercase in canonical source rendering. Uppercase hex may
  parse only if normalized before semantic comparison.
- `pattern` constraints use `edict.regex-lite/v1`, a locked non-backtracking
  regular-expression profile, not an implementation-defined host regex engine.
- Unicode normalization policies such as `canonical=nfc` resolve to a versioned
  normalization profile pinned by digest.

## Source compilation closures

A source-module import uses `use module package@version digest "sha256:..." as alias;`.
The digest is mandatory. It identifies the exact dependency UTF-8 bytes encoded
as one canonical CBOR byte string under `edict.source/v1`. The domain-framed
preimage is `["edict.digest/v1", "edict.source/v1", source-byte-string]`.
Comments and whitespace are part of source identity. Import digest spelling
must match the canonical lowercase review rendering during authentication.

`source_closure::resolve_source_closure` accepts one explicit root and all
explicit dependency sources. It performs no filesystem discovery. The fixed
ceilings are 64 units including root, 1 MiB of aggregate original source bytes,
and 32 units on any root-to-leaf import path. Caller apertures may narrow these
ceilings. Count and aggregate-byte checks precede parsing; parsed package
coordinates must be unique and source surfaces must validate.

Every module import must resolve within the supplied set, every supplied unit
must be root-reachable, every import pin must match, and the graph must be
acyclic. Topology refusal precedes pin comparison. No authenticated witness is
returned until all checks pass. Shared dependency height is independent of the
path that first visits it, so a shallow occurrence cannot hide a deeper path.

The witness retains immutable exact sources and parsed surfaces. Its canonical
artifact follows [edict-source-closure.cddl](abi/edict-source-closure.cddl), with
common digest framing from [edict-common.cddl](abi/edict-common.cddl). `root`
selects the entry package. The `modules` map is keyed by exact package
coordinates; each value contains `source` bytes and a typed `["sha256", bstr32]`
digest. It embeds no closure self-digest. Its external identity uses the same
canonical three-element digest frame under `edict.source-closure/v1`.
Dependency input order does not affect canonical bytes or identity.

The application manifest's `sources` array supplies this same explicit closure:
the first path selects the root and later paths supply dependencies. Source
paths remain confined to the manifest root and canonical path duplicates are
refused. A single source with no module import retains the legacy canonical
source-byte-string artifact. Module builds supply the new closure artifact,
domain and auxiliary kind to both provider invocations; provider support for
that form is required. Parse, linking and compilation failures retain original
file paths and source-local byte spans. An error without a source ownership
witness stays in the summary rather than acquiring a guessed file location.

Source authentication does not grant execution authority. The separate
`source_linker::link_source_closure` API creates an immutable compiler-owned
linked surface while retaining the original witness. Its `compile` method
accepts explicit compiler facts and reports failures against original package
coordinates and source-local byte spans.

Source imports expose a dependency's own type and pure-function declarations
through its explicit alias, without implicit transitive re-exports. Every
linked definition is checked, including unused definitions. Library functions
and types resolve only in their own lexical module. Compiler names are fresh
and deterministic; root intent entry names and runtime record/member labels
remain authored. Imported intent entries refuse as `ImportedIntent`. Built-in
`ExternalActionRequest` constructors and capability request callees remain
supported. Authored namespace calls, including `core.*`, require an import
owned by the same source module.

A source record retains its module-owned `package@version.Type` coordinate
across importing applications. Root record table keys retain original local
names. Renaming MUST NOT bypass reserved-type checks or original-coordinate
collisions with imported callable/type authority. Aliases remain transparent.
Type traversal accepts constructor height 128 and refuses 129 independently
of lexical statement/expression frames; active lexical traversal and flattened
member paths have fixed limits of 128.

Standalone `compile_to_core` refuses unresolved module imports as
`MissingContextFact`, including imports that are not referenced by a declaration.

## Grammar

This grammar is normative for v1 syntax shape. Operator precedence and exact
tokenization will be pinned by parser conformance fixtures.

```ebnf
module          = package-decl , import-decl* , declaration* ;

package-decl    = "package" , package-ref , ";" ;

import-decl     = shape-import
                | lawpack-import
                | target-import
                | core-import
                | capability-import
                | module-import ;

shape-import    = "use" , "shape" , string-lit , digest-clause? ,
                  "as" , ident , ";" ;
lawpack-import  = "use" , "lawpack" , package-ref , digest-clause? ,
                  "as" , ident , ";" ;
target-import   = "use" , "target" , package-ref , digest-clause? ,
                  "as" , ident , ";" ;
core-import     = "use" , "core" , package-ref , digest-clause? ,
                  "as" , ident , ";" ;
capability-import = "use" , "capability" , package-ref , digest-clause ,
                    "as" , ident , ";" ;
module-import   = "use" , "module" , package-ref , digest-clause ,
                  "as" , ident , ";" ;
digest-clause   = "digest" , digest-lit ;

declaration     = type-decl
                | enum-decl
                | const-decl
                | fn-decl
                | intent-decl ;

bound-ref       = int-lit | qual-ident ;

type-decl       = "type" , upper-ident , type-params? ,
                  "=" , type-expr , ";" ;
enum-decl       = "enum" , upper-ident , "{" ,
                  enum-case , { "," , enum-case } , ","? , "}" ;
enum-case       = upper-ident ;
const-decl      = "const" , ident , ":" , type-ref , "=" , expr , ";" ;

type-params     = "<" , ident , { "," , ident } , ">" ;
type-expr       = record-type | variant-type | type-ref ;
record-type     = "{" , field-decl* , "}" ;
field-decl      = ident , ":" , type-ref , field-constraint* , ","? ;
field-constraint = "max" , "=" , bound-ref
                 | "min" , "=" , bound-ref
                 | "pattern" , "=" , string-lit
                 | "canonical" , "=" , ident ;
variant-type    = "variant" , "{" , variant-case* , "}" ;
variant-case    = upper-ident , payload-type? , ","? ;
payload-type    = "(" , type-ref , ")" ;

type-ref        = qual-ident , type-args?
                | "String" , string-refine?
                | "Bytes" , bytes-refine?
                | "Option" , "<" , type-ref , ">"
                | "CapabilityRef" , "<" , type-ref , ">"
                | "ExternalActionRequest" , "<" , type-ref , ">"
                | "List" , "<" , type-ref , "," , "max" , "=" , bound-ref , ">"
                | "Map" , "<" , type-ref , "," , type-ref , "," ,
                  "max" , "=" , bound-ref , ">" ;
string-refine   = "<" , "max" , "=" , bound-ref ,
                  ( "," , "canonical" , "=" , ident )? , ">" ;
bytes-refine    = "<" , ( "max" | "exact" ) , "=" , bound-ref , ">" ;
type-args       = "<" , type-ref , { "," , type-ref } , ">" ;

fn-decl         = "fn" , ident , "(" , param-list? , ")" ,
                  "->" , type-ref , pure-block ;
pure-block      = "{" , pure-statement* , "}" ;
pure-statement  = let-stmt
                | assert-stmt
                | return-stmt ;

intent-decl     = "intent" , ident , "(" , param-list? , ")" ,
                  "returns" , type-ref ,
                  intent-clause* , block ;
intent-clause   = profile-clause
                | implements-clause
                | basis-clause
                | where-clause
                | footprint-clause
                | budget-clause ;
profile-clause  = "profile" , qual-ident ;
implements-clause = "implements" , qual-ident ;
basis-clause    = "basis" , ( "none" | expr ) ;
where-clause    = "where" , predicate , { "," , predicate } ;
footprint-clause = "footprint" , "<=" , qual-ident ;
budget-clause   = "budget" , "<=" , qual-ident ;

param-list      = param , { "," , param } ;
param           = ident , ":" , type-ref ;

block           = "{" , statement* , "}" ;

statement       = let-stmt
                | external-action-request-stmt
                | assert-stmt
                | require-stmt
                | guarantee-stmt
                | semantic-record-stmt
                | if-stmt
                | for-stmt
                | effect-stmt
                | return-stmt ;

let-stmt        = "let" , ident , type-annotation? , "=" ,
                  let-rhs , effect-else-clause? , ";" ;
let-rhs         = expr | effect-branch-expr ;
type-annotation = ":" , type-ref ;
external-action-request-stmt =
                  "request" , ident , ":" ,
                  "ExternalActionRequest" , "<" , type-ref , ">" , "=" ,
                  call-expr ,
                  "input" , "schema" , digest-locked-package-ref ,
                  "settlement" , "schema" , digest-locked-package-ref ,
                  "authority" , expr ,
                  "basis" , expr ,
                  "budget" ,
                    "maxSettlementBytes" , expr ,
                    "maxAttempts" , expr ,
                  "reconcile" , digest-locked-package-ref , ";" ;
digest-locked-package-ref = package-ref , digest-clause ;
assert-stmt     = "assert" , predicate , ";" ;
require-stmt    = "require" , predicate , obstruction-clause , ";" ;
guarantee-stmt  = "guarantee" , predicate , obstruction-clause? , ";" ;
obstruction-clause = "else" , obstruction-target ;
obstruction-target = qual-ident , ( "(" , expr , ")" )? ;
semantic-record-stmt = "record" , qual-ident , record-lit ,
                       effect-else-clause? , ";" ;
if-stmt         = "if" , predicate , block , else-clause? ;
else-clause     = "else" , ( block | if-stmt ) ;
for-stmt        = "for" , ident , "in" , expr ,
                  "bounded" , bound-ref , block ;
effect-stmt     = call-expr , effect-else-clause? , ";" ;
effect-else-clause = "else" , obstruction-handler ;
obstruction-handler = obstruction-target | obstruction-map-lit ;
obstruction-map-lit = "{" , obstruction-map-entry ,
                      { "," , obstruction-map-entry } , ","? , "}" ;
obstruction-map-entry = ident , failure-binder? , "=>" , obstruction-target ;
failure-binder  = "(" , ident , ")" ;
effect-branch-expr = "if" , predicate , effect-yield-block ,
                     "else" , effect-yield-block ;
effect-yield-block = "{" , statement* , yield-stmt , "}" ;
yield-stmt      = "yield" , expr , ";" ;
return-stmt     = "return" , expr , ";" ;

predicate       = expr ;

expr            = if-expr ;
if-expr         = "if" , predicate , "then" , expr , "else" , expr
                | logic-or ;
logic-or        = logic-and , { "||" , logic-and } ;
logic-and       = equality , { "&&" , equality } ;
equality        = relation , { ( "==" | "!=" ) , relation } ;
relation        = additive , { ( "<" | "<=" | ">" | ">=" ) , additive } ;
additive        = multiplicative , { ( "+" | "-" ) , multiplicative } ;
multiplicative  = unary , { ( "*" | "/" | "%" ) , unary } ;
unary           = ( "!" | "-" ) , unary | postfix ;
postfix         = primary , { field-access | call-suffix | type-call-suffix } ;
field-access    = "." , ident ;
call-suffix     = "(" , arg-list? , ")" ;
type-call-suffix = type-args , "(" , arg-list? , ")" ;
arg-list        = expr , { "," , expr } ;

primary         = ident
                | qual-ident
                | int-lit
                | bool-lit
                | string-lit
                | bytes-lit
                | unit-lit
                | digest-value-lit
                | variant-lit
                | match-expr
                | record-lit
                | list-lit
                | map-lit
                | "(" , expr , ")" ;
unit-lit        = "unit" ;
digest-value-lit = "digest" , "(" , digest-lit , ")" ;
variant-lit     = qual-ident , "::" , upper-ident ,
                  ( "(" , expr , ")" )? ;
match-expr      = "match" , expr , "{" , match-arm+ , "}" ;
match-arm       = upper-ident , match-binding? , "=>" , expr , ","? ;
match-binding   = "(" , ident , ")" ;
record-lit      = "{" , record-entry-list? , "}" ;
record-entry-list = record-entry , { "," , record-entry } , ","? ;
record-entry    = record-field | spread-field ;
record-field    = ident , ":" , expr | ident ;
spread-field    = "..." , expr ;
list-lit        = "[" , expr-list? , "]" ;
expr-list       = expr , { "," , expr } , ","? ;
map-lit         = "map" , "<" , type-ref , "," , type-ref , ">" ,
                  "{" , map-entry-list? , "}" ;
map-entry-list  = map-entry , { "," , map-entry } , ","? ;
map-entry       = expr , "=>" , expr ;

call-expr       = postfix ;
```

Semantic grammar rules:

- `migration` and `projection` are reserved words for future syntax and are not
  accepted as v1 declarations.
- A source `type` declaration name MUST classify as a `core-named-type-ref`
  before type checking may publish it. Intrinsic identities and reserved bare
  structural constructor names therefore reject at the declaration span; the
  Core reference classifier, rather than a second source-only reserved-name
  list, owns this judgment.
- Keywords are reserved as bare identifiers but may appear after `.` as member
  names, so `ref.ensure(value)` and `history.event.record(value)` are legal.
- Each intent may contain at most one `profile`, one `implements`, one `basis`,
  one `footprint`, and one `budget` clause.
- The grammar accepts `intent-clause*`, but clause **requiredness** is a semantic
  rule; omitting a required clause is parseable but rejected in semantic
  validation (`EDICT-LANG-INTENT-CLAUSES-001`). Required: **at least one** of
  `profile` (operation mode on a target) or `implements` (a portable law profile
  the intent satisfies) — an intent may carry **both** (e.g. `implements` a law
  profile while declaring a `profile` operation mode); a `budget` clause; and a
  `basis` clause (see next bullet). Optional: `footprint` (a declared ceiling;
  the computed footprint is inferred regardless) and `where`.
- The `basis` clause is required unless the resolved operation profile or lawpack
  supplies a digest-locked basis template; `basis none` is the explicit no-basis
  declaration. The `basis` expression is a typed Core expression, never a
  free-form string.
- Multiple `where` clauses are permitted and merge conjunctively.
- `effect-else-clause` is legal only when the right-hand side expression or
  statement is an imported effect.
- A `require` always carries an `obstruction-clause` (`else`), whether its
  predicate is input-only or runtime-state-dependent. A precommit `guarantee`
  carries `else`; a verifier-discharged `guarantee` is a proof obligation with
  no `else`. `assert` never carries `else`. (`EDICT-LANG-REQUIRE-ELSE-001`)
- A `where` predicate may reference only inputs and pure functions of inputs,
  never runtime/target state, and never carries `else`.
- Single-obstruction `effect-else-clause` shorthand is legal only when exactly
  one profile-declared `domainMappable` failure **coordinate** remains unmapped.
  Effects with multiple unmapped domain-mappable coordinates must use
  `else { failure => Obstruction }` mapping syntax, one arm per coordinate.
- Only profile-declared `domainMappable` failure classes may be author-mapped to
  domain obstructions.
- A bare `obstruction-target` (a coordinate with no `( expr )` payload) is
  semantically equivalent to that coordinate with an empty record payload `{}`;
  it normalizes to `ObstructionConstruct { payload: {} }` in Core
  (`EDICT-LANG-OBSTRUCT-EMPTY-001`).
- `effect-branch-expr` is legal only in `let` binding position.
- `effect-yield-block` bodies must remain A-normal and all branches must yield
  the same type.
- `yield` is legal only as the final statement of an effect-yield block.
- `return` is not legal inside an effect-yield block.
- Pure function calls evaluate arguments left to right exactly once, including
  arguments unused by the callee, then enter a fresh lexical parameter/local
  frame. Calls preserve their conditional and pre-body evaluation positions;
  inlining must not duplicate, discard or eagerly evaluate an argument.
- Function declarations are pure. `pure-block` cannot contain imported effects,
  `require`, `guarantee`, branch-yield effect expressions, semantic record
  statements, or effect statements.
- `variant-lit` constructor cases must belong to the named variant type.
- `match-expr` must cover every variant case exactly once unless a future
  version adds explicit wildcard syntax.
- A bare `ident` record entry is **field shorthand**: `{ message }` desugars to
  `{ message: message }` (the field takes the value of the in-scope local or
  field of the same name). This is what `return { message };` and
  `return { batchId };` use (`EDICT-LANG-RECORD-SHORTHAND-001`).
- Record spreads evaluate left-to-right. Later explicit fields override earlier
  spread fields; duplicate explicit fields reject; the resulting field set must
  exactly match the expected record type.
- Map literal keys must satisfy v1 map-key restrictions and duplicate canonical
  keys reject.

## Core Types

Edict v1 defines a small deterministic type universe.

### Source aliases and declaration dependencies

A source declaration `type Alias = Target;` is transparent. It resolves to the
target's existing type coordinate and bounded representation. It creates no
additional nominal contract or Core type table entry. In particular, an alias
of a digest-bound imported nominal type preserves that exact contract and does
not permit substitution of its raw storage representation.

The current compiler resolves acyclic source aliases, record and variant dependencies
before checking function signatures or intent bodies. Declaration ordering does
not change the resulting canonical Core. Alias targets must already belong to
the currently lowerable type subset. Variant declarations preserve named cases
and payload references in Core. Checked constructors preserve the named case and
optional payload. Exhaustive matching lowers to Core with arm-local optional payload binders.
Generic and recursive types remain outside this source subset.

Source type dependency paths have a ceiling of 128 declarations. Fully expanded
structural height, including inline constructors, cached alias targets and
record fields, follows the Core height rule below: height 128 is accepted and
129 refuses as `InvalidBound`. A traversal of source type reference occurrences
has 65,536 work units.
Cumulative source-defined declaration expansion also has 65,536 units: scalar
and opaque imported references cost one; containers add one plus child weights;
records add one plus field weights. A reference to another source declaration
uses that declaration's complete expanded weight at every occurrence. These
checks precede expanded shape construction. Imported representation checks and
runtime budgets remain separate.

Missing types refuse as `UnresolvedType`, cycles as `InvalidCoreTypeIntegrity`,
and depth/work exhaustion as `InvalidBound`. Shared dependency heights are
context-independent, so an earlier cached occurrence cannot hide a deeper path.
Duplicate source type declarations also refuse as `SurfaceValidation` through
the public staged `resolve_module` then `type_check` API. Unsupported string
canonicalization refuses as `UnsupportedSourceShape`, including unused aliases.

### Scalar Types

- `Bool`
- `I32`, `I64`
- `U8`, `U32`, `U64`
- `String`
- `Bytes`
- `Digest`
- `Unit`

`U8` is the exact unsigned domain 0 through 255. Source accepts decimal `u8`
literals and context-typed values without cross-width coercion. The implemented
unsigned arithmetic subset remains U32/U64. Guarded raw byte lookup is available
through `at(bytes, index) -> U8`; this adds no U8 arithmetic.

`Float` is not a core v1 type. A target or lawpack may define canonical
floating-point semantics, but the core language does not assume them.

Core `String` is an exact sequence of Unicode scalar values. No implicit
normalization is applied. Comparison and hashing operate on the exact scalar
sequence unless the type or field declares a canonicalization constraint such as
`canonical=nfc`. Raw editor or file-buffer content should use `Bytes` or a
lawpack-defined raw text scalar, not ambiently normalized `String`.

#### Refined Scalar Types

`String` and `Bytes` are bounded with the same `<max=...>` mechanism as `List`
and `Map`; `Bytes` may instead require one exact byte length, and `String` may
also pin a canonicalization policy:

```amblicit
String<max=128>
String<max=128, canonical=nfc>
Bytes<max=65536>
Bytes<exact=32>

type UserName = String<max=128, canonical=nfc>;
type RawText  = Bytes<max=1048576>;
```

Only `String` may pin a `canonical=` policy; `Bytes` carries either `max` or
`exact`.
`Bytes<max=N, canonical=...>` is a syntax error (the grammar gives `Bytes` a
length-only refinement), because bytes are measured and hashed raw and must not be
normalized (`EDICT-LANG-BYTES-NOCANON-001`).

`Bytes<exact=N>` is the closed structural interval `min=N,max=N` in Core. It is
application-neutral: lawpacks may assign nominal coordinates such as `HeadId`
or `BlobId`, while Edict owns only the exact byte-length invariant.

Boundedness is expressible **everywhere** a type appears: intent parameters,
return types, type aliases, record fields, function parameters and returns, and
imported-effect arguments. A naked, unbounded runtime `String` or `Bytes` value
is rejected in the lawful-autonomous lane because its output cost cannot be
proven (`EDICT-LANG-BOUNDS-001`). The two spellings are **equivalent and both
valid**: the refined-type form (`name: String<max=128, canonical=nfc>`) and a
record field's `field-constraint` (`name: String max=128`). What is rejected is a
**conflicting double bound** — declaring both a refined-type bound and a separate
field constraint that disagree on the same field. A bare `String`/`Bytes` with
neither a refined-type bound nor a field constraint is the rejected naked form.

Length and bound units are pinned (`EDICT-LANG-LEN-001`):

- `len(value: String) -> U64` is the count of **Unicode scalar values**.
- `len(value: Bytes) -> U64` is the count of **bytes**.
- A `String<max=N>` bound is `N` Unicode scalar values; a `Bytes<max=N>` bound
  is at most `N` bytes; and `Bytes<exact=N>` is exactly `N` bytes.
- `canonical=nfc` (or another versioned profile) is applied **before** length
  is measured and before comparison or hashing.
- String/bytes concatenation result bounds are derived as the sum of operand
  maxima; a concatenation whose derived bound exceeds the destination type bound
  rejects at compile time.

The current compiler and target implementation support the bounded structural
Bytes specialization through `core.bytes.length<OperandType>(value)`, retaining
the exact operand coordinate and returning U64. The prelude takes one argument
and no authored type arguments. Other operand families, including String's
specified Unicode-scalar specialization, remain unsupported by this lowering;
they must not be silently measured in byte units instead.

### Fixed-Point Numeric Authority

Bunny is the normative authority for Edict fixed-point arithmetic
(`EDICT-LANG-FIXED-NUMERIC-001`). Edict's integration profile
`bunny.q32_32.checked/v1` selects the **checked** Q32.32 subset of
[`bunny-num` 0.6.0's Numeric Constitution](https://github.com/flyingrobots/bunny/blob/9bf43600d08ff8e2a0ab888713948b409e386513/docs/NUMERIC_CONSTITUTION.md).
The dependency is pinned exactly; an unreviewed newer Bunny release does not
change this profile. This Edict integration identifier is distinct from Bunny's
SDL scalar profile name `q32.32`.

The adopted rules are:

- A value has one signed two's-complement raw `i64`; its mathematical value is
  `raw / 2^32`. Raw construction and extraction preserve bits. Equality and
  ordering compare raw values exactly, without epsilon.
- Addition, subtraction and negation use exact raw arithmetic and reject when
  the result is outside `i64`.
- Multiplication and division use Bunny's `i128` intermediates, round to the
  nearest Q32.32 value with ties to even for either sign, then reject an
  out-of-range **rounded** result. Quantization may legitimately turn a small
  nonzero mathematical result into zero.
- A zero divisor is a distinct domain failure, including `0 / 0`. Checked
  failure must be explicit; it cannot become saturation, wrapping or a panic.
- Saturating operators, square root and floating-point conversion APIs are
  outside this adopted subset. No host float participates in canonical math.
  A future explicit float-ingress boundary must reject non-finite or
  out-of-range input using Bunny's validated conversion policy; its rounding
  and failure behavior require separate conformance evidence.

This does not reinterpret `I32`, `I64`, `U8`, `U32` or `U64` as fixed-point values.
Their exact domains, canonical identity, range proofs and signed division's
truncation-toward-zero rule remain unchanged. There is no implicit conversion
between an integer and this fixed-point domain.

The current implementation exposes only a checked Rust numeric foundation for
compiler consumers. It adds no fixed-point source type, literal, prelude
operation, Core value tag, Target instruction, or provider capability. Existing
integer artifact tags cannot be used as unmarked fixed-point encodings.

Before a future compiler fold or target implementation can claim this profile,
its accepted representation and capability must be explicit and hash-bound.
Compiler and runtime conformance must agree on literal raw-result vectors,
structured failures and canonical artifact bytes. Bunny's generated
[`i64-le-q32.32` wire profile](https://github.com/flyingrobots/bunny/blob/9bf43600d08ff8e2a0ab888713948b409e386513/generated/bunny-graphics.manifest.json)
is an eight-byte little-endian raw representation; it is not an existing Edict
canonical-CBOR fixed-point tag. A future Edict schema must specify how it binds
this raw representation and the profile identity without silently repurposing
integer values. No such wire extension is implemented by this foundation.

A dependency/profile upgrade requires review of results, failures and encoding
compatibility, with compiler/runtime conformance rerun before adoption. A
changed arithmetic or identity contract requires an explicit profile/version
transition; changing a manifest alone is not that transition.

| Relationship | Targets |
| --- | --- |
| `refines` | [Core types](#core-types), [Integer safety](#integer-safety) |
| `supersedes` | none |
| `depends_on` | [Pinned Bunny Numeric Constitution](https://github.com/flyingrobots/bunny/blob/9bf43600d08ff8e2a0ab888713948b409e386513/docs/NUMERIC_CONSTITUTION.md) |
| `related` | [Numeric foundation](./topics/numeric-foundation/README.md), [Core IR](./topics/core-ir/README.md), [Target profiles](./topics/target-profiles/README.md) |

### Compound Types

- record types;
- enum types;
- variant types;
- `Option<T>`;
- `List<T, max=N>`;
- `Map<K, V, max=N>`;
- `CapabilityRef<T>` (inert capability-receipt reference; see Capability
  References);
- imported shape/lawpack/target types.

A list literal `[a, b, c]` is checked against an expected `List<T, max=N>`
(`EDICT-LANG-LIST-LIT-001`): a binding annotation, a parameter, a record field
or a return type. It has at most `N` elements, each checked against `T` (so bare
integer literals take `T`'s width), in source order; order and duplicates are
preserved and hash-significant. `[]` is the empty list. A list literal with no
expected list type, a non-list expected type, or more than `N` elements
rejects. It lowers to Core `{ kind: "list", values: [...] }`, and Target
independently checks the element count against the declared maximum and every
element against the item type.

Maps must use canonical key ordering. Lists and maps must declare finite
maximum cardinality. Map key types in v1 are limited to scalar, enum, digest,
bytes, string with declared canonicalization policy, or imported types that
explicitly declare canonical map-key semantics. Target references, capability
references, closures, records, lists, maps, and variants are rejected as map
keys in v1.

A `String` map key must carry an explicit canonicalization policy (see Refined
Scalar Types, e.g. `String<max=128, canonical=nfc>` or a `type` alias of that
form). An imported type is a legal map key only when its source profile or
lawpack declares canonical map-key semantics for it via the relevant ABI (see
[SPEC - Edict Lawpack ABI v1](./SPEC_edict-lawpack-abi-v1.md) and
[SPEC - Edict Target Profile ABI v1](./SPEC_edict-target-profile-abi-v1.md)).

Recursive value types are rejected in v1 unless every recursive path carries an
explicit maximum depth in the resolved source-profile or lawpack facts. Imported
GraphQL input/object cycles without a digest-locked depth bound cannot enter a
lawful-autonomous locked bundle.

## Expressions

Edict expressions are pure unless the expression is an imported effect call
in A-normal effect position inside an intent body. Pure expressions may:

- construct records, variants, lists, and maps;
- read local variables and operation inputs;
- access fields;
- call pure functions;
- call deterministic standard functions;
- compare values;
- hash canonical values;
- branch with `if then else`.

Pure expressions may call **pure** target/lawpack intrinsics — the inert,
hashable plan constructors whose `intrinsicClass` is `pure` and `writeClass` is
`none` (e.g. `echo.ref<T>(id)`, `echo.edge<...>(...)`, id/plan constructors). A
pure constructor allocates no runtime handle and observes no state; it only
produces a canonical plan term.

Source calls an admitted pure intrinsic as `alias.name(args)` through an explicit, digest-locked target import, `use target echo.dpo@1 digest "sha256:..." as echo;`, whose resource equals the intrinsic's pinned target profile; the call lowers to a Core call whose callee is the intrinsic's canonical coordinate, with no type arguments (`EDICT-TARGET-PURE-INTRINSIC-001`). Each argument is checked against the contract's ceiling type and may be narrower. The compiler charges the call from the operand bounds it established, and Target independently resolves the call against the same pinned facts, rechecks every argument and the return type, recomputes the cost from bounds it establishes itself, and refuses an intent whose intrinsic work exceeds its step budget. An intrinsic call inside a loop body is refused by Target in v1. Computing a value with a pure intrinsic grants no authority to publish or mutate anything.

Pure expressions may not:

- call target/lawpack **effect** intrinsics (any `intrinsicClass: effect`) or
  lawpack semantic effects;
- observe runtime state;
- allocate runtime handles;
- call host callbacks;
- use reflection;
- perform IO.

## Statements

Intent bodies are statement-oriented so effect ordering is explicit. The v1
statement set is deliberately small:

- `let` binds an immutable local.
- `assert` records a compiler/verifier proof obligation.
- `require` records a runtime application precondition.
- `guarantee` records a postcondition proof or precommit check.
- `record` desugars to a portable semantic lawpack effect.
- `if` supports bounded branching.
- branch-yield `if` expressions support conditional effect values in `let`
  binding position.
- `for` supports finite bounded iteration.
- effect statements call target or lawpack intrinsics and may map effect
  failures with `else`.
- `return` emits the operation receipt/output value.

No assignment statement exists in v1. Locals are immutable. Accumulation must be
expressed through bounded folds supplied by deterministic standard functions or
through target/lawpack-provided profile functions with explicit semantics.

`assert` never becomes a runtime obstruction. If the compiler, lowerer, or
verifier cannot prove an `assert`, compilation or lowering fails.

`require` is evaluated atomically against the target application snapshot when
it refers to runtime state. Failure becomes the declared typed obstruction and
the atomic application produces no externally visible writes. Local prechecks
may be emitted for diagnostics, but they are not the authoritative enforcement
for mutable state.

`guarantee` is a postcondition. It must be proven by the compiler/verifier or
checked precommit inside the same atomic application unit. A guarantee may never
fail after externally visible commit. A precommit (runtime-checked) `guarantee`
lowers only to a target profile that declares `postconditionSupport`; otherwise
lowering fails with an unsupported-lowering error, never a silent downgrade
(`EDICT-TARGET-POSTCOND-001`).

A dynamic `require`/`guarantee` failure must be total: it resolves to a typed
domain obstruction, never a host exception (`EDICT-LANG-TOTAL-CHECK-001`). The
three constructs have non-overlapping roles (`EDICT-LANG-REQUIRE-ELSE-001`):

- **`where`** is pure input-domain refinement, checked before application;
  failure is the platform result `EDICT-INPUT-CONSTRAINT`; it never carries
  `else`.
- **`require`** is an operation/domain precondition. It **always** carries a
  typed `else` obstruction clause, whether its predicate is input-only or also
  depends on runtime state. An input-only `require` is allowed when
  domain-specific obstruction semantics are intended (e.g.
  `require input.startByte <= input.endByte else text.InvalidRange({...})`).
  Pure input *admissibility* belongs in `where`; universally provable facts
  belong in `assert`.
- **`guarantee`** is a postcondition. A precommit (runtime-checked) guarantee
  carries a typed `else`; a guarantee discharged by the verifier is a proof
  obligation (see CoreProofObligation under Edict Core IR), not a runtime guard,
  and carries no `else`.
- **`assert`** is always proof-only: it never carries `else`, never becomes a
  runtime obstruction, and fails compilation if it cannot be proven.

This removes the former "`require` without `else` if verifierProof" overlap:
`require` always has an obstruction; a thing that can be statically proven and
needs no obstruction is an `assert`.

## Where Clauses And Input Refinement

A `where` clause is an **input refinement**, not a runtime precondition and not a
compiler assumption (`EDICT-LANG-WHERE-001`). It constrains the operation's typed
input and is checked by a generated input validator **before** the atomic
application unit begins.

- A `where` predicate may reference only operation inputs and pure functions of
  them. It must not reference runtime/target state.
- Failure is a platform-owned **invocation rejection** with the stable result
  `EDICT-INPUT-CONSTRAINT`. It is not a domain obstruction and carries no `else`.
- Domain-specific checks against runtime state belong in `require` (with its
  typed obstruction), never in `where`.

Multiple `where` clauses merge conjunctively. The Core intent carries the
**typed predicate trees** themselves in `inputConstraints` (not merely a
validator coordinate), because the predicates are hash-significant: changing
`where input.repo != ""` to a different input predicate must change the semantic
Core hash (`EDICT-CORE-WHERE-HASH-001`). The generated input validator is a
derived artifact lowered from these trees; any validator coordinate lives in a
non-hash sidecar.

## Basis Expressions

A `basis` expression is evaluated in the **pure pre-body environment**
(`EDICT-LANG-BASIS-PURE-001`). It may reference intent parameters, constants,
`CapabilityRef` values, and total digest-locked pure functions over those
values. It must **not** reference target state, imported effects, effect
results, or locals bound in the intent body. A profile- or lawpack-supplied
basis template obeys the same restriction.

This protects the central doctrine: the basis is what the operation *says* it
depends on, not whatever the runtime happened to reveal after execution started.
Runtime reads may **validate or resolve** a declared basis (e.g. a stale-basis
guard); they do not **define** it.

## Boolean Evaluation

`&&` and `||` **short-circuit** (`EDICT-LANG-BOOL-001`). `a && b` does not
evaluate `b` when `a` is false; `a || b` does not evaluate `b` when `a` is true.
Short-circuiting is part of the semantics because it affects proof refinement,
cost accounting, and which sub-expressions are reached. Operands are pure
expressions; effects are never permitted inside a boolean operand (A-normal
effect form still applies; effectful calls appear only in `let`/effect-statement
position).

### Option Refinement

v1 defines exactly one refinement, not general dependent typing
(`EDICT-LANG-OPTION-REFINE-001`). `isSome(x)` positively refines `x` to present:

- in the right operand of `isSome(x) && ...`; and
- in the `then` branch of `if isSome(x) ...`.

`unwrap(x)` is legal **only** inside such a refined region, or after an
exhaustive `Option` `match`. Refinement is **lexical and variable-specific**: it
does not flow through arbitrary helper calls, aliases, merge points, or target
effects. There is no other narrowing in v1.

## Loops

`for x in collection bounded N` is the collection-loop form in v1
(`EDICT-LANG-LOOP-001`):

- v1 iterates `List<T, max=M>` only. `Map` iteration is deferred to a later
  version with an explicitly defined canonical entry sequence.
- Iteration order is list order.
- The compiler must prove `M <= N` (the collection's declared maximum is within
  the loop bound). If `M` cannot be statically proven `<= N`, the operation is
  rejected at compile time; it does not defer to a runtime check that could
  truncate.
- A runtime value that violates its locked Core type bound is **malformed**, not
  an ordinary budget exhaustion (`EDICT-LANG-BOUND-VIOLATION-001`). Input
  violations reject during input validation. A target/runtime-produced value
  violating its declared bound is an `integrityFault`; if caused by compiler,
  lowerer, verifier, or runtime implementation behavior, it is an
  `internalFault`. It is **never** silently truncated and is **never** an
  ordinary author-visible `resourceFault`. (Because `M <= N` is proven
  statically, this state is impossible by contract for conforming
  implementations; the classification covers defects, not normal operation.)
- Loop bodies obey normal A-normal effect rules; per-iteration effects retain
  ordering and the static maximum cardinality in Core IR (I-016).

### First-order bounded state fold

`fold(initial, step, N)` carries bounded state through a fixed number of pure
steps (`EDICT-LANG-FOLD-001`). This expression introduces neither mutable locals
nor function values. `step` must statically name a source-owned pure helper with
signature `S -> S`; parameter and return type coordinates must be identical.
S is a bounded scalar or aggregate type. The initial value follows ordinary
argument compatibility: records are structural, variants retain nominal identity,
and narrower value bounds may fit S. N must be a `U64`-suffixed literal in the
current compiler profile; named constants and lawpack coordinates refuse. Source
closure linking resolves local and imported step helpers hygienically. The
initial state is evaluated exactly once, then the helper is invoked
in order N times. N equal to zero returns the initial state but does not exempt
the helper from type checking, authority checks or cycle rejection.

Core binds the initial expression, static step coordinate, iteration bound and
state type in a `fold` expression requiring `core.fold/v1`. Consumers must
validate the capability and state-preserving signature independently. Missing
support is a refusal, never an unrolling fallback or silent zero-iteration
execution. Costs include the initial expression, every step's transitive work,
state validation/copy costs and per-iteration overhead. Overflow while
accumulating costs or multiplying the step cost by N refuses. Runtime ceilings may refuse before completion; a partial
accumulator is not a successful result.

There is no early exit. A terminal state is represented in S and its authored
step returns the same state without reads or effects; all N iterations still count.
Pure source helper rules already prohibit reads and effects, and `fold` does
not relax them. This is distinct from effect-emitting `for` over a list that
already exists. Generic provider/runtime execution of `core.fold/v1` remains a
separate implementation gate; this source/Core contract is not execution proof.

## Standard Functions

The Edict Core prelude is intentionally small:

- `fold(initial, step, N)` is the reserved first-order state-iteration form
  described above. It is not a function value. Source declarations, parameters
  and locals cannot shadow the prelude name `fold`.

- `hash(label: StringLiteral, value...) -> Digest`
- `canonicalEncode<T>(value: T) -> Bytes<max=CanonicalEncodedMax<T>>` (the
  result is statically bounded by the input type's canonical-encoded maximum; a
  naked unbounded `Bytes` return would violate `EDICT-LANG-BOUNDS-001`).
  `CanonicalEncodedMax<T>` is a compiler-derived type-level bound: the maximum
  `edict.canonical-cbor/v1` encoded byte length of any value of type `T`,
  computed structurally (`EDICT-LANG-ENCODEMAX-001`):
  - scalars: the fixed canonical-CBOR width for `Bool`/`I32`/`I64`/`U8`/`U32`/`U64`/
    `Digest`/`Unit`;
  - `String<max=N[,canonical=...]>`: CBOR text-header bytes + the maximum UTF-8
    byte length of `N` Unicode scalar values **after** the declared
    canonicalization;
  - `Bytes<max=N>`: CBOR byte-header bytes + `N`;
  - records: header bytes + the sum of each field's key encoding +
    `CanonicalEncodedMax<fieldType>`;
  - variants: header bytes + the tag encoding + the **maximum**
    `CanonicalEncodedMax` over all cases;
  - `Option<T>`: `max(CanonicalEncodedMax<T>, none-encoding)`;
  - `List<T, max=N>`: header bytes + `N * CanonicalEncodedMax<T>`;
  - `Map<K, V, max=N>`: header bytes + `N * (CanonicalEncodedMax<K> +
    CanonicalEncodedMax<V>)`;
  - imported types: from the source-profile/lawpack-declared bound.
  It is rejected for any `T` that is not fully bounded.
- `len(value) -> U64` (Unicode scalar count for `String`, byte count for
  `Bytes`, element count for `List`/`Map`; see Refined Scalar Types,
  `EDICT-LANG-LEN-001`)
- `isUtf8(bytes: Bytes<max=N>) -> Bool` validates a complete UTF-8 byte
  sequence (`EDICT-LANG-BYTE-UTF8-001`). Empty bytes and embedded NUL are valid;
  incomplete sequences, overlong encodings, surrogate encodings and code points
  above U+10FFFF are invalid. It neither normalizes nor repairs bytes. The
  source prelude takes one bounded structural byte operand and no authored type
  arguments; it lowers to `core.bytes.isUtf8<OperandType>(bytes)` with a Bool
  result. The exact operand coordinate is retained. String values and implicit
  nominal unwrapping are not accepted by this specialization. Compiler and
  Target support establish typing and authority, not runtime execution evidence.
  A prefix slice passed to this predicate still needs its own bounds proof.
  Direct scans contribute the operand byte maximum to the static intent step
  bound, including metadata predicates. Sequential scans add; lazy alternatives
  use the maximum branch cost. Source-helper summaries already include their
  scans and are not charged twice at the call site. Runtime metering also
  charges the selected profile's expression, value and validation work.
- `count(bytes: Bytes<max=N>, b"x") -> U64` counts the occurrences of one byte
  in a complete bounded byte sequence (`EDICT-LANG-BYTE-COUNT-001`). The needle
  is a byte literal that decodes to exactly one byte, so what the scan looks for
  never depends on runtime data; empty, multi-byte, string, integer and
  non-literal needles reject. It counts raw bytes and does not decode UTF-8. The
  source prelude takes one bounded structural byte operand and no authored type
  arguments; it lowers to `core.bytes.count<OperandType>(bytes, b"x")` with a
  U64 result, retaining the exact operand coordinate and the counted byte, both
  hash-significant. The result is at most `N`, and unsigned addition proofs use
  that bound, so `count(bytes, b"\n") + 1u64` is total for any `N` below the
  U64 maximum. A proven unsigned bound travels with a `let` binding to later
  statements of the same intent or function body, in both the compiler and
  Target; it does not travel through helper results or record fields, so a
  caller computes derived metrics such as `lineCount` beside the count. A direct scan adds `N` steps to the static bound, and a helper
  scan is charged once, exactly as for `isUtf8`. Compiler and Target support
  establish typing, authority and bounds, not runtime execution evidence.
- `at(bytes: Bytes<max=N>, index: U64) -> U8` reads exactly one raw byte
  (`EDICT-LANG-BYTE-AT-001`). It lowers to
  `core.bytes.at<OperandType>(bytes, index)`, retaining the exact operand
  coordinate and expression. The index must be strictly below the operand's
  actual length, established by a dominating comparison or, for a constant
  index, a declared minimum length. A maximum, `index <= len(bytes)`, an
  unrelated operand's length, or a later guard cannot establish this proof.
  The `if ... then ... else ...` expression adds its selected arm predicate
  truth to enclosing proof facts. Statement-level `if`, branch-yield blocks and
  `require` do not establish lookup proofs.
  A later `&&` operand sees
  earlier operands as true; a later `||` operand sees earlier operands as false.
  No such fact escapes the arm or lazy operand. Target independently rechecks
  lookup typing and totality in caller-supplied Core. This adds no implicit
  integer conversion, no missing-byte sentinel and no text-specific behavior.
  `at` is a protected prelude spelling, so rename any local binding with that
  name. The compiler establishes a U8 result and bounds proof; provider/runtime
  execution is a separate acceptance gate.
- `slice(bytes: Bytes<max=N>, start: U64, end: U64) -> Bytes<max=N>`
  selects the half-open byte range `[start,end)` only when the compiler proves
  `start <= end <= len(bytes)` (`EDICT-LANG-BYTE-SLICE-001`). The bytes remain
  raw: slicing does not decode UTF-8, normalize text, clip invalid endpoints,
  or supply missing bytes. The result drops the operand's positive minimum or
  exact-length refinement; it may be empty. The source call has three arguments
  and no authored type arguments.
- `some<T>(value: T) -> Option<T>`
- `none<T>() -> Option<T>`
- `default<T>(value: Option<T>, fallback: T) -> T`
- `isSome(value) -> Bool`
- `unwrap(value) -> T` only after proof that an option is present

The minimal-v1 prelude operation set is **closed** (`EDICT-LANG-PRELUDE-001`).
Unlisted prelude operations do not exist in v1.

```text
Integer (I32/I64/U32/U64):
  + - * / %               # only when ranges prove no overflow
  unary -
  == != < <= > >=
  checkedAdd, checkedSub, checkedMul, checkedDiv, checkedRem -> Option<T>
  no bitwise operators in v1
  signed / and % truncate toward zero; remainder takes the dividend's sign
U8:
  == != < <= > >=
  # Value and comparison support; arithmetic is not implemented for U8.
String:
  +                       # bounded concatenation (see Refined Scalar Types)
  == !=                   # over the exact canonical scalar sequence
  ordered comparison only if the type pins a canonicalization/collation profile
  no slice, split, trim, case-fold, locale, or regex helper in minimal-v1
Bytes:
  +                       # bounded raw-byte concatenation
  == !=
  slice(bytes, start, end) # proven, bounded half-open byte range
  no implicit string conversion
```

Unchecked signed division/remainder is permitted only where overflow and
divide-by-zero are statically excluded; otherwise use the `checked*` forms.

### Integer Safety

All integer arithmetic in the lawful-autonomous lane is overflow-safe and total
(`EDICT-LANG-INT-SAFETY-001`):

- A bare `+`, `-`, `*`, `/`, `%`, or unary `-` is accepted only if the compiler
  proves, from declared scalar widths and value refinements, that it cannot
  overflow or wrap. An unproven operation rejects at compile time.
- Otherwise authors use the `checked*` forms, which return `Option<T>` and force
  the caller to handle the `none` case; the unhandled result cannot be unwrapped
  without proof.
- Division and remainder by zero must be statically excluded or use a `checked*`
  form; an unproven divisor rejects.
- Signed `/` and `%` truncate toward zero, with the remainder taking the
  dividend's sign.
- There is no wrapping, saturating, or trapping arithmetic in v1. Overflow is
  never a silent or runtime-panicking outcome; it is a compile-time rejection or
  a typed `Option` the author must handle.

The current source-to-Core compiler supports a conservative unsigned subtraction
slice. Operands must have the same `U32` or `U64` type. It accepts `x - x`,
`x - 0`, ordered literals, and differences whose exact operands are ordered by
an intent input constraint (`<=`, `<`, `>=`, `>`, or `==`). Conjunctive clauses
can supply this evidence; disjunction and negation cannot. The compiler does
not infer transitive relationships or propagate aliases for this proof.

The resulting Core call is `core.integer.subtract<T>(minuend, subtrahend)`;
width, operand order, and input predicates remain explicit and hash-significant.
Input constraints justify body arithmetic only: they cannot justify evaluation
of the basis or of the constraints themselves. Each intent has an isolated
proof scope. Target lowering and verification must independently support and
validate this operation before accepting executable output. This compiler
slice does not imply support for the other arithmetic forms specified above.

Every prelude function must be total over valid input or must expose a typed
diagnostic that the compiler can force authors to handle.

The source-to-Core compiler also supports same-width `U32`/`U64` addition as
`core.integer.add<T>(left, right)`. It composes literal and nested-sum upper
bounds and literal bounds on the exact operands from dominating conjunctive
input comparisons. Both comparison directions and equality can establish a
bound; disjunction and negation cannot. A sum is accepted only when its proved
upper bound fits the width, including the exact maximum. Width inference does
not require a `let` annotation when an operand determines the unsigned type.
The Target IR boundary independently checks signatures and totality over raw
Core and rejects removed or weakened proofs. This source/Target slice does not
establish provider or runtime support for the new call.

The implemented byte concatenation accepts two bounded structural byte operands
and preserves their order in `core.bytes.concat<LeftType,RightType>(left,right)`
(`EDICT-LANG-BYTE-CONCAT-001`). The result has the conservative bound
`Bytes<max=L+R>`, using the operand maxima even when exact or minimum lengths are
known. A static sum beyond U64 rejects with `InvalidBound`; incompatible operand
families and narrower destinations reject with `TypeMismatch`. There is no
implicit conversion from strings or nominal wrappers. Target independently
checks signature, exact operand coordinates, local authority, checked sum and
result compatibility. Compilation or provider acceptance does not prove runtime
byte-copy budget compliance or execution support for this call.
The [canonical lowering decision](topics/target-ir/README.md#byte-concatenation-boundary)
records the implementation relationships for this language rule.

The implemented byte-slice proof uses exact operands in conjunctive intent
input constraints, identical endpoints, unsigned zero lower bounds, and ordered
unsigned literals. The upper endpoint must be ordered against `len` of the
same byte expression and exact operand coordinate. It does not infer alias,
transitive, negated, or disjunctive relationships. A positive expression-level
`if` arm can also supply the required comparisons; its predicate does not prove
the opposite arm or a later expression. Statement-level `if` and `require` do
not extend this proof scope. Separate input
constraints form a canonical set and cannot justify one another or the earlier
basis. Within an ordered `&&` predicate, the checked left predicate establishes
positive proof for evaluation of its right predicate, including nested
conjunctions (`EDICT-LANG-GUARD-ORDER-001`). A predicate cannot use its own or
a later predicate's facts. These temporary facts do not escape that conjunction.
After all input constraints hold, they justify body evaluation.
An unproven slice rejects before Core with `UnsupportedSourceShape`; wrong
operand families or endpoint widths reject with `TypeMismatch`.

Core retains the call as `core.bytes.slice<OperandType>(bytes,start,end)` and
Target independently validates its signature, authority, result bound, and
evaluation-scope proof. The existing Call encoding carries the operation;
existing source and artifact identities without it do not change. Public
`project` may expose Core and Target review artifacts, while application
`build` additionally crosses provider packaging and verification. A provider may
preserve the call in a package and accept its artifact relation without a
consumer evaluator implementing slice execution. Neither successful build,
accepted report, nor a type maximum proves runtime budget compliance: the
declared Core budget remains explicit, and the evaluator must independently
implement byte-copy charging and input-constraint enforcement.

`hash` is a source-level helper, not the artifact hash primitive. The label must
be a string literal so digest domains are stable and reviewable.

## External-Action Requests

Edict expresses an external boundary crossing as deterministic data, not as an
effect performed by the compiler, lowerer, provider, or model
(`EDICT-LANG-EXTERNAL-REQUEST-001`).

A `use capability` import binds one digest-locked, domain-specific external
operation family. Its alias is legal only as the single-argument operation call
inside an `external-action-request-stmt`. It is not a semantic-effect import and
cannot be invoked from `let` or effect-statement position. Raw filesystem,
process, network, Git, GitHub, model, and shell operation families are not
requestable language capabilities.

Every request carries:

- an input value and its exact schema;
- an exact settlement schema;
- runtime-valued authority scope and basis expressions;
- runtime-valued maximum settlement bytes and attempt bounds;
- an exact reconciliation law;
- a typed `ExternalActionRequest<Settlement>` result.

Core and Target IR preserve the request as a separate node/collection with
fixed `awaitingSettlement` and `schemaRequired` posture. The exact operation
resource remains in the digest-bound capability closure. Target lowering must
not translate the request into a callable target intrinsic or provider import.

Execution is outside Edict. Echo admits a durable request before an authorized
adapter touches the world, admits a schema-valid settlement afterward, and
resumes deterministic interpretation from history. Waiting is explicit program
state keyed by request identity; no native stack, continuation, callback, or
`async` host frame is serialized. Runtime path, ref, basis, budget, adapter, and
settlement checks fail closed at Echo admission rather than pretending dynamic
facts are compile-time properties.

## Effects And Footprints

An effect is a typed, imported operation emitted by a target intrinsic or
lawpack semantic effect. Core IR represents effects abstractly:

```text
Effect {
  authority: TargetProfileRef | LawpackRef,
  intrinsic: CanonicalIntrinsicCoordinate,
  typeArgs: [CoreType],
  args: [CoreExpr],
  effectKind:
    read | create | ensure | replace | delete | append | reduce |
    semantic.emit | custom,
  guards: [CoreGuard],
  obstructionMap: ObstructionMap,
  footprintTemplate: FootprintTemplateRef,
  costTemplate: CostTemplateRef
}

# A CoreGuard is always an atomic runtime guard and always carries an
# obstruction (EDICT-CORE-GUARD-CATEGORY-001). Verifier/compiler proofs are NOT
# guards — they are CoreProofObligation nodes. Local prechecks are diagnostic
# sidecars, not Core truth.
CoreGuard {
  predicate: CorePredicate,
  enforcement: targetAtomic,
  obstruction: ObstructionConstruct
}

CoreProofObligation {
  predicate: CorePredicate,
  origin: assert | guarantee
}

# A hash-significant obstruction construction: the coordinate, an optional
# binder for the low-level failure, and a typed payload expression tree (an
# empty record when the obstruction has no payload). This is what lets a runtime
# guard such as `require currentHead == expectedHead else
# rope.StaleBaseHead({ expected, observed })` round-trip through Core
# (EDICT-CORE-GUARD-PAYLOAD-001).
ObstructionConstruct {
  coordinate: ObstructionCoordinate,
  failureBinder: LocalName?,
  payload: CoreExpr            # typed record expression; {} if empty
}

# ObstructionMap entries carry the same ObstructionConstruct, keyed by the
# effect's named failure coordinate.
ObstructionMap = Map<FailureCoordinate, ObstructionConstruct>
```

The target profile or lawpack defines the footprint algebra and cost template.
Core IR records the authority, intrinsic coordinate, resolved argument
expressions, effect kind, guard predicates, obstruction mappings, and
authority-owned metadata. Structured Core branches, loops, and closure nodes
carry branch conditions and cardinality bounds. Flattened path conditions and
effective cardinalities are derived effect-analysis artifacts.

Examples:

```amblicit
let basisValue = echo.ref<StructuralBasis>(input.basisId).read()
  else history.StructuralBasisMissing;
```

may infer an Echo graph read footprint:

```text
target: echo.dpo@1
reads:
  node StructuralBasis[input.basisId]
```

```amblicit
log.stream("history").append<HistoryEntryRecorded>(event);
```

may infer an EventLog append footprint:

```text
target: eventlog.append@1
writes:
  append stream history event HistoryEntryRecorded
```

```amblicit
let entry = history.entry.record(event);
```

may infer a lawpack semantic effect:

```text
authority: history.optics@1
effectKind: semantic.emit
coordinate: history.optics@1.entry.record
```

## Target Profiles

A runtime target profile declares the imported effects that Edict Core may use
and the target-owned rules for lowering those effects. The normative ABI is
defined in
[SPEC - Edict Target Profile ABI v1](./SPEC_edict-target-profile-abi-v1.md).
A target profile must at least declare:

- profile identity, version, and digest;
- accepted Edict Core ABI versions;
- target intrinsic namespace;
- type signatures for every intrinsic;
- effect signatures for every intrinsic;
- footprint algebra;
- target IR schema;
- verifier ABI;
- obstruction taxonomy;
- bundle profile;
- generated artifact profiles;
- canonical encoding rules;
- normative conformance fixture corpus;
- lowerer plugin ABI and digest requirements;
- verifier plugin ABI and digest requirements;
- sandbox/runtime identity;
- deterministic execution constraints;
- fuel and cost model;
- atomic application model.

Target profile manifests should be data. Target lowerers and verifiers may be
locked executable components, but their digests, ABI, sandbox identity,
determinism constraints, fuel model, and conformance fixtures must be explicit.
The manifest states the contract; the lowerer and verifier code satisfy it.

The canonical target profile manifest, its full field set, and the executable
lowerer/verifier plugin boundary are defined **once** in
[SPEC - Edict Target Profile ABI v1](./SPEC_edict-target-profile-abi-v1.md)
and its machine schemas (`abi/edict-target-profile.cddl`,
`abi/edict-target-provider.wit`). This document intentionally does not duplicate
the manifest JSON; duplicate normative manifests are forbidden
(`EDICT-ABI-NODUP-001`).

Display metadata is not part of the hashed target profile manifest. It belongs
in a sidecar keyed by the target profile digest.

Third-party conformance execution results are external attestations referencing
the target profile digest. They are not components of the target profile
identity.

## Profile Vocabulary

Source examples use compact profile names, but Core must split the concepts:

```text
operationMode:
  readOnly | createOnly | replace | append | custom

targetProfile:
  echo.dpo@1 | kv.transactional@1 | ...

lawProfile:
  history.optics@1.recordEntry | ...
```

A source clause such as:

```amblicit
profile echo.createOnly
```

is a claim that resolves into structured Core fields. It is not proof. The
compiler verifies the claim against inferred effects, target profile rules,
lawpack rules, footprint bounds, and cost bounds.

Bundle profile, assurance lane, admission class, participant policy, and
admission evidence are packaging and admission fields. They are not
hash-significant Edict Core operation semantics.

Operation modes are verifier predicates over inferred effects. The **normative**
definition of each predicate lives in the Operation Mode Predicates section of
[SPEC - Edict Target Profile ABI v1](./SPEC_edict-target-profile-abi-v1.md); the
summary below mirrors it for convenience and must not diverge
(`EDICT-OPMODE-AUTHORITY-001`):

- `readOnly`: effects that are **either** proof-only semantic facts **or** have
  authoritative `writeClass` `read` (including runtime semantic reads); no
  mutating `writeClass`.
- `createOnly`: read, create, and ensure effects; no replace, delete, append,
  or runtime-materialized semantic writes except effects the resolved profile
  explicitly classifies as create.
- `append`: read and append effects; no replace or delete.
- `replace`: read, create, ensure, and replace effects as profile-defined; no
  delete unless the profile explicitly classifies the operation mode as custom.
- `custom`: target-profile-defined predicate with a digest-locked verifier rule.

`ensure` counts as read plus conditional create for cost and obstruction
analysis, but it is compatible with `createOnly` when the target profile
declares idempotent ensure semantics.

## Edict Core IR

Edict Core IR is the canonical compiler output before target lowering. It is
not source syntax and not target IR.

> [!NOTE]
> [`abi/edict-core.cddl`](./abi/edict-core.cddl), assembled with
> [`abi/edict-common.cddl`](./abi/edict-common.cddl), is the normative
> `edict.core/v1` wire schema. This section is the normative semantic contract
> for constraints that CDDL's `tstr` cannot express. The reference encoder and
> reviewed Core goldens are executable conformance evidence
> (`EDICT-CORE-EXPR-CDDL-001`).

### Core Module Shape

```cddl
core-module = {
  apiVersion: "edict.core/v1",
  coordinate: tstr,
  imports: [* core-import],
  types: { * core-named-type-ref => core-type },
  intents: { + tstr => core-intent },
  requiredCoreCapabilities: [* tstr],
}
```

The module `coordinate` is its nonempty semantic identity. Imports are exact
resource references. The `types` map contains named definitions only; its key
restriction is defined below. Intents are keyed by their module-local names.

A publicly constructible raw `CoreModule` is an untrusted candidate, not proof
that the module satisfies Core type semantics. The authoritative whole-module
type-integrity judgment validates the complete candidate and, on success,
mints an opaque `ValidatedCoreModule` witness borrowing those exact immutable
contents. Canonical encoding and digesting, Target lowering, and
result-projection emission or verification MUST obtain that witness before
they can mint or verify an artifact.

### Core Type Reference Identity

Every `core-type-ref` MUST classify without consulting mutable module state as
exactly one of these identity classes:

1. **Intrinsic** — a fixed Core coordinate: `Unit`, `Bool`, or one of `I8`,
   `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, and `U64`. Its definition is built
   into the Core ABI.
2. **Structural** — canonical constructor syntax whose complete meaning is
   encoded by the reference itself. Structural references are parsed, not
   looked up.
3. **Named** — a nonempty canonical type coordinate whose exact definition is
   supplied by the owning module or authenticated lawpack.

A malformed, noncanonical, or over-depth string is not a fourth class; it is an
invalid Core type reference and MUST reject. The canonical structural grammar
is:

```ebnf
core-type-ref = intrinsic-ref | structural-ref | named-ref ;

intrinsic-ref = "Unit" | "Bool"
              | "I8" | "I16" | "I32" | "I64"
              | "U8" | "U16" | "U32" | "U64" ;

structural-ref = string-ref | bytes-ref | record-ref | option-ref
               | list-ref | map-ref | capability-ref | external-request-ref ;

string-ref = "String<max=" uint ",canonical=" string-canonical ">" ;
string-canonical = "raw-utf8" | "unicode-scalar-nfc" ;
bytes-ref = "Bytes<max=" uint ">"
          | "Bytes<exact=" uint ">"
          | "Bytes<min=" uint ",max=" uint ">" ;
record-ref = "Record<" [ record-field { "," record-field } ] ">" ;
record-field = field-ident ":" core-type-ref ;
option-ref = "Option<" core-type-ref ">" ;
list-ref = "List<" core-type-ref ",max=" uint ">" ;
map-ref = "Map<" core-type-ref "," core-type-ref ",max=" uint ">" ;
capability-ref = "CapabilityRef<" core-type-ref ">" ;
external-request-ref = "edict.external-action.request/v1<" core-type-ref ">" ;

uint = "0" | nonzero-digit { digit } ;
field-ident = ( letter | "_" ) { letter | digit | "_" } ;
named-ref = named-ref-char { named-ref-char } ;
digit = ? an ASCII digit from "0" through "9" ? ;
nonzero-digit = ? an ASCII digit from "1" through "9" ? ;
letter = ? an ASCII letter from "A" through "Z" or "a" through "z" ? ;
named-ref-char = ? any Unicode scalar other than whitespace, "<", ">", ",", ":", or "=" ? ;
```

`uint` uses canonical unsigned decimal spelling: `0` or a nonzero digit followed
by digits, with no leading zero. A `Bytes<min=M,max=N>` reference requires
`M <= N`; equal bounds render as `Bytes<exact=N>`. Record field names use the
Edict identifier shape and MUST appear once each in ascending canonical field
order. Structural children recurse under the same rules, with a maximum type
reference depth of 128. For every accepted intrinsic or structural reference
`x`, rendering its parsed Core type MUST reproduce `x` byte-for-byte.

The depth limit applies to the fully expanded acyclic type graph, not only to
the constructor text visible at one reference site. Let `H(x)` be the maximum
additional structural height below `x` after recursively resolving named
definitions. Scalars have height zero. A named reference has the height of its
definition and consumes no edge itself. `Option`, `List`, `CapabilityRef`,
`ExternalActionRequest`, and nominal representation each add one to their
child's height; `Map` adds one to the greater child height. A nonempty `Record`
adds one to its greatest field height, while an empty record has height zero. A
`Variant` with payloads adds one to its greatest payload height, while a wholly
payloadless variant has height zero. All additions MUST be checked.

An occurrence beginning at depth `d` is valid exactly when `d + H(x) <= 128`.
Consequently, depth 128 is valid and depth 129 rejects. Resolving a named
definition MUST NOT reset the budget. Memoization MAY cache a named
definition's context-free `H` value by its resolved `core.types` key, but MUST
apply the occurrence depth separately; it MUST NOT cache a context-dependent
success verdict. Cycle detection remains a distinct judgment, and validity
MUST be independent of definition order, signature-root order, prior shallow
uses, and relative versus module-qualified spelling of the same resolved key.

A `named-ref` additionally excludes every intrinsic spelling, the reserved bare
constructor names, `anonymous.record`, and every string that begins like a
reserved structural constructor but fails its canonical grammar. Those strings
are invalid references rather than names.

Intrinsic and structural references MUST NOT appear as keys in `core.types` and
MUST NOT be copied into authenticated type-closure maps. A `core.types` key is a
`core-named-type-ref`; attempts to shadow or redefine intrinsic or structural
identity reject before canonical Core bytes or Target artifacts exist. Named
records and other named structural definitions remain valid: their key carries
provenance, while the referenced children carry either structure or further
names.

The whole-module type-integrity judgment eagerly validates every named
definition, including valid unused authored definitions that intentionally
remain hash-significant. It validates each supported scalar invariant, record
field and variant shape, composite child reference, nominal contract equality,
named resolution, recursion depth, and the v1 cycle policy. It also traverses
every type reference carried by intents, complete local tables, producer and
obstruction binders, expressions and call type arguments, external requests,
predicates, reasons, nested branches and loops, and result expressions. A
malformed unused definition is still part of the Core digest preimage and does
not escape this judgment merely because Target execution cannot reach it.

Compiler-produced `core.types` entries consist only of independently authored
local named definitions and exact authenticated imported named definitions.
Field positions are represented inside their owning record definition; a
compiler-created `Type.field`, `effect.failure`, `anonymous.record`, or other
scratch coordinate is not a Core named definition and MUST NOT enter canonical
Core identity. The parent definition and its canonical child references carry
the complete field meaning.

Nominal contracts are named identities. A nominal definition's `contract` MUST
equal its table key, and its representation is another `core-type-ref`. A
structural constructor nested inside a named definition is traversed
transparently. An authenticated signature closure contains exactly every
reachable named definition, recursively, and contains no intrinsic or structural
entry. Each named lawpack coordinate in that closure MUST be below the exact
digest-locked owning lawpack coordinate and match the caller's Core definition.

`requiredCoreCapabilities` is a hash-significant Core module field: the set of
non-minimal Core constructs the module relies on (e.g. variants, maps, bounded
recursive imports), inferred from the Core itself. It is in the Core preimage so
a lowerer/verifier has an authoritative in-Core flag and a module cannot alter
its capability set without changing the Core digest
(`EDICT-LANG-CAPABILITIES-SPLIT-001`). `requiredSourceCapabilities` is **not**
here — it is source-profile/release metadata, checked by the compiler.

`edict.canonical-cbor/v1` identifies the codec used to encode this module, but it
is a protocol constant rather than a Core module field. A Core artifact must not
contain its own self-hash (`EDICT-CORE-SELFHASH-001`). The Core IR digest is
computed over the canonical module and published in an external resource
descriptor (for example, the contract bundle's `coreIrDigest`), never embedded
in the Core module itself.

The raw source artifact digest is bundle provenance only. It is intentionally
excluded from the Core IR hash preimage so comments, formatting, file paths,
source-map locations, and other nonsemantic source changes do not alter the
Core IR digest. Lowerer and verifier component digests are bundle fields, not
Core fields: a different conforming lowerer must not change the semantic Core IR
digest (`EDICT-CORE-NOPACKAGING-001`).

### Core Intent Shape

```cddl
core-intent = {
  input: core-type-ref,
  output: core-type-ref,
  requiredOperationProfile: tstr,
  ? basis: core-expr,
  inputConstraints: [* input-constraint],
  coreEvaluationBudget: core-budget,
  body: core-block,
  ? optic: core-optic,
}
```

Every present Core intent field is hash-significant: input and output type
references, the `requiredOperationProfile` requirement, optional typed basis,
typed `inputConstraints` predicate trees, Core evaluation budget, body, and the
optional optic contract. Target authorities are module imports; law profiles,
target budgets, verifier conclusions, and packaging metadata belong to their
own ABIs rather than the Core intent. The module-level
`requiredCoreCapabilities` field is also hash-significant. The following are
**not** in the Core intent preimage:

- `verifiedOperationMode` — a verifier-report field, not a Core claim. Core
  states the requirement; the verifier proves the verdict
  (`EDICT-CORE-VERIFIED-EXTERNAL-001`).
- `preconditions` / `postconditions` — derived indices over the body's
  `require`/`guarantee` nodes. The body nodes are authoritative; these flat
  arrays are derived analysis and excluded from the preimage
  (`EDICT-CORE-NODUP-PREPOST-001`).
- `diagnosticPolicy` — a compile option / diagnostic sidecar selector. Repair
  wording is not operation law (`EDICT-CORE-NODIAG-001`).

### Optic Contract

Per I-031, every intent carries a minimal normative optic contract in Core. Each
field has exactly one deterministic source of truth
(`EDICT-OPTIC-SOURCE-001`):

| Core field | How it is produced |
| --- | --- |
| `basis` | the `basis` source clause (a typed Core expression, or `none`), or a digest-locked profile/lawpack basis template |
| `opticKind` (`revelation`/`affectReintegration`) | the resolved operation-profile **optic template** (not author free-text) |
| `boundaryKind` (`projection`/`affect`) | the resolved operation-profile optic template |
| `apertureRequirement` | a typed reference: `footprintCeiling` ref or `abstractFootprintObligation` ref |
| `supportPolicy` | a canonical profile **coordinate** (e.g. `continuum.support.carry-or-obstruct/v1`), never a free-form string |
| `lossDisposition` | a canonical profile **coordinate** (e.g. `continuum.support.reject-on-loss/v1`), never a free-form string |

The **operation-profile optic template** is the owner of `opticKind`,
`boundaryKind`, `supportPolicy`, `lossDisposition`, and an optional basis
template. Its single canonical shape (`operation-profile` / `optic-template`) is
defined in [`abi/edict-common.cddl`](./abi/edict-common.cddl) and may be exported
by target profiles and lawpacks; the normative ABI surface is
[SPEC - Edict Target Profile ABI v1](./SPEC_edict-target-profile-abi-v1.md) and
[SPEC - Edict Lawpack ABI v1](./SPEC_edict-lawpack-abi-v1.md)
(`EDICT-OPTIC-TEMPLATE-OWNER-001`).

`apertureRequirement` is a **typed reference** (`footprintCeiling` or
`abstractFootprintObligation`), not a free-form string
(`EDICT-OPTIC-APERTURE-REF-001`). Where an example shows a bare coordinate, that
is the review rendering of the typed reference, not a contradictory string.

The `basis` value in Core is a typed expression tree, not the review rendering
`"input.basisId"`. Richer Observer Geometry evidence — support
carried/lost/blocked/refuting, degeneracy findings, witness debt, footprint
overlap — is **derived verifier evidence** (the Aperture Ledger), not Core
syntax, until the support algebra is pinned. A lowering may not silently erase
support loss, degeneracy, footprint overlap, or witness debt: it must reject or
record the loss in derived evidence (`EDICT-OPTIC-PRESERVE-001`).

### Core Body Shape

Core is ANF/SSA-like. It preserves ordered lets, effect nodes, structured
blocks, branch predicates, loop bounds, obstruction edges, and return
construction.

```json
{
  "kind": "block",
  "nodes": [
    {
      "kind": "let",
      "id": "%0",
      "debugName": "insertBytes",
      "expr": {
        "call": "jedit.rope@1.encodeText",
        "args": ["%input.insertText"]
      }
    },
    {
      "kind": "if",
      "predicate": {
        "op": "!=",
        "left": { "call": "core.len", "args": ["%0"] },
        "right": { "u64": "0" }
      },
      "result": "%1",
      "debugName": "newBlob",
      "then": {
        "kind": "block",
        "nodes": [
          {
            "kind": "effect",
            "bind": "%2",
            "debugName": "blob",
            "authority": "echo.dpo@1",
            "intrinsic": "echo.dpo@1.ref.ensure",
            "effectKind": "ensure",
            "guards": [],
            "obstructionMap": {
              "mismatch": {
                "coordinate": "jedit.rope@1.TextBlobHashConflict",
                "failureBinder": "fault",
                "payload": { "observed": { "field": "fault.existingContentHash" } }
              }
            },
            "costTemplate": "echo.dpo@1.cost.ensure-node",
            "footprintTemplate": "echo.dpo@1.footprint.ensure-node"
          },
          {
            "kind": "yield",
            "expr": { "option": "some", "value": "%2" }
          }
        ]
      },
      "else": {
        "kind": "block",
        "nodes": [
          {
            "kind": "yield",
            "expr": { "option": "none", "type": "shape.TextBlob" }
          }
        ]
      }
    },
    {
      "kind": "for",
      "item": "%leaf",
      "collection": "%rewrite.newLeaves",
      "bound": "jedit.rope@1.maxCreatedLeaves",
      "body": {
        "kind": "block",
        "nodes": [
          {
            "kind": "effect",
            "authority": "echo.dpo@1",
            "intrinsic": "echo.dpo@1.ref.create",
            "effectKind": "create",
            "guards": [],
            "obstructionMap": {
              "conflict": {
                "coordinate": "jedit.rope@1.RopeLeafAlreadyExists",
                "failureBinder": null,
                "payload": {}
              }
            }
          }
        ]
      }
    }
  ]
}
```

Hash-significant Core predicates are typed expression trees, not strings.
`debugName` fields and source local names are illustrative/review metadata and
must either be excluded from the authoritative hash input or stored in a
non-hash diagnostic sidecar.

### Canonicalization Rules

Core IR canonicalization must:

- sort declarations by canonical coordinate where source order is not semantic;
- preserve statement order where effect order is semantic;
- encode typed integers by width and signedness;
- encode strings in the declared normalization form;
- encode bytes as bytes, not strings;
- encode maps by canonical key order;
- resolve aliases to canonical coordinates;
- alpha-normalize local binders and SSA ids;
- encode predicates as typed Core expression trees;
- remove comments and formatting;
- remove source locations from hash input;
- include source locations in sidecar diagnostic maps;
- include all imported lawpack/profile digests (effects depend on them);
- include the module-level `requiredCoreCapabilities` set;
- exclude lowerer and verifier component digests: they are contract-bundle
  fields, not Core semantics (`EDICT-CORE-NOPACKAGING-001`);
- exclude `verifiedOperationMode`, flat `preconditions`/`postconditions`
  indices, and `diagnosticPolicy` from the preimage;
- exclude human-readable `name` fields where a canonical `coordinate` is present;
  the `coordinate` is authoritative (I-009), and the `name` is review metadata;
- fail if any imported digest is unresolved.

The Core intent preimage **includes**, positively and exhaustively
(`EDICT-CORE-PREIMAGE-LIST-001`):

- `coordinate`;
- input and output types;
- the optic contract;
- `requiredOperationProfile`;
- `targetAuthorities`;
- `lawProfiles`;
- the `implements` coordinate;
- the typed `inputConstraints` predicate trees;
- `coreEvaluationBudget`;
- `targetBudget` (both the digest-locked `costAlgebra` reference and the
  resolved typed `ceiling`);
- the authoritative structured body (including `require`/`guarantee` nodes); and
- all semantic import/profile/lawpack digests.

A reader must not have to reconstruct the preimage by scavenging the document:
this list and the exclusion list above are jointly authoritative.

The authoritative hash input is `edict.canonical-cbor/v1` unless a later spec
supersedes it. `edict.canonical-json/v1` is a review/debug rendering and must
not be used as the authority for full-width typed integers unless every scalar
is explicitly wrapped.

## Wesley Compilation To IR

Wesley should compile Edict through a source-profile boundary, not by adding
Continuum or Echo policy to `wesley-core`.

### Compiler Surfaces

Recommended bootstrap crate boundaries:

- `edict-syntax`: lexer, parser, concrete syntax tree, source maps.
- `edict-core`: AST, type model, Core IR, canonicalization, hash fixtures.
- `edict-compile`: type checking, determinism checking, effect extraction.
- `wesley-edict-profile`: Wesley integration that consumes Shape IR and Law IR
  and invokes Edict compiler components.
- target profile crates outside Wesley core, for example `echo-edict-target`.

If these crates temporarily live in the Wesley workspace, they should carry an
explicit extraction note and must not be imported by `wesley-core` in a way that
makes Edict, Continuum, or Echo part of generic GraphQL lowering.

### Implemented Effect Diagnostic Boundary

The source compiler distinguishes exact imported semantic effects from pure
helpers. A bare nongeneric effect in a pure-call position within an intent body reports
`EffectWithoutFailureMapping` and names the domain-mappable failure variants. Authors
must use the supported annotated effect binding and its required mapping.
Bare effect input expressions and their authenticated input types are checked
before failure-map guidance. Their existing primary error kinds and spans remain.
Authenticated input types provide context for integer literals. Bare effect
outputs must satisfy the surrounding expected type. Failure-map advice names
only domain-mappable coordinates; other failure classes remain platform-owned.
Generic semantic effects remain unsupported in both bare and mapped calls.
Explicit effect type arguments also report `UnsupportedSourceShape` in mapped
and bare forms before failure-map guidance.
Effects in source-owned pure functions and intent clauses report
`UnsupportedSourceShape` without recommending a binding that is illegal there.
An effect with no declared failures reports that bindings without a failure map
are unsupported; the diagnostic does not suggest an empty `else` map.
An unknown effect in an explicit effect binding reports `MissingContextFact`
with available exact-owner exports under the selected source alias. A bare
unknown call retains `UnresolvedFunction`; an unknown coordinate does not
establish whether the author intended an effect or a pure helper. This diagnostic guidance does not
provide a capability or alter effect/profile compatibility. If the context has
no authenticated export signatures, the diagnostic reports missing information
rather than asserting an empty lawpack export surface. Dependent-error
recovery remains a separate compiler behavior.

### Edict Source Pipeline

```text
Edict source
  -> lex and parse
  -> build concrete syntax tree with source spans
  -> lower to typed Edict AST
  -> resolve imports by identity/version/digest
  -> resolve optional Wesley Shape IR and Law IR facts
  -> typecheck
  -> determinism check
  -> effect extraction
  -> footprint ceiling check when declared
  -> cost budget check
  -> lower to Core IR
  -> canonicalize Core IR through edict.canonical-value/v1
  -> compute Core IR hash
```

### Implemented Ordinary Result Budget Boundary

The source compiler checks the declared ordinary result's canonical-CBOR maximum
against `maxOutputBytes` before it emits Core. This applies without source
functions. The check includes exact definite-length headers, field keys,
fixed-width integer maxima, bounded lists, and nominal representations.
`String<max=N>` permits up to `4 * N` UTF-8 bytes. Overflow or an output maximum
above the selected budget reports `InvalidBound`.

Direct `ExternalActionRequest` results retain the separate request-only protocol
contract. This pass does not assign them an ordinary result-projection encoding
maximum. Nested opaque request shapes in ordinary results report
`UnsupportedSourceShape`. Source-function accounting retains its existing
separate restrictions and conservative costs. This check does not add complete
primitive allocation inference or runtime input validation.

### GraphQL/Wesley Source Profile Pipeline

Wesley may also compile GraphQL and `weslaw` into Edict-compatible source
profile facts:

```text
GraphQL SDL
  -> Wesley Shape IR
  -> shape facts

weslaw/v1
  -> Wesley Law IR
  -> law facts

shape facts + law facts + source profile mapping
  -> Edict Core declarations or imports
```

This bridge is useful for migration and generated artifact continuity. It must
not make GraphQL the Continuum protocol or require every Edict source to start
as GraphQL.

`graphql-wesley@1` must also supply bounds before a locked lawful-autonomous
bundle can be produced. Any imported GraphQL field of type `String`, `Bytes`,
list, or nested input object must resolve to Edict bounds through Shape IR, Law
IR, digest-locked source-profile configuration, digest-locked lawpack constants,
or compile options included in the compilation identity. Unbounded imported
GraphQL text, bytes, lists, nested input graphs, or output fields reject
locked-bundle production.

Participant policy may accept, reject, or lower admitted runtime ceilings. It
must not supply missing Core type bounds, reinterpret imported shape fields, or
change semantic bounds during admission.

### Target Lowering Pipeline

```text
Core IR + lawpacks + target profile
  -> target lowering
  -> target IR
  -> target verifier
  -> inferred target footprint templates
  -> inferred target cost templates
  -> generated artifacts
  -> contract bundle manifest
  -> SHA-lock
  -> HOLMES / Watson / Moriarty evidence
```

Wesley can orchestrate this pipeline when acting as a compiler front end, but
target meaning is profile-owned.

## Diagnostics

Diagnostics must be structured and stable enough for agents to repair source.

Diagnostic shape:

```json
{
  "code": "EDICT-EFFECT-UNDERCLAIM",
  "severity": "error",
  "message": "computed target footprint exceeds declared maximum",
  "sourceSpan": {
    "file": "graft.amblicit",
    "start": { "line": 12, "column": 3 },
    "end": { "line": 12, "column": 54 }
  },
  "coordinate": "graft.structural_history@1.intent.recordGitWarpImportBatch",
  "repair": {
    "kind": "adjust-footprint-bound-or-remove-effect",
    "details": {}
  }
}
```

Initial diagnostic families:

- `EDICT-PARSE-*`
- `EDICT-IMPORT-*`
- `EDICT-TYPE-*`
- `EDICT-DETERMINISM-*`
- `EDICT-BOUNDS-*`
- `EDICT-EFFECT-*`
- `EDICT-FOOTPRINT-*`
- `EDICT-LOWERING-*`
- `EDICT-CANONICAL-*`
- `EDICT-BUNDLE-*`

Target profiles may define target-specific diagnostics, but they must preserve
the compiler/registration/obstruction split.

## What Makes Edict Different

### Compared To GraphQL

GraphQL defines shape, query, mutation, and subscription surfaces. Edict
defines the bounded lawful optic body behind such surfaces: basis, aperture,
effects, support posture, obstruction mappings, and target verification
obligations. GraphQL may be an Edict source profile input, but GraphQL does not
provide target footprint inference, support-ledger tracking, or target IR
verification by itself.

### Compared To SQL And Stored Procedures

SQL assumes a relational storage model. Edict has no built-in storage model.
Runtime target profiles provide the state model and verifier semantics.

### Compared To Solidity And Smart Contract VMs

Solidity targets a global VM and chain execution model. Edict targets no
universal runtime. It compiles to participant-supported target IRs and binds
all evidence into SHA-locked bundles.

### Compared To Policy Languages

Policy languages such as Rego decide whether something should be allowed.
Edict defines the optic body, its typed effects, its footprint template,
support obligations, obstruction mappings, and the artifacts needed to register
and invoke it. Admission policy remains separate.

### Compared To Workflow Languages

Workflow languages orchestrate external steps and often rely on opaque task
implementations. Edict rejects opaque raw callbacks in the lawful-autonomous
lane. Effects must be visible through imported intrinsics.

### Compared To IDLs And ABI Languages

IDLs define callable shape. Edict defines shape plus lawful effect semantics,
target lowering obligations, verifier evidence, and bundle hashes.

### Compared To Echo DPO

Echo DPO is a target semantics for typed graph rewrite. Edict Core is the
runtime-neutral language that can lower to Echo DPO or to non-graph targets.
Span IR is Echo target IR, not Edict Core IR.

## Language Versus Implementation Profile

The normative language and the first compiler are separated so the parser
campaign is not a multi-year heroic effort (`EDICT-LANG-PROFILE-001`):

- `edict.language/v1` is the full normative language defined by this spec:
  records, enums, variants, exhaustive `match`, `Option`, bounded `List`/`Map`,
  refined scalars, regex-lite constraints, bounded recursive imports, pure
  first-order functions, `if`/branch-yield, bounded `for`, record spread,
  target/lawpack effects, guards, typed obstructions, and budgets.
- `edict.implementation/minimal-v1` is the first conformance target. It ships:
  records, enums, `Option`, bounded `List`, pure first-order functions, `if`
  expressions, branch-yield conditional effects, target/lawpack effects, guards,
  typed obstructions, and budgets.

Features outside minimal-v1 come online behind named **conformance capability
flags**, and these are split into two distinct concepts — source-language support
and Core-ABI support are not the same thing (`EDICT-LANG-CAPABILITIES-SPLIT-001`):

- **`requiredSourceCapabilities`**: source-profile / release metadata for
  surface-syntax features (e.g. record-spread syntax, regex-lite syntax). The
  **compiler** checks these against the source profile.
- **`requiredCoreCapabilities`**: inferred from Core constructs (e.g. variants,
  maps, bounded recursive imports). This is a **hash-significant Core module
  field**, checked by the **lowerer/verifier**.

A conforming implementation supports the declared flags or rejects with a
registration-class error. A bundle may expose a derived index over these flags,
but that index is not a second authority: source capabilities are authoritative
in the source profile and Core capabilities are authoritative in the Core
module. Capability flags never change Core IR hash semantics for features that
are present.

The current compiler emits `core.variant/v1` whenever the finished Core type table
contains a variant declaration. This is a module-wide requirement, including
unused declarations and declarations linked from source modules. The conservative
scope avoids presenting a type graph as supported merely because its current
intent bodies do not inspect that type. It does not claim a variant value ABI.
Current Target lowering requires this capability for variant type tables and
independently checks constructor identities and payload types. Missing capability
metadata refuses as `InvalidCoreIdentity` before unknown capabilities are checked.
After that check, unknown capabilities refuse as `UnsupportedCoreCapability`
without an artifact. Provider and runtime variant
support and variant-valued external results remain separate implementation gates.

## Implementation Plan

### Phase 0: Spec And Fixtures

- Land this Edict v1 language spec under packet `0021`.
- Define `edict.canonical-value/v1`.
- Define `edict.canonical-cbor/v1` as the authoritative hash input.
- Define `edict.canonical-json/v1` as review/debug rendering.
- Add parser conformance fixtures for valid and invalid syntax.
- Add parser fixtures for branch-yield conditional effects, effect `else`
  obstruction mapping, `budget <=`, bound refs, spread literals,
  order-independent intent clauses, and rejected `migration`/`projection`
  declarations.
- Add Core IR canonicalization fixtures with exact expected hashes.
- Add relapse fixtures proving no graph built-ins exist in Core.
- Add same-semantics/different-spelling fixtures:
  formatting changes, comment changes, import alias changes, nonsemantic
  declaration reordering, equivalent map literal ordering, and source-location
  changes.
- Add digest mismatch fixtures for lawpack, target profile, shape, lowerer, and
  verifier substitution.
- Add JSON integer ambiguity fixtures for `I64`, `U64`, and `Bytes`.
- Add relapse fuzz fixtures for ambient time/randomness, host callbacks,
  unbounded closures, duplicate content-addressed creates, target profile digest
  swaps, source-path changes, codename coordinates, and hidden writes in
  read-only operations.

### Phase 1: Parser And AST

- Implement lexer and parser for the grammar above.
- Emit concrete syntax tree and source spans.
- Add structured parse diagnostics.
- Preserve comments only in source maps, not semantic AST.

### Phase 2: Type System And Determinism

- Implement scalar, record, enum, variant, option, list, and map types.
- Resolve imports by canonical coordinate and digest.
- Reject recursion, unbounded loops, nondeterministic APIs, and unresolved
  profile semantics.
- Enforce GraphQL source-profile bounds for imported strings, bytes, lists,
  nested inputs, and outputs.
- Add agent-readable repair metadata to diagnostics.

### Phase 3: Edict Core IR

- Lower typed AST to Edict Core IR.
- Define canonical CBOR encoding plus canonical JSON review rendering.
- Compute stable Core IR hashes.
- Alpha-normalize local binders in hash-significant Core.
- Encode predicates as typed expression trees, not strings.
- Add golden fixtures proving formatting and alias changes do not alter Core
  IR hashes.

### Phase 4: Target Profile Manifest

- Define `edict.target-profile/v1`.
- Add `kv.transactional@1` as the first non-Echo target profile fixture.
- Add a target profile conformance harness.
- Prove a participant can advertise lawful-autonomous bundle admission
  without Echo DPO.
- Add KV/CAS fixtures for read key, missing key, compare version,
  create-if-absent, ensure-same, replace-if-version, delete-if-version, bounded
  range read, conflict obstruction, and cost bounds.

### Phase 5: Echo DPO Target

- Implement `echo.dpo@1` in the Echo-owned target lane.
- Lower a narrow runtime-native Edict fixture to Echo Span IR.
- Verify graph footprint inference and DPO side-condition diagnostics.
- Generate Echo-specific registration metadata outside Wesley core.

### Phase 6: Bundle And SHA-lock

- Define `continuum.contract-bundle/v1`.
- Keep contract bundle identity participant-neutral and separate from
  admission.
- Bind raw source artifact provenance, source-profile semantic facts, Core IR,
  target IR, lawpack, target profile, verifier report, generated artifacts, and
  compiler/lowerer/verifier evidence references.
- Add tamper tests for each hash-bound component.

### Phase 7: Wesley Source Profile Bridge

- Add `graphql-wesley@1` profile facts from Shape IR.
- Add `weslaw@1` profile facts from Law IR.
- Prove GraphQL and weslaw can feed Edict Core without becoming required for
  all Edict sources.

### Phase 8: Extraction Decision

- If bootstrap work started in Wesley, extract language crates and fixtures to
  `flyingrobots/edict` before treating Edict as a public product surface.
- Leave Wesley with adapter crates and integration tests.
- Leave Echo with `echo.dpo@1`.
- Leave Continuum with participant protocol and admission flow.

### Adjacent Platform Milestones

These are important but are not parser or language freeze prerequisites:

- Continuum admission requests, receipts, capability receipts, and participant
  policy epochs.
- HOLMES, Watson, and Moriarty integration over bundle, admission, and verifier
  evidence.
- Transparency-log publication and Merkle consistency proof export.
- `edict profile diff` compatibility classification.
- Nutrition labels and explanation artifacts for humans, agents, and auditors.

## Acceptance Criteria

- Edict Core has no graph built-ins.
- Source syntax can import `echo.dpo@1` and a non-Echo target profile.
- Formatting-only source changes do not alter Core IR hash.
- Import alias changes do not alter Core IR hash.
- Local binder spelling does not affect Core IR hash.
- Raw source bytes, source paths, source locations, comments, formatting,
  bundle profile, assurance lane, admission class, participant policy, and
  display metadata are excluded from Core IR hash input.
- Digest mismatch rejects compilation.
- Nondeterminism rejects compilation.
- Unbounded loops reject compilation.
- Recursive pure functions reject compilation.
- Recursive value types reject unless every recursive path has a digest-locked
  maximum depth.
- Declared footprint underclaim rejects compilation.
- FIDLAR raw callbacks are impossible in normal syntax.
- Target lowering produces profile-owned target IR.
- Target lowering preserves the intent optic structure: basis, aperture or
  footprint, projection or affect boundary, support posture, guards,
  obstruction mappings, cost bounds, and canonical artifact identity.
- Support loss, degeneracy, footprint overlap, and witness debt cannot be
  silently erased by Core lowering, target lowering, or explanation artifacts.
- Echo Span IR is documented and tested as Echo target IR only.
- A toy non-Echo target proves the architecture is not graph-bound.
- Wesley GraphQL/weslaw bridge emits source-profile facts without making
  GraphQL required by Edict.
- GraphQL source-profile imports without digest-locked bounds reject
  locked-bundle production. Participant policy cannot supply missing Core type
  bounds.
- Read-only intent containing a write effect rejects.
- Read-only intent containing a runtime-materialized semantic append/log effect
  rejects.
- Query and observer operations are first-class lawful-autonomous operations
  with Core IR digest, target profile, footprint, budget, verifier result,
  generated artifacts, and obstruction taxonomy.
- Proof-only semantic facts are distinguished from durable runtime effects.
- Unbounded closure read rejects compilation or locked-bundle production.
- Conditional effects preserve structured branches in Core IR.
- Loop effects preserve static maximum cardinality in structured Core IR.
- Flattened path predicates and effective cardinalities are derived analysis,
  not duplicate hash-significant Core truth.
- Footprint and cost are checked separately.
- Content-addressed duplicate create fixture distinguishes `create` from
  `ensure`.
- Full-buffer text snapshot without maximum bound rejects.
- Shape import without digest may parse in development mode but cannot produce
  a locked bundle without lockfile resolution.
- Lawpack helper implemented outside Edict must be digest-locked, sandboxed,
  deterministic, fuel-bounded, and cost-bounded.
- Cross-bundle `invoke` is rejected in v1.
- Runtime stale-basis assumptions lower to target guards, not local assertions
  only.
- `assert`, `require`, and `guarantee` have non-overlapping semantics:
  proof-only, atomic runtime precondition, and precommit postcondition.
- Runtime reads, guards, writes, resource checks, and guarantees lower to one
  target-owned atomic application unit with no externally visible partial
  writes on obstruction.
- An intent may emit runtime effects for only one target profile unless it uses
  a composite target profile that owns coordination and atomicity.
- Codename terms reject from hash-significant canonical coordinates.
- Branch-yield conditional effect values lower to structured Core branches with
  result bindings.
- Effect failures map to typed domain obstructions through checked
  `obstructionMap` entries.
- Single `else Obstruction` shorthand rejects when an effect exposes multiple
  unmapped domain-mappable failure coordinates.
- Participant-owned, integrity, resource, and internal failures cannot be
  laundered into author-defined domain obstructions.
- Source `budget <=` clauses lower to Core budgets and reject underclaimed cost.
- Loop, list, map, and field bounds accept literal or digest-locked bound refs.
- Intent clauses are order-independent with unique profile/implements/footprint
  and budget clauses.
- `migration` and `projection` are reserved and rejected by the v1 parser.
- `some`, `none`, and `default` are prelude functions with pinned semantics.
- Record spread literals are deterministic and reject ambiguous explicit fields.
- Hash-significant Core predicates are typed expression trees, not strings.
- Map key types reject non-canonical key domains.
- Unit, Digest, variant constructor, exhaustive match, and map literal syntax
  have parser fixtures.
- Bytes literal escapes, integer underscore placement, digest hex
  normalization, regex profile, checked arithmetic, and Unicode normalization
  profile behavior have canonical fixtures.
- Source-level `hash` requires a string-literal label and lowers to
  domain-separated canonical hashing.

## Resolved v1 Directions

- MVP user-defined pure functions are first-order, non-recursive,
  non-higher-order, monomorphized or non-generic, effect-free, and cost-bounded.
- Authoritative hash input is `edict.canonical-cbor/v1`; canonical JSON is
  review/debug rendering.
- First non-Echo target is `kv.transactional@1`; event-log follows later.
- Migrations start as ordinary intents with a migration profile.
- Target profile manifests are data; lowerers and verifiers are locked,
  sandboxed executable components satisfying the manifest.
- Portable semantic records may exist only as sugar for lawpack semantic
  effects.
- Conditional effect values use branch-yield syntax in `let` binding position.
- Effect failure obstruction mapping is part of v1 source and Core.
- Cross-bundle `invoke` is rejected in v1.
- Query/observer operations are first-class and must prove read-only from
  inferred effects.

## Future Work

- A future version may allow cross-bundle `invoke` only when callee effects,
  costs, obstructions, target requirements, and admission requirements are
  inlined or summarized by verifier-approved bundle evidence.
- A future version may add first-class `migration`, `projection`, and `observer`
  declarations after ordinary intents and observer desugaring prove the
  semantics.
- A future version may loosen map-key restrictions for imported types with
  explicitly declared canonical key semantics.

## Relapse Checks

- Do not put Echo graph operations in Edict Core.
- Do not let Wesley core own Continuum participant policy.
- Do not let Continuum admission policy define language semantics.
- Do not describe Span IR as universal Edict or Continuum IR.
- Do not trust declared footprints without inferred effects.
- Do not let privileged host callbacks pass as lawful-autonomous
  operations.
- Do not require GraphQL for native Edict authoring.
- Do not make HOLMES, Watson, or Moriarty runtime authorities.

## Appendix A: jedit Intent Stress Test

> [!NOTE]
> jedit is the intended first real-world use case, so these are kept
> **clause-conformant**, not left to rot. The rope-package and structural-history
> intents below carry the clauses v1 requires (including `basis`, per
> `EDICT-LANG-INTENT-CLAUSES-001`) and are fixture candidates. The one
> deliberate exception is the **Product Text Buffer Optic Sketch**, which uses
> `use capability` and `invoke` — **explicitly rejected v1 grammar** — to expose
> design pressure; it is a negative/illustrative sketch, not a conforming intent.

### Source Anchors

The current jedit checkout inspected for this appendix is:

```text
~/git/jim/jedit
```

The installed app package surface is:

```text
contracts/jedit/rope.graphql
```

It exposes five operation boundaries:

- mutation `createBufferWorldline`;
- mutation `replaceRangeAsTick`;
- mutation `createCheckpoint`;
- query `worldlineSnapshot`;
- query `textWindow`.

Two adjacent jedit contract surfaces also exist:

- `contracts/jedit/text-buffer-optic.graphql`, an app-facing product optic
  contract with opaque read-basis handles;
- `contracts/jedit/structural-history.graphql`, an app-owned structural
  history contract marked as a surface Wesley should consume later.

This appendix treats `rope.graphql` as the primary stress surface because it is
the installed generated jedit package. Consistent with the note above: the
rope-package and structural-history intents are clause-conformant **fixture
candidates**, while the product text-buffer optic surface is the one deliberate
**non-v1 sketch** (it exposes capability invocation/composition and opaque
handles via the rejected `invoke`/`use capability` forms). "Stress fixture" here
means the shape/source anchor, not a promise that every sketch in the appendix
compiles.

### Stress Findings

The jedit translation supports the core proposal but exposes several v1
language requirements and negative fixtures:

- Read-only is an inferred theorem. Source may claim read-only posture, but
  target and lawpack effects must prove it.
- Target profiles need closure intrinsics. jedit's rope operation reads
  `ropeRangeClosure` and `anchorsIntersectingEditWindow`; those must be
  target/lawpack-defined closure reads, not generic Core concepts.
- Closure reads must carry cost and size bounds. `textWindow` is a positive
  fixture; unbounded full snapshot materialization is a negative fixture.
- Optional creates need first-class representation. `createBufferWorldline`
  optionally creates `Checkpoint`, `TextBlob`, and `RopeLeaf`.
- Content-addressed values need explicit `ensure` semantics. `TextBlob` ids
  derived from bytes should not use absence-only `create`.
- Current jedit SDL still uses `String` for `initialText` and `insertText`.
  The Edict design's determinism posture still recommends future raw editor
  text migrate to `Bytes` or a lawpack-defined raw text scalar so Unicode
  normalization cannot corrupt buffer content.
- Product-facing optic APIs need opaque capability handles. `ReadBasisHandle`
  belongs above the rope target and should be modeled as participant/app
  capability, not as Echo graph identity.
- Product-facing Edicts may need declared lowering from one capability to
  another. `createBuffer` naturally lowers to `createBufferWorldline` plus
  read-basis issuance. Cross-bundle `invoke` is rejected in v1.
- Query operations are not second-class. `worldlineSnapshot` and `textWindow`
  need the same SHA-lock, profile, target, and assurance posture as mutations.

### Installed Rope Package As Edict

The following source is intentionally written as a near-v1 stress sketch, not as
a committed parser fixture. It uses branch-yield conditional effects,
effect-level obstruction mapping, explicit budgets, and target symbolic refs.
`worldlineSnapshot` remains a negative v1 fixture until finite bounds exist. The
sketch uses `jedit.rope@1` helper functions and `echo.dpo@1` target intrinsics
to keep Echo graph semantics behind the target profile.

```amblicit
package jedit.rope_contract@1;

use shape "contracts/jedit/rope.graphql" as shape;
use lawpack jedit.rope@1 as rope;
use target echo.dpo@1 as echo;

intent createBufferWorldline(input: shape.CreateBufferWorldlineInput)
  returns shape.CreateBufferWorldlineResult
  profile echo.createOnly
  basis none
  footprint <= rope.createBufferWorldlineMax
  budget <= rope.createBufferWorldlineBudget
  where input.bufferKey != ""
{
  let initialText = default(input.initialText, "");
  let projectionPath = default(input.projectionPath, input.bufferKey);
  let worldlineId = rope.worldlineId(projectionPath);

  let initialBytes = rope.encodeText(initialText);
  let initialBlob = if len(initialBytes) == 0 {
    yield none<shape.TextBlob>();
  } else {
    let blobRef = echo.ref<shape.TextBlob>(rope.textBlobId(initialBytes));
    let blob = blobRef.ensure({
      blobId: rope.textBlobId(initialBytes),
      encoding: shape.TextEncoding.UTF8,
      byteLength: len(initialBytes),
      contentHash: hash("TextBlob", initialBytes),
    }) else rope.TextBlobHashConflict;
    yield some(blob);
  };

  let initialLeaf = if isSome(initialBlob) {
    let leafRef = echo.ref<shape.RopeLeaf>(rope.initialLeafId(worldlineId));
    let leaf = leafRef.create({
      leafId: rope.initialLeafId(worldlineId),
      blobId: unwrap(initialBlob).blobId,
      startByte: 0,
      endByte: len(initialBytes),
      byteLength: len(initialBytes),
      lineCount: rope.lineCount(initialBytes),
      utf16Length: rope.utf16Length(initialText),
    }) else rope.RopeLeafAlreadyExists;
    yield some(leaf);
  } else {
    yield none<shape.RopeLeaf>();
  };

  let root = rope.initialRoot(worldlineId, initialLeaf);
  let headRef = echo.ref<shape.RopeHead>(rope.headId(root));
  let head = headRef.create({
    headId: rope.headId(root),
    worldlineId,
    rootNodeId: rope.rootNodeId(root),
    byteLength: len(initialBytes),
    lineCount: rope.lineCount(initialBytes),
    utf16Length: rope.utf16Length(initialText),
    equivalenceDigest: hash("RopeHead", initialBytes),
  }) else rope.RopeHeadAlreadyExists;

  let worldlineRef = echo.ref<shape.BufferWorldline>(worldlineId);
  let worldline = worldlineRef.create({
    worldlineId,
    bufferKey: input.bufferKey,
    canonicalHeadId: head.headId,
    createdAtRopeRewriteId: none<shape.ID>(),
    projectionPath,
  }) else rope.BufferWorldlineAlreadyExists;

  let checkpoint = if input.createInitialCheckpoint {
    let checkpointRef = echo.ref<shape.Checkpoint>(
      rope.checkpointId(worldlineId, head.headId)
    );
    let createdCheckpoint = checkpointRef.create({
      checkpointId: rope.checkpointId(worldlineId, head.headId),
      worldlineId,
      headId: head.headId,
      kind: shape.CheckpointKind.INITIAL,
      label: none<String>(),
      createdByRopeRewriteId: none<shape.ID>(),
    }) else rope.CheckpointAlreadyExists;
    yield some(createdCheckpoint);
  } else {
    yield none<shape.Checkpoint>();
  };

  return {
    worldline,
    head,
    checkpoint,
  };
}

intent replaceRangeAsTick(input: shape.ReplaceRangeAsTickInput)
  returns shape.ReplaceRangeAsTickResult
  profile echo.boundaryReplacementLens
  basis input.baseHeadId
  footprint <= rope.replaceRangeAsTickMax
  budget <= rope.replaceRangeAsTickBudget
  where input.startByte <= input.endByte
{
  let worldlineRef = echo.ref<shape.BufferWorldline>(input.worldlineId);
  let worldline = worldlineRef.read()
    else rope.WorldlineMissing;

  let baseHeadRef = echo.ref<shape.RopeHead>(input.baseHeadId);
  let baseHead = baseHeadRef.read()
    else rope.BaseHeadMissing;

  require worldline.canonicalHeadId == baseHead.headId
    else rope.StaleBaseHead;

  require baseHead.worldlineId == worldline.worldlineId
    else rope.BaseHeadWorldlineMismatch;

  let touchedRope = rope.rangeClosure(
    baseHead,
    input.startByte,
    input.endByte
  ).read() else rope.RopeRangeClosureExceeded;

  let affectedAnchors = rope.anchorsIntersectingEditWindow(
    worldline,
    baseHead,
    input.startByte,
    input.endByte
  ).read() else rope.AnchorClosureExceeded;

  let insertBytes = rope.encodeText(input.insertText);
  let rewrite = rope.replaceRangePlan(
    worldline,
    baseHead,
    touchedRope,
    affectedAnchors,
    input.startByte,
    input.endByte,
    insertBytes
  );

  let newBlob = if len(insertBytes) == 0 {
    yield none<shape.TextBlob>();
  } else {
    let blobRef = echo.ref<shape.TextBlob>(rewrite.newBlobId);
    let blob = blobRef.ensure({
      blobId: rewrite.newBlobId,
      encoding: shape.TextEncoding.UTF8,
      byteLength: len(insertBytes),
      contentHash: hash("TextBlob", insertBytes),
    }) else rope.TextBlobHashConflict;
    yield some(blob);
  };

  for leaf in rewrite.newLeaves bounded rope.maxCreatedLeaves {
    echo.ref<shape.RopeLeaf>(leaf.leafId).create(leaf)
      else rope.RopeLeafAlreadyExists;
  }

  for branch in rewrite.newBranches bounded rope.maxCreatedBranches {
    echo.ref<shape.RopeBranch>(branch.branchId).create(branch)
      else rope.RopeBranchAlreadyExists;
  }

  let nextHead = echo.ref<shape.RopeHead>(rewrite.nextHead.headId).create(
    rewrite.nextHead
  ) else rope.RopeHeadAlreadyExists;

  let ropeRewrite = echo.ref<shape.RopeRewrite>(rewrite.ropeRewriteId).create({
    ropeRewriteId: rewrite.ropeRewriteId,
    worldlineId: worldline.worldlineId,
    kind: shape.RewriteKind.REPLACE_RANGE_AS_TICK,
    sequenceNumber: rewrite.sequenceNumber,
    author: input.author,
  }) else rope.RopeRewriteAlreadyExists;

  let ropeDiff = echo.ref<shape.RopeDiff>(rewrite.ropeDiffId).create({
    ropeDiffId: rewrite.ropeDiffId,
    ropeRewriteId: ropeRewrite.ropeRewriteId,
    baseHeadId: baseHead.headId,
    nextHeadId: nextHead.headId,
    rewriteKind: shape.RewriteKind.REPLACE_RANGE_AS_TICK,
    startByte: input.startByte,
    endByte: input.endByte,
    insertedByteLength: len(insertBytes),
    deletedByteLength: input.endByte - input.startByte,
    inverseFragmentDigest: rewrite.inverseFragmentDigest,
    summary: rewrite.summary,
  }) else rope.RopeDiffAlreadyExists;

  let nextWorldline = {
    ...worldline,
    canonicalHeadId: nextHead.headId,
  };

  worldlineRef.replace(nextWorldline)
    else rope.StaleBaseHead;

  return {
    worldline: nextWorldline,
    nextHead,
    ropeRewrite,
    ropeDiff,
  };
}

intent createCheckpoint(input: shape.CreateCheckpointInput)
  returns shape.CreateCheckpointResult
  profile echo.createOnly
  basis input.worldlineId
  footprint <= rope.createCheckpointMax
  budget <= rope.createCheckpointBudget
{
  let worldlineRef = echo.ref<shape.BufferWorldline>(input.worldlineId);
  let worldline = worldlineRef.read()
    else rope.WorldlineMissing;

  let currentHeadRef = echo.ref<shape.RopeHead>(worldline.canonicalHeadId);
  let currentHead = currentHeadRef.read()
    else rope.CurrentHeadMissing;

  require worldline.canonicalHeadId == currentHead.headId
    else rope.StaleBaseHead;

  let checkpoint = echo.ref<shape.Checkpoint>(
    rope.checkpointId(worldline.worldlineId, currentHead.headId)
  ).create({
    checkpointId: rope.checkpointId(worldline.worldlineId, currentHead.headId),
    worldlineId: worldline.worldlineId,
    headId: currentHead.headId,
    kind: input.kind,
    label: input.label,
    createdByRopeRewriteId: none<shape.ID>(),
  }) else rope.CheckpointAlreadyExists;

  return {
    worldline,
    head: currentHead,
    checkpoint,
  };
}

intent worldlineSnapshot(input: shape.WorldlineSnapshotInput)
  returns shape.WorldlineSnapshot
  profile echo.readOnly
  basis input.worldlineId
  footprint <= rope.worldlineSnapshotMax
  budget <= rope.worldlineSnapshotBudget
{
  // Negative v1 fixture unless the lawpack or input supplies a finite
  // maxBytes/maxNodes bound for fullRopeClosure and materialized output.
  let worldline = echo.ref<shape.BufferWorldline>(input.worldlineId).read()
    else rope.WorldlineMissing;
  let head = echo.ref<shape.RopeHead>(worldline.canonicalHeadId).read()
    else rope.CurrentHeadMissing;
  let ropeNodes = rope.fullRopeClosure(head).read()
    else rope.UnboundedSnapshotRejected;
  let checkpoints = rope.checkpointsForWorldline(worldline).read()
    else rope.CheckpointReadExceeded;

  return {
    worldline,
    head,
    checkpoints,
    text: rope.materializeText(ropeNodes),
  };
}

intent textWindow(input: shape.TextWindowInput)
  returns shape.TextWindowReading
  profile echo.readOnly
  basis input.worldlineId
  footprint <= rope.textWindowMax
  budget <= rope.textWindowBudget
  where input.viewportLineCount >= 0,
        input.beforeLines >= 0,
        input.afterLines >= 0,
        input.maxBytes >= 0,
        input.viewportLineCount <= rope.maxViewportLineCount,
        input.beforeLines <= rope.maxContextLines,
        input.afterLines <= rope.maxContextLines,
        input.maxBytes <= rope.maxTextWindowBytes
{
  let worldline = echo.ref<shape.BufferWorldline>(input.worldlineId).read()
    else rope.WorldlineMissing;
  let head = echo.ref<shape.RopeHead>(worldline.canonicalHeadId).read()
    else rope.CurrentHeadMissing;
  let window = rope.textWindowPlan(
    head,
    input.cursorLine,
    input.viewportLineCount,
    input.beforeLines,
    input.afterLines,
    input.maxBytes
  );
  let ropeNodes = rope.windowClosure(head, window).read()
    else rope.TextWindowBoundExceeded;

  return rope.materializeTextWindow(worldline, head, window, ropeNodes);
}
```

Checkpoint creation also needs an ID semantics decision. If
`rope.checkpointId(worldlineId, headId)` permits one checkpoint per head, then
`create` is correct and repeated calls obstruct. If the checkpoint is
content-addressed or idempotent, it should use `ensure`. If label or kind
distinguish checkpoints, the ID needs those fields or another entropy source.
The `StaleBaseHead` requirement in the sketch must lower to an atomic runtime
guard on the checkpoint creation or on the operation as a whole.

### Product Text Buffer Optic Sketch

The product-facing optic surface is not just a target operation. It creates and
uses opaque `ReadBasisHandle` capabilities over the installed rope package.
This stresses whether Edict v1 needs cross-capability invocation syntax, or
whether such composition should be expressed as a lawpack lowering.

The following `invoke` sketch is intentionally rejected for v1. The preferred
v1 route is a lawpack lowering from the product optic to target IR. A future
version may admit invocation only with verifier-approved callee summaries or
inlined effects, costs, obstructions, target requirements, and admission
requirements.

```amblicit
package jedit.text_buffer_optic@1;

use shape "contracts/jedit/text-buffer-optic.graphql" as optic;
use lawpack jedit.text_buffer_optic@1 as textBuffer;
use capability jedit.rope_contract@1 as rope;

intent createBuffer(input: optic.CreateBufferInput)
  returns optic.CreateBufferPayload
  profile textBuffer.productCreate
{
  let created = invoke rope.createBufferWorldline({
    bufferKey: input.bufferKey,
    initialText: input.initialText,
    projectionPath: input.projectionPath,
    createInitialCheckpoint: false,
  });

  let readBasis = textBuffer.issueReadBasis(created.worldline.worldlineId);

  return {
    buffer: textBuffer.toTextBuffer(created.worldline),
    readBasis,
    bufferVersion: 1,
    receiptId: textBuffer.receiptId(created),
  };
}

intent replaceRange(input: optic.ReplaceRangeInput)
  returns optic.ReplaceRangePayload
  profile textBuffer.productEdit
{
  let resolvedBasis = textBuffer.resolveBuffer(input.bufferId);

  let edited = invoke rope.replaceRangeAsTick({
    worldlineId: resolvedBasis.worldlineId,
    baseHeadId: resolvedBasis.headId,
    startByte: input.startByte,
    endByte: input.endByte,
    insertText: input.insertText,
    author: none<String>(),
  });

  let readBasis = textBuffer.issueReadBasis(edited.worldline.worldlineId);

  return {
    buffer: textBuffer.toTextBuffer(edited.worldline),
    readBasis,
    bufferVersion: textBuffer.nextBufferVersion(input.bufferId),
    receiptId: textBuffer.receiptId(edited),
  };
}

intent textWindow(
  readBasis: optic.ReadBasisHandle,
  input: optic.TextWindowInput
)
  returns optic.TextWindowReading
  profile textBuffer.productRead
{
  let resolvedBasis = textBuffer.resolveReadBasis(readBasis);

  let reading = invoke rope.textWindow({
    worldlineId: resolvedBasis.worldlineId,
    cursorLine: input.cursorLine,
    viewportLineCount: input.viewportLineCount,
    beforeLines: input.beforeLines,
    afterLines: input.afterLines,
    maxBytes: input.maxBytes,
  });

  return textBuffer.toProductTextWindow(reading);
}
```

The `use capability` and `invoke` forms above are not accepted v1 grammar. They
are included to make the design pressure visible. The v1 alternative is a
lawpack lowering that rewrites the product optic directly to the same target IR
as the rope package without an explicit capability call in source.

### Structural History Sketch

The structural-history contract is semantic and storage-neutral in shape. It is
a good fit for portable Edict because it can be expressed first as history facts
and then lowered to Echo, git-warp, KV, or event-log targets through lawpacks.

```amblicit
package jedit.structural_history@1;

use shape "contracts/jedit/structural-history.graphql" as historyShape;
use lawpack jedit.structural_history@1 as history;

intent createTextHistory(input: historyShape.CreateTextHistoryInput)
  returns historyShape.CreateTextHistoryPayload
  implements history.createTextHistory
  basis none
  budget <= history.createTextHistoryBudget
{
  let _created = history.textHistoryCreated.record({
    historyId: history.historyId(input.bufferKey, input.provenance),
    bufferKey: input.bufferKey,
    projectionPath: input.projectionPath,
    initialTextDigest: hash("TextHistory.initialText", input.initialText),
    provenance: input.provenance,
  }) else history.TextHistoryAlreadyExists;

  return history.createdPayload(input);
}

intent replaceTextRange(input: historyShape.ReplaceTextRangeInput)
  returns historyShape.ReplaceTextRangePayload
  implements history.replaceTextRange
  basis input.baseRevisionId
  budget <= history.replaceTextRangeBudget
  where input.startByte <= input.endByte
{
  let _replaced = history.textRangeReplaced.record({
    historyId: input.historyId,
    baseRevisionId: input.baseRevisionId,
    range: {
      startByte: input.startByte,
      endByte: input.endByte,
    },
    insertedTextDigest: hash("TextHistory.insertText", input.insertText),
    author: input.author,
    provenance: input.provenance,
  }) else history.StaleTextRevision;

  return history.replacePayload(input);
}

intent openTextEditGroup(input: historyShape.OpenTextEditGroupInput)
  returns historyShape.TextEditGroupPayload
  implements history.openTextEditGroup
  basis input.historyId
  budget <= history.openTextEditGroupBudget
{
  let _opened = history.textEditGroupOpened.record({
    historyId: input.historyId,
    provenance: input.provenance,
  }) else history.TextEditGroupAlreadyOpen;

  return history.openGroupPayload(input);
}

intent includeTextEventInOpenGroup(
  input: historyShape.IncludeTextEventInOpenGroupInput
)
  returns historyShape.TextEditGroupPayload
  implements history.includeTextEventInOpenGroup
  basis input.historyId
  budget <= history.includeTextEventInOpenGroupBudget
{
  let _included = history.textEventIncludedInOpenGroup.record({
    historyId: input.historyId,
    eventId: input.eventId,
    provenance: input.provenance,
  }) else history.TextEditGroupMissing;

  return history.includeEventPayload(input);
}

intent closeTextEditGroup(input: historyShape.CloseTextEditGroupInput)
  returns historyShape.TextEditGroupPayload
  implements history.closeTextEditGroup
  basis input.historyId
  budget <= history.closeTextEditGroupBudget
{
  let _closed = history.textEditGroupClosed.record({
    historyId: input.historyId,
    provenance: input.provenance,
  }) else history.TextEditGroupMissing;

  return history.closeGroupPayload(input);
}

intent createTextCheckpoint(input: historyShape.CreateTextCheckpointInput)
  returns historyShape.CreateTextCheckpointPayload
  implements history.createTextCheckpoint
  basis input.revisionId
  budget <= history.createTextCheckpointBudget
{
  let _checkpoint = history.textCheckpointCreated.record({
    historyId: input.historyId,
    revisionId: input.revisionId,
    kind: input.kind,
    projectionPath: input.projectionPath,
    provenance: input.provenance,
  }) else history.TextCheckpointAlreadyExists;

  return history.checkpointPayload(input);
}

intent textHistorySnapshot(input: historyShape.TextHistorySnapshotInput)
  returns historyShape.TextHistorySnapshotReading
  implements history.textHistorySnapshot
  profile history.readOnly
  basis input.historyId
  budget <= history.textHistorySnapshotBudget
{
  let reading = history.textHistorySnapshot.read({
    historyId: input.historyId,
    cursorLine: input.cursorLine,
    viewportLineCount: input.viewportLineCount,
    beforeLines: input.beforeLines,
    afterLines: input.afterLines,
    maxBytes: input.maxBytes,
    includeEvents: input.includeEvents,
    includeEditGroups: input.includeEditGroups,
    includeCheckpoints: input.includeCheckpoints,
  }) else history.SnapshotObstructed;

  return reading;
}
```

The structural-history sketch is the strongest argument for keeping Edict in a
dedicated language repository. It is not Echo-shaped, but it should still lower
to Echo when an Echo target lawpack exists. It may also lower naturally to an
event-log target without graph semantics.

The snapshot intent uses a semantic query effect because a pure lawpack helper
cannot observe history state. If a future lawpack exposes a proof-only plan
constructor, it must be named and classified as a non-runtime symbolic plan; it
must not smuggle state-dependent output through a pure helper.

## Exported effect signature mismatch detail

An explicit mapped effect binding with incompatible authenticated input or receipt types reports `TypeMismatch` at the effect call span. Optional structured context identifies the input or receipt position and the deterministic first incompatible structural path. Expected input is the exported signature; expected receipt is the authored binding annotation. Actual input is the argument type; actual receipt is the exported output. Record fields compare in lexical depth-first order, missing/extra fields have an absent corresponding type, and nominal identity remains distinct. This diagnostic detail changes no type compatibility or effect authority.
