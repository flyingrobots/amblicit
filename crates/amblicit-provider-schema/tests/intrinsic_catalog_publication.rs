//! Intrinsic catalog publication boundary: which published contract pack
//! admits which `edict.target-profile.intrinsics` catalog version.
//!
//! Oracles: the catalog a pack must admit is the one the compiler's own
//! admission API (`admit_target_pure_intrinsics`) accepts, and published pack
//! bytes are the checked-in fixtures.

use amblicit_provider_schema::{
    assemble_provider_contract_pack, ProviderContractInstanceValidationErrorKind,
    ProviderContractPack, ProviderContractPackInput,
};
use amblicit_syntax::{
    admit_target_pure_intrinsics, canonical_target_profile_contract_resources, compile_to_core,
    decode_canonical_cbor, digest_target_intrinsics_catalog, encode_canonical_cbor,
    encode_core_module, parse_module, CanonicalValue as V, CompilerContext, CoreBudget, CoreExpr,
    CoreNode, RequiredPureIntrinsic, ResourceRef, TargetPureIntrinsicFact,
    TargetPureIntrinsicFailureKind, CORE_MODULE_DIGEST_DOMAIN,
};

const CATALOG_CONTRACT: &str = "target-profile-intrinsics";
const CATALOG: &str = "echo.dpo@1.intrinsics";
const CONTENT_ADDRESS: &str = "echo.dpo@1.contentAddress";
const VERSION: &str = "echo.content-address/v1";
const ID: &str = "Bytes<exact=32>";
const EDGE: &str = "Record<edgeType:Bytes<exact=32>,to:Bytes<exact=32>>";

/// Target-profile schema as published before the catalog v2 pack.
const PRE_CATALOG_V2_TARGET_PROFILE_CDDL: &[u8] =
    include_bytes!("../../../fixtures/target-profile/pre-catalog-v2-target-profile.cddl");

fn text(value: &str) -> V {
    V::Text(value.into())
}

fn map(entries: Vec<(&str, V)>) -> V {
    V::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn texts(values: &[&str]) -> V {
    V::Array(values.iter().map(|value| text(value)).collect())
}

/// The fields a pure contract carries in both catalog versions.
fn pure_fields() -> Vec<(&'static str, V)> {
    vec![
        ("intrinsicClass", text("pure")),
        ("typeParameters", V::Array(Vec::new())),
        (
            "argumentTypes",
            texts(&[
                ID,
                ID,
                ID,
                "Bytes<max=1048576>",
                "List<echo.dpo@1.OutgoingEdge,max=16>",
            ]),
        ),
        ("returnType", text(ID)),
        ("guardSupport", V::Bool(false)),
        (
            "footprintTemplate",
            text("echo.dpo@1.contentAddress.footprint"),
        ),
        ("costTemplate", text("echo.dpo@1.contentAddress.cost")),
        ("writeClass", text("none")),
    ]
}

fn effect_entry() -> V {
    map(vec![
        ("intrinsicClass", text("effect")),
        ("typeParameters", V::Array(Vec::new())),
        ("argumentTypes", texts(&["echo.dpo@1.replace.input"])),
        ("returnType", text("echo.dpo@1.replace.result")),
        ("effectKind", text("replace")),
        ("effectFailures", V::Map(Vec::new())),
        ("guardSupport", V::Bool(true)),
        ("footprintTemplate", text("echo.dpo@1.replace.footprint")),
        ("costTemplate", text("echo.dpo@1.replace.cost")),
        ("writeClass", text("replace")),
        ("canParticipateInAtomicGuard", V::Bool(true)),
    ])
}

/// A v1 catalog: one pure and one effect intrinsic, no cost rule.
fn catalog_v1() -> V {
    map(vec![
        ("apiVersion", text("edict.target-profile.intrinsics/v1")),
        (
            "intrinsics",
            map(vec![
                (CONTENT_ADDRESS, map(pure_fields())),
                ("echo.dpo@1.replace", effect_entry()),
            ]),
        ),
    ])
}

/// A v2 catalog: the same intrinsics, the pure one with a complete contract.
fn catalog_v2() -> V {
    let mut pure = pure_fields();
    pure.push(("contractVersion", text(VERSION)));
    pure.push((
        "costRule",
        map(vec![
            ("base", V::Integer(71)),
            (
                "perArgumentMax",
                V::Array([0, 0, 0, 1, 67].map(V::Integer).to_vec()),
            ),
        ]),
    ));
    map(vec![
        ("apiVersion", text("edict.target-profile.intrinsics/v2")),
        ("types", map(vec![("echo.dpo@1.OutgoingEdge", text(EDGE))])),
        (
            "intrinsics",
            map(vec![
                (CONTENT_ADDRESS, map(pure)),
                ("echo.dpo@1.replace", effect_entry()),
            ]),
        ),
    ])
}

fn target() -> ResourceRef {
    ResourceRef {
        coordinate: "echo.dpo@1".into(),
        digest: Some(format!("sha256:{}", "1".repeat(64))),
    }
}

/// What the compiler's admission API decides for one catalog value.
fn compiler_admission(
    catalog: &V,
) -> Result<Vec<TargetPureIntrinsicFact>, Vec<TargetPureIntrinsicFailureKind>> {
    let bytes = encode_canonical_cbor(catalog).expect("canonical catalog");
    let pinned = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("catalog digest")),
    };
    let required = [RequiredPureIntrinsic {
        coordinate: CONTENT_ADDRESS.into(),
        contract_version: VERSION.into(),
    }];
    admit_target_pure_intrinsics(&target(), &pinned, &bytes, &required)
        .map_err(|failures| failures.into_iter().map(|failure| failure.kind).collect())
}

fn pack_with(target_profile_cddl: &'static [u8]) -> ProviderContractPack {
    assemble_provider_contract_pack(ProviderContractPackInput {
        common_cddl: include_bytes!("../../../docs/abi/edict-common.cddl"),
        core_cddl: include_bytes!("../../../docs/abi/edict-core.cddl"),
        lawpack_cddl: include_bytes!("../../../docs/abi/edict-lawpack.cddl"),
        lawpack_adapter_cddl: include_bytes!("../../../docs/abi/edict-lawpack-adapter.cddl"),
        target_profile_cddl,
        authority_facts_cddl: include_bytes!("../../../docs/abi/edict-authority-facts.cddl"),
        result_projection_cddl: include_bytes!("../../../docs/abi/edict-result-projection.cddl"),
        target_ir_cddl: include_bytes!("../../../docs/abi/edict-target-ir.cddl"),
        source_closure_cddl: Some(include_bytes!(
            "../../../docs/abi/edict-source-closure.cddl"
        )),
        contract_resources: canonical_target_profile_contract_resources(),
    })
    .expect("pack assembles")
}

/// The `bounded-state-fold-v1` publication, reproduced from its frozen input.
fn bounded_state_fold_pack() -> ProviderContractPack {
    let pack = pack_with(PRE_CATALOG_V2_TARGET_PROFILE_CDDL);
    assert_eq!(
        pack.cddl_bytes(),
        include_bytes!(
            "../../../fixtures/provider-contracts/bounded-state-fold-v1/edict-provider-contracts.cddl"
        ),
        "frozen inputs no longer reproduce the published bounded-state-fold-v1 schema"
    );
    assert_eq!(
        pack.manifest_bytes(),
        include_bytes!("../../../fixtures/provider-contracts/bounded-state-fold-v1/manifest.json"),
        "frozen inputs no longer reproduce the published bounded-state-fold-v1 manifest"
    );
    pack
}

#[test]
fn bounded_state_fold_pack_refuses_the_catalog_the_compiler_admits() {
    let pack = bounded_state_fold_pack();
    let catalog = catalog_v2();
    let facts = compiler_admission(&catalog).expect("compiler admits the v2 catalog");
    assert_eq!(facts.len(), 1, "one required pure intrinsic is admitted");

    assert_eq!(
        pack.validate_contract(CATALOG_CONTRACT, &catalog),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
        "bounded-state-fold-v1 must not admit a v2 catalog"
    );
}

#[test]
fn bounded_state_fold_pack_admits_the_v1_catalog_the_compiler_refuses() {
    let pack = bounded_state_fold_pack();
    let catalog = catalog_v1();

    pack.validate_contract(CATALOG_CONTRACT, &catalog)
        .expect("bounded-state-fold-v1 admits a v1 catalog");
    assert_eq!(
        compiler_admission(&catalog),
        Err(vec![TargetPureIntrinsicFailureKind::MalformedCatalog]),
        "the compiler admits pure intrinsics only from a v2 catalog"
    );
}

// --- The catalog v2 publication ---------------------------------------------

/// The current publication, assembled from the authoritative ABI sources.
fn catalog_v2_pack() -> ProviderContractPack {
    pack_with(include_bytes!(
        "../../../docs/abi/edict-target-profile.cddl"
    ))
}

fn field_mut<'a>(value: &'a mut V, key: &str) -> &'a mut V {
    let V::Map(entries) = value else {
        panic!("{key}: not a map")
    };
    &mut entries
        .iter_mut()
        .find(|(name, _)| name == &text(key))
        .unwrap_or_else(|| panic!("missing field {key}"))
        .1
}

fn remove_field(value: &mut V, key: &str) {
    let V::Map(entries) = value else {
        panic!("{key}: not a map")
    };
    let before = entries.len();
    entries.retain(|(name, _)| name != &text(key));
    assert_eq!(entries.len() + 1, before, "missing field {key}");
}

fn pure_contract_mut(catalog: &mut V) -> &mut V {
    field_mut(field_mut(catalog, "intrinsics"), CONTENT_ADDRESS)
}

#[test]
fn catalog_v2_pack_admits_the_catalog_the_compiler_admits() {
    let pack = catalog_v2_pack();
    let catalog = catalog_v2();
    compiler_admission(&catalog).expect("compiler admits the v2 catalog");

    pack.validate_contract(CATALOG_CONTRACT, &catalog)
        .expect("the current pack admits a v2 catalog");
}

#[test]
fn catalog_v2_pack_refuses_a_v1_catalog() {
    let pack = catalog_v2_pack();

    assert_eq!(
        pack.validate_contract(CATALOG_CONTRACT, &catalog_v1()),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
        "a v1 catalog belongs to bounded-state-fold-v1"
    );
}

#[test]
fn catalog_v2_pack_refuses_contracts_the_compiler_refuses() {
    use TargetPureIntrinsicFailureKind as K;
    type Mutation = fn(&mut V);
    // Each row breaks one requirement the compiler's admission API reads.
    let cases: [(&str, Mutation, &[K]); 8] = [
        (
            "v1 api version with v2 contracts",
            |v| *field_mut(v, "apiVersion") = text("edict.target-profile.intrinsics/v1"),
            &[K::MalformedCatalog],
        ),
        (
            "unknown catalog field",
            |v| {
                let V::Map(entries) = v else { panic!("map") };
                entries.push((text("extra"), V::Bool(true)));
            },
            &[K::MalformedCatalog],
        ),
        (
            "missing contract version",
            |v| remove_field(pure_contract_mut(v), "contractVersion"),
            &[K::MalformedContract],
        ),
        (
            "missing cost rule",
            |v| remove_field(pure_contract_mut(v), "costRule"),
            &[K::MalformedContract],
        ),
        (
            "generic pure contract",
            |v| *field_mut(pure_contract_mut(v), "typeParameters") = texts(&["T"]),
            &[K::MalformedContract],
        ),
        (
            "negative cost base",
            |v| *field_mut(field_mut(pure_contract_mut(v), "costRule"), "base") = V::Integer(-1),
            &[K::InvalidCostRule],
        ),
        (
            "cost rule without coefficients",
            |v| {
                remove_field(
                    field_mut(pure_contract_mut(v), "costRule"),
                    "perArgumentMax",
                );
            },
            &[K::InvalidCostRule],
        ),
        (
            "provider type that is not a type reference",
            |v| *field_mut(field_mut(v, "types"), "echo.dpo@1.OutgoingEdge") = V::Integer(1),
            &[K::UnresolvedType, K::InvalidProviderType],
        ),
    ];
    let pack = catalog_v2_pack();

    for (case, mutate, compiler_refusal) in cases {
        let mut catalog = catalog_v2();
        mutate(&mut catalog);
        assert_eq!(
            compiler_admission(&catalog),
            Err(compiler_refusal.to_vec()),
            "{case}: compiler admission"
        );
        assert_eq!(
            pack.validate_contract(CATALOG_CONTRACT, &catalog),
            Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
            "{case}: published schema"
        );
    }
}

#[test]
fn source_calling_a_catalog_v2_pure_intrinsic_compiles_to_core() {
    let pack = catalog_v2_pack();
    let catalog = catalog_v2();
    pack.validate_contract(CATALOG_CONTRACT, &catalog)
        .expect("the current pack admits the catalog the source is compiled against");
    let facts = compiler_admission(&catalog).expect("compiler admits the v2 catalog");
    let mut context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 65_536,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        );
    for fact in facts {
        context = context.with_target_pure_intrinsic(fact);
    }
    let source = include_str!("../../../fixtures/lang/intrinsics/content-address.amblicit");

    let core = compile_to_core(&parse_module(source).expect("fixture parses"), &context)
        .expect("fixture compiles to Core");

    let CoreNode::Let {
        value: CoreExpr::Call { callee, args, .. },
        ..
    } = &core.intents["ids"].body.nodes[0]
    else {
        panic!("the intent's first node is not an intrinsic call binding")
    };
    assert_eq!(callee, CONTENT_ADDRESS);
    assert_eq!(args.len(), 5);
    let encoded = encode_core_module(&core).expect("Core encodes");
    pack.validate_domain(
        CORE_MODULE_DIGEST_DOMAIN,
        &decode_canonical_cbor(&encoded).expect("Core decodes"),
    )
    .expect("the current pack admits the compiled Core module");
}
