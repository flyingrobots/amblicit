//! Canonical Rust-neutral provider contract-pack assembly and validation.

use std::collections::BTreeMap;

use cddl_cat::flatten::flatten_from_str;
use edict_provider_schema::{
    assemble_provider_contract_pack, validate_provider_contract_pack_manifest,
    ProviderContractInstanceValidationErrorKind, ProviderContractPack,
    ProviderContractPackFailureKind, ProviderContractPackInput, PROVIDER_CONTRACT_PACK_API_VERSION,
    PROVIDER_CONTRACT_PACK_COORDINATE, PROVIDER_CONTRACT_PACK_LICENSE,
    TARGET_IR_ARTIFACT_CDDL_ROOT,
};
use edict_syntax::{
    canonical_target_profile_contract_resources, compile_to_core, decode_canonical_cbor,
    digest_target_profile_contract_resource, encode_core_module, encode_result_projection,
    encode_target_ir_artifact, parse_module, CanonicalValue, CompilerContext, CoreBudget, CoreExpr,
    CoreExternalActionBudget, CoreObstructionReason, CorePredicate, CoreValue, LocalRef,
    ProviderArtifactSchemaValidationErrorKind, ResourceRef, ResultProjection, ResultProjectionExpr,
    ResultProjectionSource, TargetIrArtifact, TargetIrExternalActionRequest, TargetIrIntent,
    TargetIrPureBinding, TargetIrRequireFailure, TargetIrRequirement, TargetIrSemanticClosure,
    TargetProfileContractResource, WriteClass, AUTHORITY_FACTS_API_VERSION,
    CORE_MODULE_DIGEST_DOMAIN, PROVIDER_LAWPACK_ARTIFACT_DOMAIN, RESULT_PROJECTION_DIGEST_DOMAIN,
    TARGET_IR_ARTIFACT_DIGEST_DOMAIN, TARGET_PROFILE_API_VERSION,
};
use sha2::{Digest, Sha256};

const COMMON_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-common.cddl");
const CORE_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-core.cddl");
const LAWPACK_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-lawpack.cddl");
const LAWPACK_ADAPTER_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-lawpack-adapter.cddl");
const TARGET_PROFILE_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-target-profile.cddl");
const AUTHORITY_FACTS_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-authority-facts.cddl");
const RESULT_PROJECTION_CDDL: &[u8] =
    include_bytes!("../../../docs/abi/edict-result-projection.cddl");
const TARGET_IR_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-target-ir.cddl");
const CORE_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/core/canonical/bounded-hello.core.cbor");
const EXTERNAL_REQUEST_CORE_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/core/canonical/workspace-snapshot.core.cbor");
const AUTHORITY_FACTS_FIXTURE: &[u8] = include_bytes!(
    "../../../fixtures/authority-facts/canonical/example-effectful.authority-facts.cbor"
);
const TARGET_IR_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/target-ir/canonical/echo-effectful.target-ir.cbor");
const ALTERNATE_TARGET_IR_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/target-ir/canonical/gitwarp-append.target-ir.cbor");
const EXTERNAL_REQUEST_TARGET_IR_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/target-ir/canonical/workspace-snapshot.target-ir.cbor");
const LAWPACK_ADAPTER_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor");
const RESULT_PROJECTION_FIXTURE: &[u8] =
    include_bytes!("../../../fixtures/lawpack/hello-echo/create-greeting.result-projection.cbor");
const OPERATION_SOURCE: &str =
    include_str!("../../../fixtures/lang/operations/explicit-basis-u64.edict");
const EXPECTED_DOMAIN_BINDINGS: [(&str, &str); 8] = [
    (AUTHORITY_FACTS_API_VERSION, "authority-facts"),
    (CORE_MODULE_DIGEST_DOMAIN, "core-module"),
    (PROVIDER_LAWPACK_ARTIFACT_DOMAIN, "lawpack-manifest"),
    ("edict.lowering-requirements/v1", "lowering-requirements"),
    (RESULT_PROJECTION_DIGEST_DOMAIN, "result-projection"),
    ("edict.source-closure/v1", "source-closure"),
    (TARGET_IR_ARTIFACT_DIGEST_DOMAIN, "target-ir-artifact"),
    (TARGET_PROFILE_API_VERSION, "target-profile-manifest"),
];

#[test]
fn contract_pack_is_self_contained_and_repeatable() {
    let forward = assemble(canonical_target_profile_contract_resources());
    let mut reversed_resources = canonical_target_profile_contract_resources();
    reversed_resources.reverse();
    let reversed = assemble(reversed_resources);

    assert_eq!(forward.cddl_bytes(), reversed.cddl_bytes());
    assert_eq!(forward.manifest_bytes(), reversed.manifest_bytes());
    let expected_pack_digest: [u8; 32] = Sha256::digest(forward.cddl_bytes()).into();
    assert_eq!(forward.raw_sha256(), &expected_pack_digest);
    assert_eq!(
        forward.manifest().api_version,
        PROVIDER_CONTRACT_PACK_API_VERSION
    );
    assert_eq!(
        forward.manifest().coordinate,
        PROVIDER_CONTRACT_PACK_COORDINATE
    );
    assert_eq!(forward.manifest().license, PROVIDER_CONTRACT_PACK_LICENSE);
    assert_eq!(forward.manifest().contracts.len(), 12);
    assert_eq!(forward.manifest().domains.len(), 8);
    assert_eq!(forward.manifest().resources.len(), 5);
    assert!(forward
        .cddl_bytes()
        .starts_with(b"; SPDX-License-Identifier: Apache-2.0\n"));
    assert_sorted_unique(
        forward
            .manifest()
            .contracts
            .iter()
            .map(|binding| binding.contract.as_str()),
    );
    assert_sorted_unique(
        forward
            .manifest()
            .domains
            .iter()
            .map(|binding| binding.domain.as_str()),
    );
    assert_eq!(
        forward
            .manifest()
            .domains
            .iter()
            .map(|binding| (binding.domain.as_str(), binding.root_rule.as_str()))
            .collect::<Vec<_>>(),
        EXPECTED_DOMAIN_BINDINGS
    );
    assert_sorted_unique(
        forward
            .manifest()
            .resources
            .iter()
            .map(|resource| resource.coordinate.as_str()),
    );

    let manifest_bytes = forward.manifest_bytes();
    assert!(manifest_bytes.ends_with(b"\n"));
    assert!(!manifest_bytes.ends_with(b"\n\n"));
    let manifest = std::str::from_utf8(&manifest_bytes).expect("manifest JSON is UTF-8");
    assert!(manifest.contains("\"license\": \"Apache-2.0\""));
    assert!(manifest.contains("\"rawSha256\""));
    assert!(manifest.contains("\"domainFramedDigest\""));

    let schema = std::str::from_utf8(forward.cddl_bytes()).expect("pack CDDL is UTF-8");
    let rules = flatten_from_str(schema).expect("pack has no unresolved external CDDL rules");
    for binding in &forward.manifest().contracts {
        assert!(
            rules.contains_key(&binding.root_rule),
            "missing compiled root {}",
            binding.root_rule
        );
    }
    for resource in &forward.manifest().resources {
        let expected_resource_digest: [u8; 32] = Sha256::digest(&resource.canonical_bytes).into();
        assert_eq!(resource.raw_sha256, expected_resource_digest);
        assert!(resource.domain_framed_digest.starts_with("sha256:"));
    }
}

#[test]
fn every_published_root_validates_reference_and_rejects_mutation() {
    assert_eq!(TARGET_IR_ARTIFACT_CDDL_ROOT, "target-ir-artifact");
    let pack = assemble(canonical_target_profile_contract_resources());
    let contracts = representative_contract_instances();

    assert_eq!(contracts.len(), pack.manifest().contracts.len());
    for (contract, value) in contracts {
        pack.validate_contract(contract, &value)
            .unwrap_or_else(|error| panic!("{contract} rejected representative value: {error:?}"));
        assert_eq!(
            pack.validate_contract(contract, &CanonicalValue::Null),
            Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
            "{contract} accepted a structurally invalid value"
        );
    }
    assert_eq!(
        pack.validate_contract("unknown-contract", &CanonicalValue::Null),
        Err(ProviderContractInstanceValidationErrorKind::UnknownContract)
    );

    let contracts = representative_contract_instances();
    for (domain, root) in EXPECTED_DOMAIN_BINDINGS {
        let contract = pack
            .manifest()
            .contracts
            .iter()
            .find(|binding| binding.root_rule == root)
            .unwrap_or_else(|| panic!("missing logical contract for domain root {root}"));
        let value = contracts
            .iter()
            .find_map(|(name, value)| (name == &contract.contract).then_some(value))
            .unwrap_or_else(|| panic!("missing representative value for {domain}"));
        pack.validate_domain(domain, value)
            .unwrap_or_else(|error| panic!("{domain} rejected representative value: {error:?}"));
        assert_eq!(
            pack.validate_domain(domain, &CanonicalValue::Null),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
            "{domain} accepted a structurally invalid value"
        );
    }
    assert_eq!(
        pack.validate_domain("runtime.unknown/v1", &CanonicalValue::Null),
        Err(ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain)
    );
}

#[test]
fn core_root_matches_reference_encoder() {
    let pack = assemble(canonical_target_profile_contract_resources());
    for fixture in [CORE_FIXTURE, EXTERNAL_REQUEST_CORE_FIXTURE] {
        let value = decode_canonical_cbor(fixture).expect("reviewed Core is canonical");
        pack.validate_domain(CORE_MODULE_DIGEST_DOMAIN, &value)
            .expect("reviewed Core satisfies the Edict-owned root");
    }
}

#[test]
fn target_ir_root_matches_reference_encoder() {
    let pack = assemble(canonical_target_profile_contract_resources());
    for fixture in [
        TARGET_IR_FIXTURE,
        ALTERNATE_TARGET_IR_FIXTURE,
        EXTERNAL_REQUEST_TARGET_IR_FIXTURE,
    ] {
        let value = decode_canonical_cbor(fixture).expect("reviewed Target IR is canonical");
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &value)
            .expect("reviewed Target IR satisfies the Edict-owned root");
    }
    let encoded_requirements = encoded_target_ir_with_requirements();
    pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &encoded_requirements)
        .expect("encoder output with both requirement dispositions satisfies the root");

    let encoded_request = encoded_target_ir_with_external_request();
    pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &encoded_request)
        .expect("encoder output with an external request satisfies the closed root");
    let mut request_without_closure = encoded_request;
    remove_map_field(&mut request_without_closure, "semanticClosure");
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &request_without_closure),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "external requests cannot validate through the legacy closure-free root"
    );

    let mut missing_semantic_closure = encoded_target_ir_with_coordinate("example.target@1");
    remove_map_field(&mut missing_semantic_closure, "semanticClosure");
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &missing_semantic_closure),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "basis-bearing external Target IR must carry its semantic closure"
    );

    let mut invalid = decode_canonical_cbor(TARGET_IR_FIXTURE).expect("fixture is canonical");
    *map_value_mut(&mut invalid, "kind") = CanonicalValue::Text("targetIrDraft".to_owned());
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &invalid),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );

    let mut missing_field = decode_canonical_cbor(TARGET_IR_FIXTURE).expect("fixture is canonical");
    remove_map_field(&mut missing_field, "sourceCoreCoordinate");
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &missing_field),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );

    let mut empty_source_core =
        decode_canonical_cbor(TARGET_IR_FIXTURE).expect("fixture is canonical");
    *map_value_mut(&mut empty_source_core, "sourceCoreCoordinate") = text("");
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &empty_source_core),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "published Target IR schema must reject an empty source Core coordinate"
    );

    let mut empty_target_profile =
        decode_canonical_cbor(TARGET_IR_FIXTURE).expect("fixture is canonical");
    let target_profile = map_value_mut(&mut empty_target_profile, "targetProfile");
    *map_value_mut(target_profile, "id") = CanonicalValue::Text(String::new());
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &empty_target_profile),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );

    let mut malformed_nested =
        decode_canonical_cbor(TARGET_IR_FIXTURE).expect("fixture is canonical");
    let CanonicalValue::Map(intents) = map_value_mut(&mut malformed_nested, "intents") else {
        panic!("Target IR intents must be a map");
    };
    let intent = &mut intents
        .first_mut()
        .expect("reviewed Target IR has an intent")
        .1;
    *map_value_mut(intent, "steps") = CanonicalValue::Null;
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &malformed_nested),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );

    let mut malformed_requirement = encoded_target_ir_with_requirements();
    let CanonicalValue::Map(intents) = map_value_mut(&mut malformed_requirement, "intents") else {
        panic!("Target IR intents must be a map");
    };
    let intent = &mut intents
        .first_mut()
        .expect("encoded Target IR has an intent")
        .1;
    let CanonicalValue::Array(requirements) = map_value_mut(intent, "requirements") else {
        panic!("Target IR requirements must be an array");
    };
    let requirement = requirements
        .first_mut()
        .expect("encoded Target IR has requirements");
    *map_value_mut(requirement, "onFailure") = CanonicalValue::Null;
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &malformed_requirement),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );

    assert_eq!(
        pack.validate_domain("runtime.unknown/v1", &invalid),
        Err(ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain)
    );
}

#[test]
fn ordered_target_ir_schema_requires_explicit_execution_order() {
    let pack = assemble(canonical_target_profile_contract_resources());
    let mut artifact = representative_target_ir("echo.dpo@1");
    artifact.domain = "echo.span-ir/v2".to_owned();
    artifact
        .intents
        .get_mut("apply")
        .expect("intent")
        .execution_order = Some(vec![
        "apply.require.0".to_owned(),
        "apply.require.1".to_owned(),
    ]);
    let value = encode_target_ir_value(&artifact);
    pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &value)
        .expect("v2 schema agrees with encoder");
    let mut missing = value.clone();
    let intents = map_value_mut(&mut missing, "intents");
    remove_map_field(map_value_mut(intents, "apply"), "executionOrder");
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &missing),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );
    let mut downgrade = value;
    *map_value_mut(&mut downgrade, "domain") = CanonicalValue::Text("echo.span-ir/v1".to_owned());
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &downgrade),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );

    let mut missing_closure = encode_target_ir_value(&artifact);
    remove_map_field(&mut missing_closure, "semanticClosure");
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &missing_closure),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );
}

#[test]
fn target_ir_root_accepts_only_closed_nonempty_pure_bindings() {
    let pack = assemble(canonical_target_profile_contract_resources());
    let encoded_pure = encoded_target_ir_with_pure_binding();
    pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &encoded_pure)
        .expect("encoder output with a pure binding satisfies the closed root");

    let mut pure_without_closure = encoded_pure.clone();
    remove_map_field(&mut pure_without_closure, "semanticClosure");
    let CanonicalValue::Map(intents) = map_value_mut(&mut pure_without_closure, "intents") else {
        panic!("Target IR intents must be a map");
    };
    remove_map_field(
        &mut intents.first_mut().expect("pure Target IR intent").1,
        "basis",
    );
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &pure_without_closure),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "pure bindings cannot validate through the legacy closure-free root"
    );

    let mut empty_pure_binding_id = encoded_pure;
    let CanonicalValue::Map(intents) = map_value_mut(&mut empty_pure_binding_id, "intents") else {
        panic!("Target IR intents must be a map");
    };
    let intent = &mut intents.first_mut().expect("pure Target IR intent").1;
    let CanonicalValue::Array(bindings) = map_value_mut(intent, "pureBindings") else {
        panic!("Target IR pure bindings must be an array");
    };
    *map_value_mut(bindings.first_mut().expect("one pure binding"), "id") = text("");
    assert_eq!(
        pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &empty_pure_binding_id),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "published Target IR schema must reject an empty pure binding id"
    );
}

#[test]
fn result_projection_root_matches_reference_encoder() {
    let pack = assemble(canonical_target_profile_contract_resources());
    let projection =
        decode_canonical_cbor(RESULT_PROJECTION_FIXTURE).expect("projection fixture is canonical");
    pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &projection)
        .expect("projection fixture satisfies the compiler-owned root");

    let pure_projection = representative_pure_result_projection();
    pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &pure_projection)
        .expect("pure-binding projection satisfies the compiler-owned root");
    let mut empty_binding_id = pure_projection;
    let source = map_value_mut(map_value_mut(&mut empty_binding_id, "expression"), "source");
    *map_value_mut(source, "bindingId") = text("");
    assert_eq!(
        pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &empty_binding_id),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "published projection schema must reject an empty pure binding id"
    );

    let mut missing_output_bound = projection;
    remove_map_field(&mut missing_output_bound, "maxOutputBytes");
    assert_eq!(
        pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &missing_output_bound),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );
}

#[test]
fn result_projection_root_enforces_decoder_path_limit() {
    let pack = assemble(canonical_target_profile_contract_resources());
    let mut projection = representative_result_projection();
    *projection_path_mut(&mut projection) = array(
        (0..edict_syntax::MAX_RESULT_PROJECTION_PATH_SEGMENTS)
            .map(|index| text(&format!("segment{index:02}")))
            .collect(),
    );

    pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &projection)
        .expect("the decoder's exact path-segment limit satisfies the published root");

    let CanonicalValue::Array(path) = projection_path_mut(&mut projection) else {
        panic!("projection path must be an array");
    };
    path.push(text("segment32"));
    assert_eq!(
        pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &projection),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "the published root must reject one more segment than the decoder permits"
    );
}

#[test]
fn result_projection_root_enforces_output_and_record_node_limits() {
    let pack = assemble(canonical_target_profile_contract_resources());

    let mut zero_output_bound = representative_result_projection();
    *map_value_mut(&mut zero_output_bound, "maxOutputBytes") = CanonicalValue::Integer(0);
    assert_eq!(
        pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &zero_output_bound),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "the published root must reject the decoder's zero output bound"
    );

    let mut boundary = representative_result_projection();
    let expression = map_value_mut(&mut boundary, "expression");
    let fields = map_value_mut(expression, "fields");
    *fields = CanonicalValue::Map(
        (0..(edict_syntax::MAX_RESULT_PROJECTION_NODES - 1))
            .map(|index| {
                (
                    text(&format!("field{index:03}")),
                    map(vec![
                        ("kind", text("source")),
                        ("source", map(vec![("kind", text("applicationInput"))])),
                        ("path", array(Vec::new())),
                    ]),
                )
            })
            .collect(),
    );
    pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &boundary)
        .expect("the root record plus 255 source nodes satisfies the decoder limit");

    let fields = map_value_mut(map_value_mut(&mut boundary, "expression"), "fields");
    let CanonicalValue::Map(entries) = fields else {
        panic!("projection record fields must be a map");
    };
    entries.push((
        text("field255"),
        map(vec![
            ("kind", text("source")),
            ("source", map(vec![("kind", text("applicationInput"))])),
            ("path", array(Vec::new())),
        ]),
    ));
    assert_eq!(
        pack.validate_domain(RESULT_PROJECTION_DIGEST_DOMAIN, &boundary),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch),
        "the published root must reject one more field than the flat record node limit permits"
    );
}

#[test]
fn core_root_accepts_reference_encoded_operation_basis() {
    let context = CompilerContext::new()
        .with_operation_profile("sequence.splice", "continuum.profile.write/v1")
        .with_operation_profile_write_classes("sequence.splice", [WriteClass::Replace])
        .with_effect_write_class("sequence.splice", WriteClass::Replace)
        .with_budget(
            "sequence.small",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 16 * 1024,
                max_output_bytes: 4096,
            },
        );
    let module = parse_module(OPERATION_SOURCE).expect("operation source parses");
    let core = compile_to_core(&module, &context).expect("operation source compiles");
    let value = decode_canonical_cbor(&encode_core_module(&core).expect("Core encodes"))
        .expect("Core bytes are canonical");

    assemble(canonical_target_profile_contract_resources())
        .validate_domain(CORE_MODULE_DIGEST_DOMAIN, &value)
        .expect("reference-encoded operation Core satisfies the published root");
}

#[test]
fn target_ir_root_accepts_encoder_valid_line_feed_coordinate() {
    let pack = assemble(canonical_target_profile_contract_resources());
    let line_feed_coordinate = encoded_target_ir_with_coordinate("\n");

    pack.validate_domain(TARGET_IR_ARTIFACT_DIGEST_DOMAIN, &line_feed_coordinate)
        .expect("schema accepts the encoder-valid LF-only coordinate");
}

#[test]
fn contract_pack_rejects_missing_duplicate_and_tampered_members() {
    assert_assembly_failure(
        |resources| {
            resources.remove(0);
        },
        &[ProviderContractPackFailureKind::ContractResourceMissing],
    );
    assert_assembly_failure(
        |resources| resources.push(resources[0].clone()),
        &[ProviderContractPackFailureKind::ContractResourceDuplicate],
    );
    assert_assembly_failure(
        |resources| {
            let mut unknown = resources[0].clone();
            unknown.coordinate = "runtime.unknown-contract/v1".to_owned();
            resources.push(unknown);
        },
        &[ProviderContractPackFailureKind::ContractResourceUnknown],
    );
    assert_assembly_failure(
        |resources| resources[0].canonical_bytes = vec![0xff],
        &[ProviderContractPackFailureKind::ContractResourceInvalidCanonicalArtifact],
    );
    assert_assembly_failure(
        |resources| {
            resources[0].canonical_bytes = resources[1].canonical_bytes.clone();
            resources[0].digest = digest_target_profile_contract_resource(
                &resources[0].coordinate,
                &resources[0].canonical_bytes,
            )
            .expect("replacement bytes are canonical");
        },
        &[ProviderContractPackFailureKind::ContractResourceBytesMismatch],
    );
    assert_assembly_failure(
        |resources| resources[0].digest = format!("sha256:{}", "0".repeat(64)),
        &[ProviderContractPackFailureKind::ContractResourceDomainFramedDigestMismatch],
    );
    assert_assembly_failure(
        |resources| resources[0].provenance.source_path.push_str(".stale"),
        &[ProviderContractPackFailureKind::ContractResourceProvenanceMismatch],
    );
}

#[test]
fn manifest_validation_recomputes_schema_and_resource_identity() {
    let pack = assemble(canonical_target_profile_contract_resources());
    validate_provider_contract_pack_manifest(&pack, pack.manifest())
        .expect("assembled manifest validates exactly");

    let mut raw_resource_mismatch = pack.manifest().clone();
    raw_resource_mismatch.resources[0].raw_sha256[0] ^= 0xff;
    assert_manifest_failure(
        &pack,
        &raw_resource_mismatch,
        &[ProviderContractPackFailureKind::ContractResourceRawDigestMismatch],
    );

    let mut framed_resource_mismatch = pack.manifest().clone();
    framed_resource_mismatch.resources[0].domain_framed_digest =
        format!("sha256:{}", "0".repeat(64));
    assert_manifest_failure(
        &pack,
        &framed_resource_mismatch,
        &[ProviderContractPackFailureKind::ContractResourceDomainFramedDigestMismatch],
    );

    let mut resource_order_mismatch = pack.manifest().clone();
    resource_order_mismatch.resources.reverse();
    assert_manifest_failure(
        &pack,
        &resource_order_mismatch,
        &[ProviderContractPackFailureKind::ManifestResourceOrderMismatch],
    );

    let mut schema_digest_mismatch = pack.manifest().clone();
    schema_digest_mismatch.schema_sha256[0] ^= 0xff;
    assert_manifest_failure(
        &pack,
        &schema_digest_mismatch,
        &[ProviderContractPackFailureKind::ManifestSchemaDigestMismatch],
    );

    let mut metadata_mismatches = pack.manifest().clone();
    metadata_mismatches.api_version.push_str(".stale");
    metadata_mismatches.coordinate.push_str(".stale");
    metadata_mismatches.license = "MIT".to_owned();
    metadata_mismatches.contracts.reverse();
    metadata_mismatches.domains.reverse();
    assert_manifest_failure(
        &pack,
        &metadata_mismatches,
        &[
            ProviderContractPackFailureKind::ManifestApiVersionMismatch,
            ProviderContractPackFailureKind::ManifestCoordinateMismatch,
            ProviderContractPackFailureKind::ManifestLicenseMismatch,
            ProviderContractPackFailureKind::ManifestContractBindingsMismatch,
            ProviderContractPackFailureKind::ManifestDomainBindingsMismatch,
        ],
    );

    let mut schema_bytes_mismatch = pack.manifest().clone();
    schema_bytes_mismatch
        .schema_bytes
        .extend_from_slice(b"; stale\n");
    schema_bytes_mismatch.schema_sha256 =
        Sha256::digest(&schema_bytes_mismatch.schema_bytes).into();
    assert_manifest_failure(
        &pack,
        &schema_bytes_mismatch,
        &[
            ProviderContractPackFailureKind::ManifestSchemaBytesMismatch,
            ProviderContractPackFailureKind::ManifestSchemaDigestMismatch,
        ],
    );
}

#[test]
fn assembly_rejects_missing_invalid_uncompilable_and_incomplete_schema_fragments() {
    let missing = assemble_provider_contract_pack(input_with(
        b"",
        TARGET_IR_CDDL,
        canonical_target_profile_contract_resources(),
    ))
    .expect_err("empty Core fragment rejects");
    assert_eq!(
        failure_kinds(&missing),
        vec![ProviderContractPackFailureKind::SchemaFragmentMissing]
    );

    let invalid_utf8 = assemble_provider_contract_pack(ProviderContractPackInput {
        common_cddl: &[0xff],
        ..input(canonical_target_profile_contract_resources())
    })
    .expect_err("non-UTF-8 fragment rejects");
    assert_eq!(
        failure_kinds(&invalid_utf8),
        vec![ProviderContractPackFailureKind::SchemaFragmentInvalidUtf8]
    );

    let uncompilable = assemble_provider_contract_pack(input_with(
        CORE_CDDL,
        b"target-ir-artifact = {",
        canonical_target_profile_contract_resources(),
    ))
    .expect_err("invalid CDDL rejects");
    assert_eq!(
        failure_kinds(&uncompilable),
        vec![ProviderContractPackFailureKind::SchemaCompileFailed]
    );

    let unresolved = assemble_provider_contract_pack(input_with(
        CORE_CDDL,
        b"target-ir-artifact = externally-owned-rule\n",
        canonical_target_profile_contract_resources(),
    ))
    .expect_err("unresolved external CDDL rule rejects");
    assert_eq!(
        failure_kinds(&unresolved),
        vec![ProviderContractPackFailureKind::SchemaExternalRuleUnresolved]
    );
    assert_eq!(unresolved[0].subject(), "externally-owned-rule");

    let missing_root = assemble_provider_contract_pack(input_with(
        CORE_CDDL,
        b"unrelated-target-ir-rule = null\n",
        canonical_target_profile_contract_resources(),
    ))
    .expect_err("pack missing Target IR root rejects");
    assert_eq!(
        failure_kinds(&missing_root),
        vec![ProviderContractPackFailureKind::SchemaRootMissing]
    );
    assert_eq!(missing_root[0].subject(), TARGET_IR_ARTIFACT_CDDL_ROOT);
}

#[test]
fn assembly_rejects_uninspectable_schema_controls_explicitly() {
    let failures = assemble_provider_contract_pack(input_with(
        CORE_CDDL,
        b"target-ir-artifact = bstr .cbor externally-owned-rule\n",
        canonical_target_profile_contract_resources(),
    ))
    .expect_err("an uninspectable nested rule graph rejects");

    assert_eq!(
        failure_kinds(&failures),
        vec![ProviderContractPackFailureKind::SchemaControlUnsupported]
    );
    assert_eq!(failures[0].subject(), ".cbor");
}

fn assemble(resources: Vec<TargetProfileContractResource>) -> ProviderContractPack {
    assemble_provider_contract_pack(input(resources)).expect("authoritative pack assembles")
}

fn input(resources: Vec<TargetProfileContractResource>) -> ProviderContractPackInput<'static> {
    ProviderContractPackInput {
        source_closure_cddl: Some(include_bytes!(
            "../../../docs/abi/edict-source-closure.cddl"
        )),
        ..input_with(CORE_CDDL, TARGET_IR_CDDL, resources)
    }
}

fn input_with(
    core_cddl: &'static [u8],
    target_ir_cddl: &'static [u8],
    resources: Vec<TargetProfileContractResource>,
) -> ProviderContractPackInput<'static> {
    ProviderContractPackInput {
        common_cddl: COMMON_CDDL,
        core_cddl,
        lawpack_cddl: LAWPACK_CDDL,
        lawpack_adapter_cddl: LAWPACK_ADAPTER_CDDL,
        target_profile_cddl: TARGET_PROFILE_CDDL,
        authority_facts_cddl: AUTHORITY_FACTS_CDDL,
        result_projection_cddl: RESULT_PROJECTION_CDDL,
        target_ir_cddl,
        source_closure_cddl: None,
        contract_resources: resources,
    }
}

fn assert_assembly_failure(
    mutate: impl FnOnce(&mut Vec<TargetProfileContractResource>),
    expected: &[ProviderContractPackFailureKind],
) {
    let mut resources = canonical_target_profile_contract_resources();
    mutate(&mut resources);
    let failures = assemble_provider_contract_pack(input(resources))
        .expect_err("mutated resource closure rejects");
    assert_eq!(failure_kinds(&failures), expected);
}

fn assert_manifest_failure(
    pack: &ProviderContractPack,
    manifest: &edict_provider_schema::ProviderContractPackManifest,
    expected: &[ProviderContractPackFailureKind],
) {
    let failures = validate_provider_contract_pack_manifest(pack, manifest)
        .expect_err("mutated manifest rejects");
    assert_eq!(failure_kinds(&failures), expected);
}

fn failure_kinds(
    failures: &[edict_provider_schema::ProviderContractPackFailure],
) -> Vec<ProviderContractPackFailureKind> {
    failures
        .iter()
        .map(edict_provider_schema::ProviderContractPackFailure::kind)
        .collect()
}

fn representative_contract_instances() -> Vec<(&'static str, CanonicalValue)> {
    vec![
        (
            "authority-facts",
            decode_canonical_cbor(AUTHORITY_FACTS_FIXTURE).expect("authority fixture is canonical"),
        ),
        (
            "core-module",
            decode_canonical_cbor(CORE_FIXTURE).expect("Core fixture is canonical"),
        ),
        (
            "lawpack-adapter",
            decode_canonical_cbor(LAWPACK_ADAPTER_FIXTURE)
                .expect("lawpack adapter fixture is canonical"),
        ),
        ("lawpack-exports", lawpack_exports()),
        ("lawpack-manifest", lawpack_manifest()),
        ("lowering-requirements", lowering_requirements()),
        ("result-projection", representative_result_projection()),
        ("source-closure", representative_source_closure()),
        (
            "target-ir-artifact",
            decode_canonical_cbor(TARGET_IR_FIXTURE).expect("Target IR fixture is canonical"),
        ),
        ("target-profile-intrinsics", intrinsics_document()),
        ("target-profile-manifest", target_profile_manifest()),
        (
            "target-profile-operation-profiles",
            operation_profiles_document(),
        ),
    ]
}

fn representative_result_projection() -> CanonicalValue {
    let projection = ResultProjection {
        api_version: edict_syntax::RESULT_PROJECTION_API_VERSION.to_owned(),
        operation_coordinate: "example.application@1.create".to_owned(),
        output_type: "example.application@1.Created".to_owned(),
        max_output_bytes: 512,
        expression: ResultProjectionExpr::Record {
            fields: BTreeMap::from([(
                "value".to_owned(),
                ResultProjectionExpr::Source {
                    source: ResultProjectionSource::ApplicationInput,
                    path: vec!["value".to_owned()],
                },
            )]),
        },
    };
    let bytes = encode_result_projection(&projection).expect("representative projection encodes");
    decode_canonical_cbor(&bytes).expect("representative projection is canonical")
}

fn representative_pure_result_projection() -> CanonicalValue {
    let projection = ResultProjection {
        api_version: edict_syntax::RESULT_PROJECTION_API_VERSION.to_owned(),
        operation_coordinate: "example.application@1.compute".to_owned(),
        output_type: "example.application@1.Computed".to_owned(),
        max_output_bytes: 512,
        expression: ResultProjectionExpr::Source {
            source: ResultProjectionSource::PureBinding {
                binding_id: "compute.binding.0".to_owned(),
            },
            path: Vec::new(),
        },
    };
    let bytes = encode_result_projection(&projection).expect("pure projection encodes");
    decode_canonical_cbor(&bytes).expect("pure projection is canonical")
}

fn encoded_target_ir_with_requirements() -> CanonicalValue {
    encoded_target_ir_with_coordinate("example.target-profile@1")
}

fn encoded_target_ir_with_coordinate(coordinate: &str) -> CanonicalValue {
    encode_target_ir_value(&representative_target_ir(coordinate))
}

fn encoded_target_ir_with_external_request() -> CanonicalValue {
    let mut artifact = representative_target_ir("example.target-profile@1");
    let operation = ResourceRef {
        coordinate: "workspace.snapshot.observe@1".to_owned(),
        digest: Some(format!("sha256:{}", "4".repeat(64))),
    };
    artifact
        .semantic_closure
        .as_mut()
        .expect("representative Target IR is closed")
        .capabilities
        .push(operation.clone());
    artifact
        .intents
        .get_mut("apply")
        .expect("representative intent")
        .external_action_requests
        .push(TargetIrExternalActionRequest {
            id: "apply.request.0".to_owned(),
            binding: LocalRef {
                id: "local.0".to_owned(),
                alpha_name: "$local0".to_owned(),
                ty: "edict.external-action.request/v1<Bytes<max=64>>".to_owned(),
            },
            operation,
            input_type: "Bytes<max=32>".to_owned(),
            settlement_type: "Bytes<max=64>".to_owned(),
            input_schema: ResourceRef {
                coordinate: "workspace.snapshot.input@1".to_owned(),
                digest: Some(format!("sha256:{}", "5".repeat(64))),
            },
            settlement_schema: ResourceRef {
                coordinate: "workspace.snapshot.settlement@1".to_owned(),
                digest: Some(format!("sha256:{}", "6".repeat(64))),
            },
            input: CoreExpr::Const(CoreValue::Bytes(vec![1, 2, 3])),
            authority_scope: CoreExpr::Const(CoreValue::Bytes(vec![4, 5])),
            basis: CoreExpr::Const(CoreValue::Bytes(vec![6, 7])),
            budget: CoreExternalActionBudget {
                max_settlement_bytes: CoreExpr::Const(CoreValue::Int {
                    width: "U64".to_owned(),
                    value: "64".to_owned(),
                }),
                max_attempts: CoreExpr::Const(CoreValue::Int {
                    width: "U32".to_owned(),
                    value: "3".to_owned(),
                }),
            },
            reconciliation_law: ResourceRef {
                coordinate: "workspace.snapshot.reconcile@1".to_owned(),
                digest: Some(format!("sha256:{}", "7".repeat(64))),
            },
        });
    encode_target_ir_value(&artifact)
}

fn encoded_target_ir_with_pure_binding() -> CanonicalValue {
    let mut artifact = representative_target_ir("example.target-profile@1");
    let binding = LocalRef {
        id: "local.0".to_owned(),
        alpha_name: "$local0".to_owned(),
        ty: "U64".to_owned(),
    };
    let intent = artifact
        .intents
        .get_mut("apply")
        .expect("representative intent");
    intent.pure_bindings.push(TargetIrPureBinding {
        id: "apply.binding.0".to_owned(),
        binding: binding.clone(),
        value: CoreExpr::Const(CoreValue::Int {
            width: "U64".to_owned(),
            value: "1".to_owned(),
        }),
    });
    intent.result = CoreExpr::Local { reference: binding };
    encode_target_ir_value(&artifact)
}

fn representative_target_ir(coordinate: &str) -> TargetIrArtifact {
    let terminal_reason = CoreObstructionReason {
        kind: "example.Terminal".to_owned(),
        payload: BTreeMap::from([(
            "provided".to_owned(),
            CoreExpr::Const(CoreValue::String("terminal".to_owned())),
        )]),
    };
    let preserved_reason = CoreObstructionReason {
        kind: "example.Preserved".to_owned(),
        payload: BTreeMap::from([(
            "provided".to_owned(),
            CoreExpr::Const(CoreValue::String("preserved".to_owned())),
        )]),
    };
    TargetIrArtifact {
        domain: "example.target-ir/v1".to_owned(),
        target_profile: ResourceRef {
            coordinate: coordinate.to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        source_core_coordinate: "example.core@1".to_owned(),
        semantic_closure: Some(TargetIrSemanticClosure {
            source_core: ResourceRef {
                coordinate: "example.core@1".to_owned(),
                digest: Some(format!("sha256:{}", "2".repeat(64))),
            },
            lawpacks: vec![ResourceRef {
                coordinate: "example.lawpack@1".to_owned(),
                digest: Some(format!("sha256:{}", "3".repeat(64))),
            }],
            capabilities: Vec::new(),
        }),
        intents: BTreeMap::from([(
            "apply".to_owned(),
            TargetIrIntent {
                execution_order: None,
                operation_profile: "example.operation/v1".to_owned(),
                basis: Some(CoreExpr::Const(CoreValue::String(
                    "example.basis@1".to_owned(),
                ))),
                input_constraints: Vec::new(),
                core_evaluation_budget: CoreBudget {
                    max_steps: 1,
                    max_allocated_bytes: 1,
                    max_output_bytes: 1,
                },
                pure_bindings: Vec::new(),
                requirements: vec![
                    TargetIrRequirement {
                        id: "apply.require.0".to_owned(),
                        predicate: CorePredicate::True,
                        on_failure: TargetIrRequireFailure::Terminal {
                            reason: terminal_reason,
                        },
                    },
                    TargetIrRequirement {
                        id: "apply.require.1".to_owned(),
                        predicate: CorePredicate::False,
                        on_failure: TargetIrRequireFailure::ContinueObstructed {
                            reason: preserved_reason,
                        },
                    },
                ],
                steps: Vec::new(),
                external_action_requests: Vec::new(),
                result: CoreExpr::Const(CoreValue::Null),
            },
        )]),
    }
}

fn encode_target_ir_value(artifact: &TargetIrArtifact) -> CanonicalValue {
    let bytes = encode_target_ir_artifact(artifact).expect("representative Target IR encodes");
    decode_canonical_cbor(&bytes).expect("encoded representative Target IR is canonical")
}

fn lawpack_manifest() -> CanonicalValue {
    map(vec![
        ("apiVersion", text("edict.lawpack/v1")),
        ("id", text("example.lawpack")),
        ("version", text("1")),
        ("acceptedCoreAbi", array(vec![text("edict.core/v1")])),
        ("dependencies", array(vec![])),
        ("exports", resource_ref("example.lawpack.exports@1")),
        (
            "verifier",
            map(vec![
                ("class", text("declarative")),
                ("ruleset", resource_ref("example.lawpack.rules@1")),
            ]),
        ),
        ("compatibility", resource_ref("example.compatibility@1")),
        (
            "conformanceFixtureCorpus",
            resource_ref("example.fixtures@1"),
        ),
    ])
}

fn lawpack_exports() -> CanonicalValue {
    map(vec![
        ("types", array(vec![])),
        ("constants", array(vec![])),
        ("pureFunctions", array(vec![])),
        ("effects", array(vec![])),
        ("obstructions", array(vec![])),
        ("operationProfiles", map(vec![])),
    ])
}

fn target_profile_manifest() -> CanonicalValue {
    map(vec![
        ("apiVersion", text("edict.target-profile/v1")),
        ("id", text("example.target")),
        ("version", text("1")),
        ("acceptedCoreAbi", array(vec![text("edict.core/v1")])),
        ("intrinsics", resource_ref("example.intrinsics@1")),
        ("intrinsicNamespace", text("example.target@1")),
        (
            "operationProfiles",
            resource_ref("example.operation-profiles@1"),
        ),
        ("footprintAlgebra", resource_ref("example.footprints@1")),
        ("costAlgebra", resource_ref("example.costs@1")),
        ("targetIr", resource_ref("example.target-ir@1")),
        (
            "obstructionTaxonomy",
            resource_ref("example.obstructions@1"),
        ),
        ("verifier", resource_ref("example.verifier@1")),
        ("lowerer", resource_ref("example.lowerer@1")),
        ("sandbox", resource_ref("edict.wasm-component/v1")),
        ("fuelModel", resource_ref("edict.fuel/v1")),
        ("bundleProfile", resource_ref("example.bundle@1")),
        ("generatedArtifactProfiles", array(vec![])),
        (
            "canonicalEncodingRules",
            resource_ref("edict.canonical-cbor/v1"),
        ),
        ("diagnosticAbi", resource_ref("edict.diagnostics/v1")),
        ("applicationModel", text("atomic")),
        ("readConsistency", text("application-snapshot")),
        ("guardEvaluation", text("precommit-atomic")),
        ("obstructionRollback", text("no-visible-effects")),
        ("multiTarget", CanonicalValue::Bool(false)),
        ("postconditionSupport", CanonicalValue::Bool(true)),
        (
            "deterministicExecution",
            resource_ref("edict.determinism/v1"),
        ),
        (
            "conformanceFixtureCorpus",
            resource_ref("example.fixtures@1"),
        ),
    ])
}

fn intrinsics_document() -> CanonicalValue {
    map(vec![
        ("apiVersion", text("edict.target-profile.intrinsics/v1")),
        ("intrinsics", map(vec![])),
    ])
}

fn operation_profiles_document() -> CanonicalValue {
    map(vec![
        (
            "apiVersion",
            text("edict.target-profile.operation-profiles/v1"),
        ),
        ("profiles", map(vec![])),
    ])
}

fn lowering_requirements() -> CanonicalValue {
    map(vec![
        ("apiVersion", text("edict.lowering-requirements/v1")),
        ("operationProfile", text("example.operation/v1")),
        ("semanticEffects", array(vec![])),
        ("requiredWriteClasses", array(vec![])),
        ("guardKinds", array(vec![])),
        ("atomicity", text("atomic")),
        ("postconditionSupport", CanonicalValue::Bool(true)),
        ("obstructionCoordinates", array(vec![])),
        ("footprintObligations", array(vec![])),
        ("costObligations", array(vec![])),
        ("opticContract", text("example.optic/v1")),
    ])
}

fn resource_ref(id: &str) -> CanonicalValue {
    map(vec![
        ("id", text(id)),
        (
            "digest",
            array(vec![text("sha256"), CanonicalValue::Bytes(vec![0x11; 32])]),
        ),
    ])
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

fn array(values: Vec<CanonicalValue>) -> CanonicalValue {
    CanonicalValue::Array(values)
}

fn map(entries: Vec<(&str, CanonicalValue)>) -> CanonicalValue {
    CanonicalValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn map_value_mut<'a>(value: &'a mut CanonicalValue, field: &str) -> &'a mut CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("{field} parent must be a map");
    };
    entries
        .iter_mut()
        .find_map(|(key, value)| (key == &text(field)).then_some(value))
        .unwrap_or_else(|| panic!("missing map field {field}"))
}

fn projection_path_mut(projection: &mut CanonicalValue) -> &mut CanonicalValue {
    let expression = map_value_mut(projection, "expression");
    let fields = map_value_mut(expression, "fields");
    let CanonicalValue::Map(fields) = fields else {
        panic!("projection record fields must be a map");
    };
    let source = &mut fields
        .first_mut()
        .expect("representative projection has one field")
        .1;
    map_value_mut(source, "path")
}

fn remove_map_field(value: &mut CanonicalValue, field: &str) {
    let CanonicalValue::Map(entries) = value else {
        panic!("{field} parent must be a map");
    };
    let index = entries
        .iter()
        .position(|(key, _value)| key == &text(field))
        .unwrap_or_else(|| panic!("missing map field {field}"));
    entries.remove(index);
}

fn assert_sorted_unique<'a>(values: impl IntoIterator<Item = &'a str>) {
    let values = values.into_iter().collect::<Vec<_>>();
    assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn source_function_contract_preserves_old_modules_and_requires_new_schema_selection() {
    let context = CompilerContext::new()
        .with_operation_profile("text.replaceRange", "continuum.profile.read-only/v1")
        .with_budget(
            "text.replaceRangeBudget",
            CoreBudget {
                max_steps: 4096,
                max_allocated_bytes: 65536,
                max_output_bytes: 4096,
            },
        );
    let values = [
        include_str!("../../../fixtures/lang/functions/range-assembly-baseline.edict"),
        include_str!("../../../fixtures/lang/functions/range-assembly.edict"),
    ]
    .map(|source| {
        let core = compile_to_core(&parse_module(source).unwrap(), &context).unwrap();
        decode_canonical_cbor(&encode_core_module(&core).unwrap()).unwrap()
    });
    let old = assemble_provider_contract_pack(input_with(
        include_bytes!("../../../fixtures/lang/functions/legacy-core.cddl"),
        TARGET_IR_CDDL,
        canonical_target_profile_contract_resources(),
    ))
    .unwrap();
    let new = assemble(canonical_target_profile_contract_resources());
    assert_ne!(old.raw_sha256(), new.raw_sha256());
    assert_eq!(
        old.cddl_bytes(),
        include_bytes!("../../../fixtures/provider-contracts/v1/edict-provider-contracts.cddl")
    );
    assert_eq!(
        old.manifest_bytes(),
        include_bytes!("../../../fixtures/provider-contracts/v1/manifest.json")
    );
    old.validate_domain(CORE_MODULE_DIGEST_DOMAIN, &values[0])
        .unwrap();
    assert_eq!(
        old.validate_domain(CORE_MODULE_DIGEST_DOMAIN, &values[1]),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );
    for value in &values {
        new.validate_domain(CORE_MODULE_DIGEST_DOMAIN, value)
            .unwrap();
    }
    let mut missing_result = values[1].clone();
    let function = map_value_mut(
        map_value_mut(&mut missing_result, "functions"),
        "assembleFragments",
    );
    remove_map_field(function, "returnType");
    assert_eq!(
        new.validate_domain(CORE_MODULE_DIGEST_DOMAIN, &missing_result),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );
    let mut empty_table = values[1].clone();
    *map_value_mut(&mut empty_table, "functions") = CanonicalValue::Map(vec![]);
    assert_eq!(
        new.validate_domain(CORE_MODULE_DIGEST_DOMAIN, &empty_table),
        Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
    );
}

#[test]
fn source_closure_contract_has_its_own_domain_and_closed_typed_wire() {
    use edict_syntax::source_closure::SOURCE_CLOSURE_DIGEST_DOMAIN;
    let pack = assemble(canonical_target_profile_contract_resources());
    let value = representative_source_closure();
    pack.validate_domain(SOURCE_CLOSURE_DIGEST_DOMAIN, &value)
        .expect("source closure publication has a distinct typed domain");
    let old = assemble_provider_contract_pack(input_with(
        CORE_CDDL,
        TARGET_IR_CDDL,
        canonical_target_profile_contract_resources(),
    ))
    .unwrap();
    assert_eq!(old.cddl_bytes(), include_bytes!("../../../fixtures/provider-contracts/source-functions-v1/edict-provider-contracts.cddl"));
    assert_eq!(
        old.manifest_bytes(),
        include_bytes!("../../../fixtures/provider-contracts/source-functions-v1/manifest.json")
    );
    assert_eq!(
        old.validate_domain(SOURCE_CLOSURE_DIGEST_DOMAIN, &value),
        Err(ProviderArtifactSchemaValidationErrorKind::UnsupportedDomain)
    );
    let mut extra = value.clone();
    let CanonicalValue::Map(fields) = &mut extra else {
        panic!("closure envelope")
    };
    fields.push((CanonicalValue::Text("extra".into()), CanonicalValue::Null));
    let mut wrong_digest = value.clone();
    let module = map_value_mut(
        map_value_mut(&mut wrong_digest, "modules"),
        "example.modules@1",
    );
    *map_value_mut(module, "digest") = CanonicalValue::Text(format!("sha256:{}", "0".repeat(64)));
    for invalid in [
        CanonicalValue::Bytes(b"package example.modules@1;".to_vec()),
        extra,
        wrong_digest,
    ] {
        assert_eq!(
            pack.validate_domain(SOURCE_CLOSURE_DIGEST_DOMAIN, &invalid),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
        );
    }
}

fn representative_source_closure() -> CanonicalValue {
    use edict_syntax::source_closure::{resolve_source_closure, SourceClosureLimits};
    let closure = resolve_source_closure(
        "package example.modules@1;",
        &[],
        SourceClosureLimits::default(),
    )
    .unwrap();
    decode_canonical_cbor(closure.canonical_bytes()).unwrap()
}

#[test]
fn source_closure_domain_enforces_typed_digest_and_module_count_boundaries() {
    use edict_syntax::source_closure::{
        resolve_source_closure, source_module_digest, SourceClosureLimits,
        SOURCE_CLOSURE_DIGEST_DOMAIN,
    };
    use std::fmt::Write as _;
    let pack = assemble(canonical_target_profile_contract_resources());
    let value = representative_source_closure();
    for (algorithm, length) in [("sha256", 31), ("sha256", 33), ("sha512", 32)] {
        let mut invalid = value.clone();
        *map_value_mut(
            map_value_mut(map_value_mut(&mut invalid, "modules"), "example.modules@1"),
            "digest",
        ) = CanonicalValue::Array(vec![
            text(algorithm),
            CanonicalValue::Bytes(vec![0; length]),
        ]);
        assert_eq!(
            pack.validate_domain(SOURCE_CLOSURE_DIGEST_DOMAIN, &invalid),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
        );
    }
    let dependencies = (0..63)
        .map(|index| format!("package example.module{index}@1;"))
        .collect::<Vec<_>>();
    let mut root = String::from("package example.modules@1;");
    for (index, source) in dependencies.iter().enumerate() {
        write!(
            root,
            " use module example.module{index}@1 digest \"{}\" as m{index};",
            source_module_digest(source).unwrap()
        )
        .unwrap();
    }
    let sources = dependencies.iter().map(String::as_str).collect::<Vec<_>>();
    let closure = resolve_source_closure(&root, &sources, SourceClosureLimits::default()).unwrap();
    let maximum = decode_canonical_cbor(closure.canonical_bytes()).unwrap();
    pack.validate_domain(SOURCE_CLOSURE_DIGEST_DOMAIN, &maximum)
        .expect("exactly 64 authenticated modules satisfy the published wire");
    let mut empty = value;
    *map_value_mut(&mut empty, "modules") = CanonicalValue::Map(Vec::new());
    let mut overflow = maximum;
    let CanonicalValue::Map(modules) = map_value_mut(&mut overflow, "modules") else {
        panic!("module map")
    };
    assert_eq!(modules.len(), 64);
    modules.push((text("example.overflow@1"), modules[0].1.clone()));
    for invalid in [empty, overflow] {
        assert_eq!(
            pack.validate_domain(SOURCE_CLOSURE_DIGEST_DOMAIN, &invalid),
            Err(ProviderArtifactSchemaValidationErrorKind::SchemaMismatch)
        );
    }
}
