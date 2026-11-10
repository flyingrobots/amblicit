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
    admit_target_pure_intrinsics, canonical_target_profile_contract_resources,
    digest_target_intrinsics_catalog, encode_canonical_cbor, CanonicalValue as V,
    RequiredPureIntrinsic, ResourceRef, TargetPureIntrinsicFact, TargetPureIntrinsicFailureKind,
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
