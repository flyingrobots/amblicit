//! Intrinsic catalog publication boundary: which published contract pack
//! admits which `edict.target-profile.intrinsics` catalog version.
//!
//! Oracles: for the pure contracts a consumer requires, the compiler's own
//! admission API (`admit_target_pure_intrinsics`) decides what a pack must
//! admit and refuse wherever CDDL can state the rule; published pack bytes are
//! the checked-in fixtures.

use amblicit_provider_schema::{
    assemble_provider_contract_pack, ProviderContractInstanceValidationErrorKind,
    ProviderContractPack, ProviderContractPackInput,
};
use amblicit_syntax::{
    admit_target_pure_intrinsics, canonical_target_profile_contract_resources, compile_to_core,
    decode_canonical_cbor, digest_target_intrinsics_catalog, encode_canonical_cbor,
    encode_core_module, parse_module, CanonicalValue as V, CompilerContext, CompilerErrorKind,
    CoreBudget, CoreExpr, CoreNode, RequiredPureIntrinsic, ResourceRef, TargetPureIntrinsicFact,
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

/// A CBOR text value.
fn text(value: &str) -> V {
    V::Text(value.into())
}

/// A CBOR map with text keys, in the given order.
fn map(entries: Vec<(&str, V)>) -> V {
    V::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

/// A CBOR array of text values.
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

/// An effect intrinsic, whose shape is the same in both catalog versions.
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

/// The digest-locked target profile the intrinsics belong to.
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

/// A contract pack assembled from the authoritative ABI sources, except for
/// the target-profile schema, which the caller selects.
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

/// The frozen pack refuses a v2 catalog that compiler admission accepts.
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

/// The frozen pack still admits a v1 catalog, which compiler admission refuses.
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

/// The value of one field of a CBOR map, for mutation.
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

/// Remove one field, which must be present, from a CBOR map.
fn remove_field(value: &mut V, key: &str) {
    let V::Map(entries) = value else {
        panic!("{key}: not a map")
    };
    let before = entries.len();
    entries.retain(|(name, _)| name != &text(key));
    assert_eq!(entries.len() + 1, before, "missing field {key}");
}

/// The catalog's `contentAddress` pure contract, for mutation.
fn pure_contract_mut(catalog: &mut V) -> &mut V {
    field_mut(field_mut(catalog, "intrinsics"), CONTENT_ADDRESS)
}

/// The current pack admits the v2 catalog that compiler admission accepts.
#[test]
fn catalog_v2_pack_admits_the_catalog_the_compiler_admits() {
    let pack = catalog_v2_pack();
    let catalog = catalog_v2();
    compiler_admission(&catalog).expect("compiler admits the v2 catalog");

    pack.validate_contract(CATALOG_CONTRACT, &catalog)
        .expect("the current pack admits a v2 catalog");
}

/// The current pack refuses a v1 catalog.
#[test]
fn catalog_v2_pack_refuses_a_v1_catalog() {
    let pack = catalog_v2_pack();

    assert_eq!(
        pack.validate_contract(CATALOG_CONTRACT, &catalog_v1()),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
        "a v1 catalog belongs to bounded-state-fold-v1"
    );
}

/// Any non-empty contract text that admission accepts, including line breaks,
/// whitespace and non-ASCII characters, is also accepted by the current pack.
#[test]
fn catalog_v2_pack_admits_any_nonempty_contract_text() {
    let pack = catalog_v2_pack();
    for field in ["contractVersion", "footprintTemplate", "costTemplate"] {
        for value in ["\n", " ", "a\nb", "\u{e9}"] {
            let mut catalog = catalog_v2();
            *field_mut(pure_contract_mut(&mut catalog), field) = text(value);
            let mut required_version = VERSION;
            if field == "contractVersion" {
                required_version = value;
            }
            let bytes = encode_canonical_cbor(&catalog).expect("canonical catalog");
            let pinned = ResourceRef {
                coordinate: CATALOG.into(),
                digest: Some(
                    digest_target_intrinsics_catalog(CATALOG, &bytes).expect("catalog digest"),
                ),
            };
            let required = [RequiredPureIntrinsic {
                coordinate: CONTENT_ADDRESS.into(),
                contract_version: required_version.into(),
            }];
            admit_target_pure_intrinsics(&target(), &pinned, &bytes, &required)
                .unwrap_or_else(|failures| panic!("{field}={value:?}: admission {failures:?}"));

            assert_eq!(
                pack.validate_contract(CATALOG_CONTRACT, &catalog),
                Ok(()),
                "{field}={value:?}: published schema"
            );
        }
    }
}

/// Thirteen single-field corruptions that admission refuses are refused by the
/// current pack too. Type resolution is not among them: admission owns it.
#[test]
fn catalog_v2_pack_refuses_contracts_the_compiler_refuses() {
    use TargetPureIntrinsicFailureKind as K;
    type Mutation = fn(&mut V);
    // Each row breaks one requirement the compiler's admission API reads.
    let cases: [(&str, Mutation, &[K]); 13] = [
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
            "empty contract version",
            |v| *field_mut(pure_contract_mut(v), "contractVersion") = text(""),
            &[K::MalformedContract],
        ),
        (
            "empty footprint template",
            |v| *field_mut(pure_contract_mut(v), "footprintTemplate") = text(""),
            &[K::MalformedContract],
        ),
        (
            "empty cost template",
            |v| *field_mut(pure_contract_mut(v), "costTemplate") = text(""),
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
            "text cost base",
            |v| *field_mut(field_mut(pure_contract_mut(v), "costRule"), "base") = text("71"),
            &[K::InvalidCostRule],
        ),
        (
            "negative cost coefficient",
            |v| {
                let V::Array(coefficients) = field_mut(
                    field_mut(pure_contract_mut(v), "costRule"),
                    "perArgumentMax",
                ) else {
                    panic!("coefficient array")
                };
                coefficients[3] = V::Integer(-1);
            },
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

/// The current pack refuses a catalog with an incomplete uncalled pure entry.
#[test]
fn catalog_v2_pack_requires_complete_contracts_for_uncalled_pure_intrinsics() {
    // Publication rule: every pure intrinsic in a v2 catalog carries a
    // complete monomorphic contract, whether or not a consumer requires it.
    let pack = catalog_v2_pack();
    let generic = {
        let mut fields = pure_fields();
        fields.retain(|(key, _)| *key != "typeParameters");
        fields.push(("typeParameters", texts(&["T"])));
        fields.push(("contractVersion", text(VERSION)));
        fields.push((
            "costRule",
            map(vec![
                ("base", V::Integer(1)),
                ("perArgumentMax", V::Array(vec![V::Integer(0); 5])),
            ]),
        ));
        map(fields)
    };
    for (case, uncalled) in [
        ("generic pure constructor", generic),
        ("v1-shaped pure intrinsic", map(pure_fields())),
    ] {
        let mut catalog = catalog_v2();
        let V::Map(intrinsics) = field_mut(&mut catalog, "intrinsics") else {
            panic!("intrinsics map")
        };
        intrinsics.push((text("echo.dpo@1.uncalled"), uncalled));

        assert_eq!(
            pack.validate_contract(CATALOG_CONTRACT, &catalog),
            Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
            "{case}: published schema"
        );
    }
}

/// The compiler context the source fixture names, with no target facts.
fn fixture_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 65_536,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65_536,
            },
        )
}

/// Without admitted facts the fixture's intrinsic call does not resolve.
#[test]
fn source_fixture_does_not_compile_without_catalog_facts() {
    let source = include_str!("../../../fixtures/lang/intrinsics/content-address.amblicit");

    let errors = compile_to_core(
        &parse_module(source).expect("fixture parses"),
        &fixture_context(),
    )
    .expect_err("the intrinsic call needs admitted facts")
    .into_iter()
    .map(|error| error.kind)
    .collect::<Vec<_>>();

    assert_eq!(errors, [CompilerErrorKind::UnresolvedFunction]);
}

/// Source calling a pure intrinsic from a published v2 catalog compiles to
/// Core that the current pack accepts.
#[test]
fn source_calling_a_catalog_v2_pure_intrinsic_compiles_to_core() {
    let pack = catalog_v2_pack();
    let catalog = catalog_v2();
    pack.validate_contract(CATALOG_CONTRACT, &catalog)
        .expect("the current pack admits the catalog the source is compiled against");
    let facts = compiler_admission(&catalog).expect("compiler admits the v2 catalog");
    let mut context = fixture_context();
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
