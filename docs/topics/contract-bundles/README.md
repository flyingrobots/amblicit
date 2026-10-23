# Contract Bundles Topic

Status: current HEAD contract.

This chapter describes the v1 participant-neutral contract bundle manifest
validation and assembly contract that exists today. It validates typed bundle
values after Core and target lowering have produced hash-addressed artifacts,
and it can assemble a `ContractBundleManifest` from a real `CoreModule` plus
either supplied digest-locked references or a real `TargetIrArtifact` whose
digest is computed by the assembler. It does not load bundle files, run
lowerers or verifiers, validate CDDL instances, or make an admission claim.

## Public Surface

The `amblicit_syntax` crate exposes `validate_contract_bundle_manifest` and typed
contract-bundle data structures for:

- `ContractBundleManifest`, the typed `edict.contract-bundle/v1` manifest value;
- `assemble_contract_bundle`, `ContractBundleAssemblyInput`, and typed supplied
  digest/resource wrappers for constructing a manifest and recomputing the
  semantic and release bundle digests;
- `assemble_contract_bundle_from_target_ir` and
  `ContractBundleAssemblyFromTargetIrInput` for constructing a manifest from a
  real Target IR artifact whose digest is computed from canonical Target IR
  bytes;
- `ContractBundleValidationReport`, including `Valid` and `Invalid`
  classifications;
- stable `ContractBundleValidationFailureKind` categories;
- `BundleSubject`, `BundleSubjectKind`, `AssuranceEvidenceRef`, and
  `AssuranceRole` for participant-neutral HOLMES, Watson, and Moriarty evidence
  references when those external assurance artifacts are bundled.
  [BUNDLE-REQ-001]

The canonical artifact identity rules are named in
[`docs/SPEC_continuum-contract-bundle-v1.md`](../../SPEC_continuum-contract-bundle-v1.md).
[BUNDLE-REQ-001]

## Current Contract

- `ContractBundleManifest` records semantic and release bundle digests plus
  source, source-profile facts, Core IR, target-profile, target-IR, lawpack,
  generated artifact, compiler, lowerer, verifier, semantic and nonsemantic
  compile-option, build-provenance, canonicalization-profile,
  conformance-corpus, verifier-report, compile-explanation, and assurance
  evidence references. [BUNDLE-REQ-001]
- Validation is runtime-neutral. Echo-shaped and KV-shaped bundles are checked
  by the same obligations; the checker does not require graph, database,
  event-log, repository, or storage runtime nouns. [BUNDLE-REQ-002]
- Every hash-significant artifact reference must carry a non-empty coordinate and
  valid lowercase `sha256:<64 lowercase hex>` digest review rendering.
  [BUNDLE-REQ-003]
- Source artifact paths must be logical package-relative paths. Absolute paths,
  drive-letter paths, backslashes, empty paths, current-directory segments, and
  parent-directory segments are rejected by both manifest validation and the
  bundle digest preimage helper. [BUNDLE-REQ-004]
- HOLMES, Watson, and Moriarty evidence is optional in the typed bundle. When an
  evidence entry is present, it must bind to the manifest's selected bundle
  subject digest, target profile digest, and target IR digest. [BUNDLE-REQ-005]
- Admission artifacts remain outside the participant-neutral contract bundle
  manifest. Non-empty admission references are rejected. [BUNDLE-REQ-006]
- The canonicalization profile coordinate is pinned to `edict.canonical-cbor/v1`.
  [BUNDLE-REQ-007]
- `assemble_contract_bundle` computes `coreIrDigest` from the supplied
  `CoreModule` with `digest_core_module`; callers cannot supply an alternate
  Core digest. The assembler validates its generated manifest before returning
  it and rejects inputs that would produce an invalid required bundle structure.
  [BUNDLE-REQ-008]
- `assemble_contract_bundle_from_target_ir` computes `targetIrDigest` from a
  real `TargetIrArtifact` with `digest_target_ir_artifact`, writes that digest
  into `manifest.target_ir.digest`, and uses the same digest in the semantic
  bundle preimage. The target profile reference is derived from the artifact's
  digest-locked `target_profile`. Before assembly, this path recomputes the
  semantic closure from the supplied Core when Core declares one and requires
  the artifact closure to match its exact Core identity and lawpack set. When
  Core declares no closure, a provider-supplied artifact closure is accepted
  only when it binds that exact Core and its lawpack set equals the explicit
  bundle lawpacks. Missing or substituted bindings remain rejected. Equivalent
  bundle lawpack order and duplicate identical references normalize to the
  closure's canonical set before bundle identity is computed. Malformed Core
  retains the same canonical-digest error classification as the direct
  assembly path. This is cross-artifact corroboration at the computed assembly
  boundary, not a runtime admission claim. [BUNDLE-REQ-008]
  [BUNDLE-REQ-010]
- `assemble_contract_bundle` remains available for already-digested external
  Target IR references. On that supplied-reference path, the same typed Target
  IR resource supplies both `manifest.target_ir.digest` and the semantic bundle
  digest preimage. [BUNDLE-REQ-008]
- `semanticBundleDigest` is recomputed from the exact ordered semantic preimage:
  computed Core digest, target profile digest, supplied Target IR digest,
  lawpack digests, source-profile semantic facts digest, generated artifact
  digests, canonicalization profile digest, semantic compile options digest,
  conformance fixture corpus digests, and verifier report digest.
  [BUNDLE-REQ-008]
- `releaseBundleDigest` is recomputed from the semantic bundle digest plus
  structured release-only provenance: source logical paths and artifact
  references, compiler/lowerer/verifier coordinates and digests, nonsemantic
  compile options, build provenance, and compile explanation. Coordinate and
  logical-path changes move the release digest even when digest bytes stay the
  same. [BUNDLE-REQ-008]
- Optional assurance evidence is generated with subject, target-profile, and
  target-IR bindings when supplied, but it is not a top-level semantic or
  release digest preimage component. Admission artifacts remain rejected and are
  not emitted by the assembler. [BUNDLE-REQ-006] [BUNDLE-REQ-008]
- Target IR produced through the built-in lowerer compatibility seam
  yields the same semantic and release bundle identities as direct lowering when
  every explicit assembly input, including lowerer identity, is unchanged.
  Changing only the explicit lowerer identity leaves semantic identity stable
  and changes release identity. Bundle assembly still consumes an artifact; it
  does not invoke a lowerer. [BUNDLE-REQ-009]
- The v0.11 bundle digest golden in
  `fixtures/bundle/assembly/bounded-hello.bundle-digests.txt` freezes the
  semantic/release bundle preimage shape and resulting digest review strings.
  `cargo xtask bundle-goldens --check` regenerates it from executable assembly.
  [BUNDLE-REQ-008]

## Deferred

The following are not implemented by this contract-bundle slice:

- canonical-CBOR encode/decode helpers for `ContractBundleManifest`;
- file-backed bundle loading;
- full CDDL instance validation;
- target lowerer or verifier execution;
- admission request, receipt, policy, catalog, participant descriptor, and
  signature validation;
- distribution envelope validation.

The verification matrix is tracked in [test-plan.md](./test-plan.md).
