# Target Profiles Topic

Status: current HEAD contract.

This chapter describes the v1 target-profile manifest conformance contract that
exists today. It validates typed manifest values for runtime-neutral profile
metadata and atomic application doctrine before any target lowerer runs. It does
not load manifest files, lower Core to Target IR, run a verifier, or make an
admission claim. The separate authority-facts loader can load first compiler
context facts from digest-bound files whose source kind is `targetProfile`, but
that is not full target-profile manifest loading.

Edict also publishes the five runtime-neutral contract resources it owns in
every target profile: canonical encoding, component sandbox, fuel accounting,
diagnostics, and deterministic execution. These are canonical authority
artifacts with exact repository sources and domain-framed digests, not names
that permit callers to bless arbitrary bytes. [TPROF-REQ-010]

Provider manifests can describe target profiles and authority facts as generated
provider artifacts with digest-locked semantic-source and generator provenance.
This validates only the provider envelope and provenance lock; it does not load
or interpret target-profile runtime semantics. [TPROF-REQ-009]

## Public Surface

The `amblicit_syntax` crate exposes `validate_target_profile_manifest` and typed
target-profile data structures for:

- `TargetProfileManifest`, the typed `edict.target-profile/v1` manifest value;
- `TargetProfileConformanceReport`, including `Conformant` and `NonConformant`
  classifications;
- stable `TargetProfileConformanceFailureKind` categories. [TPROF-REQ-001]

The crate also exposes authority-facts loading for target-profile-sourced
operation-profile facts consumed by the compiler context. [TPROF-REQ-008]

`canonical_target_profile_contract_resources` returns the complete reviewed
resource set as explicit in-memory values.
`validate_target_profile_contract_resources` compares coordinate, provenance,
canonical bytes, and digest against Edict's compiled authority and returns
`ValidatedTargetProfileContractResources` only for a complete exact set.
`bind_manifest` on that sealed value applies the five exact references to a
runtime-owned `TargetProfileManifest`. [TPROF-REQ-010] [TPROF-REQ-011]
[TPROF-REQ-012]

Provider manifest validation can carry generated target-profile and
authority-facts artifact references without making Edict runtime-aware.
[TPROF-REQ-009]

The canonical artifact shape for `edict.target-profile/v1` is named in
[`docs/abi/edict-target-profile.cddl`](../../abi/edict-target-profile.cddl).
[TPROF-REQ-001]

## Current Contract

- `TargetProfileManifest` records profile identity, accepted Core ABI,
  intrinsic namespace, every required digest-locked manifest component,
  canonical encoding rules, direct lawpack-adapter ABI entries, diagnostics,
  v1 application doctrine, deterministic execution, and conformance fixture
  corpus references. [TPROF-REQ-001]
- Conformance is runtime-neutral. `echo.dpo@1` and `kv.transactional@1` shaped
  profiles are checked by the same manifest obligations; the checker does not
  require Echo, graph, database, event-log, repository, or storage runtime
  nouns. [TPROF-REQ-002]
- Every normative manifest component reference must carry a non-empty
  coordinate and valid `sha256:<64 hex>` digest review rendering.
  [TPROF-REQ-003]
- A conforming profile must accept `edict.core/v1`. [TPROF-REQ-004]
- `acceptedLawpackAdapterAbi` is absent/empty for profiles that do not consume
  lawpack adapters or exactly `["edict.lawpack-adapter/v1"]` for profiles that
  accept the direct declarative ABI. Unknown and duplicate claims reject.
  [TPROF-REQ-005]
- `multiTarget` remains false in v1 conformance until composite profile
  validation exists. [TPROF-REQ-006]
- The v1 application doctrine accepted by the checker is atomic application,
  application-snapshot reads, precommit-atomic guard evaluation, and
  no-visible-effects obstruction rollback. [TPROF-REQ-007]
- Authority-facts documents may identify a `targetProfile` source and provide
  operation-profile facts for the compiler context. This is not full
  target-profile manifest file loading. [TPROF-REQ-008]
- Provider manifests may identify target profiles and authority facts as
  generated artifacts. The provider validator checks lowercase digest locks for
  the artifact, semantic source, and generator, and rejects component
  provenance for target-profile metadata roles. [TPROF-REQ-009]
- The five Edict-owned resource coordinates resolve to checked canonical-CBOR
  policy documents under `fixtures/target-profile/contract-resources/`. Each
  digest uses the coordinate as its `edict.digest/v1` domain. Repeated
  generation is byte-identical, and a semantic mutation moves the digest.
  [TPROF-REQ-010]
- Resource validation consumes only explicit in-memory values. Missing,
  unknown, duplicate, non-canonical, byte-mismatched, digest-mismatched, or
  provenance-mismatched inputs reject with stable kinds; no file path is read
  and no registry, network, environment, or mutable name is consulted.
  [TPROF-REQ-011]
- A validated resource set binds exactly `sandbox`, `fuelModel`,
  `canonicalEncodingRules`, `diagnosticAbi`, and `deterministicExecution`.
  Runtime-owned semantic slots and provider-owned lowerer/verifier component
  selection remain unchanged. Echo-shaped and KV-shaped profiles continue
  through the same conformance checker after binding. [TPROF-REQ-012]

## Authority Flow

```text
caller-supplied bytes + expected digests + review provenance
  -> complete five-coordinate validation against Edict authority
  -> sealed ValidatedTargetProfileContractResources
  -> exact ResourceRef values in a runtime-owned target profile
```

The fixture path is review provenance, not a discovery handle. Digest and exact
byte comparison establish content identity, while complete-set validation
prevents a generator from silently omitting one Edict-owned policy. Provider
manifests still select the concrete lowerer and verifier components separately.

## Deferred

The following are not implemented by this target-profile slice:

- canonical-CBOR encode/decode helpers for `TargetProfileManifest`;
- full file-backed `edict.target-profile/v1` manifest loading;
- CDDL instance validation;
- intrinsic and operation-profile corpus parsing;
- target lowerers;
- verifier reports;
- file-backed integration with contract-bundle validation;
- admission tooling;
- multi-target composite profile validation beyond rejection.
- runtime distribution, registry lookup, or mutable-name resolution for the
  five contract resources.

The verification matrix is tracked in [test-plan.md](./test-plan.md).
