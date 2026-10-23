# Contract Bundle Assembly — v0.11 Assembly Boundary

Status: design record for the `v0.11.0-alpha.1` contract-bundle assembly slice.
This is a scope boundary note, not a bundle manifesto. The normative contract is
[SPEC - Continuum Contract Bundle v1](../SPEC_continuum-contract-bundle-v1.md);
the existing validation surface is in `crates/amblicit-syntax/src/contract_bundle.rs`.

## Why this note exists

Before v0.11 the crate could *validate* a hand-written `ContractBundleManifest`
but could not *assemble* one. v0.11 moves bundle digest recomputation from
"out of scope" to implemented behavior. The one thing this slice must not do is
silently absorb a second, unrelated freeze — **canonical Target IR byte
identity** — under bundle pressure. That freeze belongs to the target-IR track
and was completed separately in
[Canonical Target IR v0.11](./canonical-target-ir-v0.11.md).

## 1. Scope

This slice implements **bundle digest derivation and assembly**:

- The assembler **computes** `semanticBundleDigest` and `releaseBundleDigest`
  from digest-locked artifact references, using the existing
  `edict.digest/v1` domain-separated SHA-256 framing (the same primitive that
  produces the Core digest).
- The assembler **computes** `coreIrDigest` from the actual compiled Core
  artifact (`digest_core_module`), so the bundle is anchored on a real Core
  freeze.
- The supplied-reference assembler accepts `targetIrDigest` and every other
  layer hash (lawpacks, source-profile semantic facts, generated artifacts,
  canonicalization profile, semantic/nonsemantic compile options, conformance
  fixture corpora, verifier report, compiler/lowerer/verifier identities, source
  provenance, build provenance, compile explanation) as **supplied,
  digest-locked references**.
- After the Target IR canonical-byte freeze, the computed-artifact assembler can
  instead compute `targetIrDigest` from a real `TargetIrArtifact` and derive the
  target-profile reference from that artifact. Before the computed digest enters
  the bundle preimage, the assembler recomputes the semantic closure from the
  supplied `CoreModule`, requires the artifact's Core identity and lawpack set to
  match it, and requires the bundle lawpack set to equal that closure. Equivalent
  lawpack order and duplicate identical references normalize to the closure's
  canonical set before the bundle preimage is computed. A malformed Core keeps
  the direct assembly path's canonical-digest failure category.
- The assembled manifest is consumed by the existing
  `validate_contract_bundle_manifest` (exit gate: validation consumes the
  assembled artifact, not a hand-written fixture).

The two bundle preimages are exact (`CONTINUUM-BUNDLE-SUBJECT-001`):

```text
semanticBundleDigest = digest("edict.bundle.semantic/v1", [
  coreIrDigest, targetProfileDigest, targetIrDigest, lawpackDigests,
  sourceProfileSemanticFactsDigest, generatedArtifactDigests,
  canonicalizationProfileDigest, semanticCompileOptionsDigest,
  conformanceFixtureCorpusDigests, verifierReportDigest ])

releaseBundleDigest = digest("edict.bundle.release/v1", [
  semanticBundleDigest, rawSourceArtifactDescriptors,
  compilerIdentityAndDigest, lowererIdentityAndDigest, verifierIdentityAndDigest,
  nonSemanticCompileOptionsDigest, buildProvenance, compileExplanationDigest ])
```

`releaseBundleDigest` references `semanticBundleDigest`, never the reverse
(`CONTINUUM-BUNDLE-DAG-001`).

### 1a. Canonical preimage byte shape

The assembler does not hash `sha256:<hex>` review strings directly. For bundle
digest preimages, every review digest is parsed into the authoritative canonical
digest value `["sha256", h'..32 bytes..']` (the same typed `[algorithm, bytes]`
shape the canonical encoder already uses). Each bundle digest is then SHA-256
over:

```text
canonical-cbor([
  "edict.digest/v1",
  "<bundle-domain>",
  <typed bundle preimage value without self digest>
])
```

where `<bundle-domain>` is `edict.bundle.semantic/v1` or
`edict.bundle.release/v1`, and the typed bundle preimage value is the ordered
list of component digests (each a typed `["sha256", <bytes>]` value), never the
human review strings. This mirrors the Core digest path exactly, which frames the
canonical Core module value inside
`["edict.digest/v1", "edict.core.module/v1", <canonical Core module value>]`.
This is the byte-level contract, not just an ingredient list.

### Provenance is typed, not commented

The assembly input APIs make computed-versus-supplied **unrepresentable as a
mistake**: the Core digest enters as a *computed* value derived from a real
`CoreModule`; supplied hashes enter through distinct *supplied reference* types;
and the computed Target IR path has no supplied target-IR digest field.

## 2. Non-claim

The bundle assembly slice deliberately does **not**:

- load files, run target verifiers, or perform admission.

The original supplied-reference path detects target-IR **digest-reference**
changes in the bundle graph: if the supplied `targetIrDigest` reference changes,
the bundle digests change. The computed Target IR path additionally rehashes
canonical Target IR artifact bytes and corroborates its semantic closure against
the exact supplied Core and lawpacks before the digest enters the bundle
preimage. Standalone Target IR canonical encoding remains deterministic
self-validation; it cannot reconstruct dependencies erased by a caller.

## 3. Target IR freeze

[#105](https://github.com/flyingrobots/edict/issues/105) freezes canonical
Target IR bytes/digests on the target-IR track: a canonical CBOR/value model,
byte fixtures, a digest function, reviewed goldens, and integration into
assembly so `targetIrDigest` can be **computed** from a real artifact.

## 4. Test matrix

Mutation sensitivity honors the spec's semantic/release split — not every
mutation changes everything (`CONTINUUM-SEMANTIC-OPTIONS-001`,
`EDICT-CORE-NODIAG-001`):

Top-level digest mutation tests apply to **semantic/release preimage
components**. Excluded external artifacts, optional assurance evidence, and
admission artifacts are handled by validation, subject binding, or explicit
rejection — not by pretending every nearby artifact changes the bundle digest.

| Mutation | semanticBundleDigest | releaseBundleDigest |
| --- | --- | --- |
| Core semantic change (Core digest changes) | changes | changes |
| Target IR artifact digest/reference changed | changes | changes |
| target profile / lawpack / generated / fixture-corpus / verifier-report digest | changes | changes |
| `sourceProfileSemanticFactsDigest` changed | changes | changes |
| `canonicalizationProfileDigest` changed | changes | changes |
| semantic compile options digest | changes | changes |
| provenance-only source digest / logical path | unchanged | changes |
| compiler / lowerer / verifier identity (artifacts unchanged) | unchanged | changes |
| nonsemantic compile options digest | unchanged | changes |
| build provenance | unchanged | changes |
| compile explanation digest | unchanged | changes |
| optional assurance evidence artifact changed | unchanged | unchanged (no top-level digest claim) |
| assurance evidence subject / target mismatch | rejected | rejected |
| admission artifact inserted | rejected | rejected |

Notes: semantic source *edits* are covered by the Core-digest row (Core digest
changes ⇒ both change); the provenance-only row is exactly that — provenance,
not meaning. A lowerer/verifier identity change is release-only *only when the
produced artifacts are unchanged*; if the target IR artifact digest also
changes, the semantic digest changes through `targetIrDigest`. Optional
assurance evidence and admission artifacts are not top-level preimage
components, so they are governed by validation/subject-binding/rejection, not by
digest propagation.

Plus a round-trip: `assemble_contract_bundle(...)` →
`validate_contract_bundle_manifest` returns `Valid`.

The bundle digest goldens are checked in with an `xtask` check/regenerate path,
mirroring the Core golden discipline. **This slice freezes the v0.11 bundle
digest preimage shape and the resulting semantic/release golden digest values.
It does not freeze canonical `ContractBundleManifest` bytes** unless that
manifest encoder is explicitly added as a separate reviewed scope item.
Target-IR byte identity is frozen separately by the Target IR canonical-byte
slice.

## Documentation discipline

Per `AGENTS.md`: planned cases land in the contract-bundles **test plan** first
(observed RED), behavior is implemented (GREEN), and only then are the planned
rows marked implemented and the topic **README** updated to describe current-HEAD
truth. No README prose describes intended behavior as current truth before it
lands.
