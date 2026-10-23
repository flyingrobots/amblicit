# Target Profiles Test Plan

Status: current verification design for v1 target-profile manifest conformance.

## Scope

In scope:

- typed `TargetProfileManifest` values;
- `validate_target_profile_manifest` conformance behavior;
- runtime-neutral acceptance of Echo and non-Echo target-profile shapes;
- digest-locked manifest component references;
- accepted Core ABI requirements;
- exact direct lawpack-adapter ABI compatibility;
- v1 atomic application doctrine.
- authority-facts documents whose source kind is `targetProfile` for first
  compiler operation-profile facts.
- provider manifests that describe target profiles as generated, digest-locked
  provider artifacts with explicit provenance.
- Edict-owned canonical contract resources for target-profile sandbox,
  fuel-accounting, diagnostics, deterministic-execution, and canonical-encoding
  slots;
- explicit, provenance-checked binding of those resources into runtime-owned
  target profiles.

Out of scope:

- canonical-CBOR encode/decode helpers for `TargetProfileManifest`;
- full file-backed `edict.target-profile/v1` manifest loading;
- full CDDL instance validation;
- intrinsic and operation-profile corpus parsing;
- target lowering;
- verifier reports;
- file-backed integration with contract-bundle validation;
- admission validation;
- multi-target composite profile validation.
- generating target profiles from Wesley or runtime-owned semantic sources.
- provider-owned lowerer or verifier component selection.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| TPROF-REQ-001 | implemented | `TargetProfileManifest` is a typed contract for `edict.target-profile/v1`, including profile identity, accepted Core ABI, intrinsic namespace, manifest component references, canonical encoding rules, diagnostics, v1 application doctrine, deterministic execution, and conformance fixture corpus. | issue #1, docs/abi/edict-target-profile.cddl |
| TPROF-REQ-002 | implemented | Target-profile conformance is runtime-neutral: Echo-shaped and KV-shaped profiles are checked by the same obligations without requiring graph/runtime-specific nouns. | issue #1 |
| TPROF-REQ-003 | implemented | Normative manifest component references must be digest-locked by non-empty coordinate and valid `sha256:<64 hex>` digest review renderings. | docs/abi/edict-target-profile.cddl |
| TPROF-REQ-004 | implemented | A conforming v1 target profile must accept `edict.core/v1`. | docs/abi/edict-target-profile.cddl |
| TPROF-REQ-005 | implemented | `acceptedLawpackAdapterAbi` is absent/empty or exactly `["edict.lawpack-adapter/v1"]`; unknown and duplicate claims reject. | EDICT-ABI-LAWPACK-ADAPTER-001 |
| TPROF-REQ-006 | implemented | `multiTarget: true` is rejected until composite profile validation exists. | ROADMAP.md |
| TPROF-REQ-007 | implemented | V1 conformance requires atomic application, application-snapshot reads, precommit-atomic guard evaluation, and no-visible-effects obstruction rollback. | docs/SPEC_edict-target-profile-abi-v1.md |
| TPROF-REQ-008 | implemented | Authority-facts loading accepts digest-locked `targetProfile` source identity for first compiler operation-profile facts without claiming full manifest loading. | docs/topics/authority-facts/test-plan.md |
| TPROF-REQ-009 | implemented | Provider manifests model target profiles and authority facts as generated provider artifacts with digest-locked semantic source and generator provenance; Edict validates the reference/provenance envelope without owning runtime-specific profile semantics. | issue #139, docs/topics/providers/test-plan.md |
| TPROF-REQ-010 | implemented | Edict publishes one canonical, domain-digested, runtime-neutral contract artifact for each Edict-owned target-profile resource coordinate: `edict.canonical-cbor/v1`, `edict.wasm-component/v1`, `edict.fuel/v1`, `edict.diagnostics/v1`, and `edict.determinism/v1`. | EDICT-ABI-TARGET-PROFILE-RESOURCES-001, issue #158 |
| TPROF-REQ-011 | implemented | Runtime-owned generators receive all five resources as explicit in-memory inputs; validation rejects missing, unknown, duplicate, non-canonical, byte-mismatched, digest-mismatched, or provenance-mismatched resources without filesystem, registry, network, environment, or mutable-name discovery. | EDICT-ABI-TARGET-PROFILE-RESOURCES-001, issue #158 |
| TPROF-REQ-012 | implemented | Only a completely validated Edict contract-resource set can bind the five Edict-owned slots of a target-profile manifest, and binding preserves runtime-neutral conformance. | EDICT-ABI-TARGET-PROFILE-RESOURCES-001, issue #158 |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| crates/amblicit-syntax/tests/target_profile.rs | In-memory typed Echo and KV target-profile manifests plus negative manifest mutations. | The same checker accepts both runtime-neutral positive shapes and rejects only the mutated obligations with stable failure kinds. |
| crates/amblicit-syntax/tests/target_profile_contract_resources.rs | Canonical Edict contract-resource generation, validation, mutation, ordering, and target-profile binding cases. | Tests compare exact resources, structured failure kinds, domain-framed digests, and the resulting conforming runtime-neutral manifest. |
| fixtures/target-profile/contract-resources/README.md | Reviewed canonical bytes and digest renderings for all five Edict-owned target-profile resources. | `cargo xtask target-profile-resource-goldens --check` regenerates every file from the executable semantic model and requires byte equality. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TPROF-TP-001 | implemented | Golden path | TPROF-REQ-001, TPROF-REQ-002 | Echo-shaped and KV-shaped manifests both return `TargetProfileConformanceStatus::Conformant` with no failures. | echo_and_kv_profiles_conform_to_the_same_runtime_neutral_manifest_contract | crates/amblicit-syntax/tests/target_profile.rs | Proves the checker is not Echo-specific. |
| TPROF-TP-002 | implemented | Boundary guard | TPROF-REQ-003 | Removing or malforming the verifier digest returns `NonConformant` with `NonDigestLockedResource` on the `verifier` field. | missing_digest_on_normative_manifest_slot_is_rejected, malformed_digest_on_normative_manifest_slot_is_rejected | crates/amblicit-syntax/tests/target_profile.rs | Stable failure kind and field, not prose. |
| TPROF-TP-003 | implemented | Boundary guard | TPROF-REQ-004 | Removing `edict.core/v1` from accepted Core ABI returns `MissingAcceptedCoreAbi`. | accepted_core_abi_must_include_v1_core | crates/amblicit-syntax/tests/target_profile.rs | Ensures target profiles declare the Core contract they accept. |
| TPROF-TP-004 | implemented | Compatibility | TPROF-REQ-005 | The exact direct adapter ABI is accepted; unknown and duplicate ABI claims return `UnsupportedLawpackAdapterAbi`. | direct_lawpack_adapter_abi_is_supported_in_v1, unknown_or_duplicate_lawpack_adapter_abis_are_rejected | crates/amblicit-syntax/tests/target_profile.rs | Keeps target claims closed over the one implemented ABI. |
| TPROF-TP-005 | implemented | Boundary guard | TPROF-REQ-006 | Setting `multiTarget` true returns `UnsupportedCompositeProfile`. | multi_target_profiles_are_rejected_until_composite_validation_exists | crates/amblicit-syntax/tests/target_profile.rs | Prevents unvalidated composite profiles from passing v1 conformance. |
| TPROF-TP-006 | implemented | Boundary guard | TPROF-REQ-007 | Non-atomic application doctrine returns stable failure kinds for application model, read consistency, guard evaluation, and rollback. | atomic_application_semantics_are_required_for_v1_conformance | crates/amblicit-syntax/tests/target_profile.rs | Asserts structured behavior only. |
| TPROF-TP-007 | implemented | Authority facts | TPROF-REQ-008 | A target-profile-sourced authority-facts file can provide operation-profile facts consumed by the compiler. | file_backed_authority_facts_compile_bounded_hello, file_backed_authority_facts_reject_write_effect_profile_mismatch | crates/amblicit-syntax/tests/authority_facts.rs | Asserts compiler behavior, not manifest prose. |
| TPROF-TP-008 | implemented | Provider provenance | TPROF-REQ-009 | A provider manifest fixture can carry generated target-profile and authority-facts artifacts with digest-locked semantic source and generator provenance, while component-sourced metadata roles reject with stable provider validation failures. | generated_provider_manifest_fixture_validates, provider_manifest_rejects_component_metadata_roles | fixtures/providers/echo-generated/provider-manifest.json, crates/amblicit-syntax/tests/provider.rs | Provider validation is envelope/provenance validation only; no Echo target-profile semantics are interpreted. |
| TPROF-TP-009 | implemented | Golden path | TPROF-REQ-010 | Repeated generation returns the same five canonical artifacts in coordinate order with stable lowercase domain-framed SHA-256 digests and exact repository-source provenance. | canonical_contract_resources_are_complete_and_reproducible | crates/amblicit-syntax/tests/target_profile_contract_resources.rs | The artifacts contain structured policy, not illustrative coordinate-only payloads. |
| TPROF-TP-010 | implemented | Digest sensitivity | TPROF-REQ-010, TPROF-REQ-011 | A canonical semantic mutation moves the coordinate-framed digest but remains rejected as non-authoritative even when the caller supplies that recomputed digest. | semantic_mutation_moves_digest_without_gaining_authority | crates/amblicit-syntax/tests/target_profile_contract_resources.rs | Digest computation is identity evidence, not authority to mint a replacement Edict contract. |
| TPROF-TP-011 | implemented | Boundary guard | TPROF-REQ-011 | Wrong coordinate, bytes, digest, or provenance plus missing, duplicate, and unknown resources reject with exact stable failure kinds before a binding token exists. | contract_resource_boundary_rejects_every_identity_mismatch | crates/amblicit-syntax/tests/target_profile_contract_resources.rs | Inputs are explicit values; the validator performs no discovery. |
| TPROF-TP-012 | implemented | Determinism | TPROF-REQ-011 | Reversing explicit input order produces the same validated coordinate-ordered resource set. | contract_resource_validation_is_input_order_independent | crates/amblicit-syntax/tests/target_profile_contract_resources.rs | Input collection order is non-semantic. |
| TPROF-TP-013 | implemented | Compatibility | TPROF-REQ-012 | A validated resource set replaces all five Edict-owned target-profile slots with exact canonical references and the resulting Echo-shaped and KV-shaped manifests remain conformant under the same runtime-neutral checker. | validated_contract_resources_bind_runtime_neutral_profiles | crates/amblicit-syntax/tests/target_profile_contract_resources.rs | Runtime-owned fields and provider component selection remain untouched. |
| TPROF-TP-014 | implemented | Fixture drift | TPROF-REQ-010 | Check mode regenerates every reviewed resource byte file and digest from the executable semantic model and fails on any mismatch. | target_profile_resource_goldens_match_executable_contract | xtask/src/tests.rs, fixtures/target-profile/contract-resources/README.md | The full local gate runs the same check mode. |

## Determinism Obligations

- Manifests are built from in-memory constants.
- Assertions use structured statuses, failure kinds, and stable fields.
- Manifest conformance tests do not inspect stdout, stderr, diagnostic prose,
  filesystem ordering, network state, or wall-clock time. Contract-resource
  tests intentionally compare their public canonical bytes and digests.
- The contract graph is checked by `cargo xtask contract-check`.
- Contract-resource generation and validation operate only on supplied in-memory
  values and normalize non-semantic input order by coordinate.
- Golden checks compare exact canonical bytes and coordinate-framed digests for
  every Edict-owned target-profile resource.

## Open Gaps

- Canonical-CBOR encode/decode helpers for `edict.target-profile/v1`.
- Full file-backed target-profile manifest loading and conformance fixtures.
- Full CDDL instance validation.
- Intrinsic and operation-profile corpus parsing.
- Target lowerers and verifier reports.
- File-backed integration with contract-bundle validation.
- Admission validation.
- Multi-target composite profile validation beyond explicit v1 rejection.
- Runtime distribution or registry lookup for target-profile contract
  resources; callers must supply trusted bytes and expected digests explicitly.
