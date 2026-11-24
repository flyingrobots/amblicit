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
/// Core schema as published before typed failures.
const PRE_TYPED_FAILURE_CORE_CDDL: &[u8] =
    include_bytes!("../../../fixtures/core/pre-typed-failure-core.cddl");
/// Target IR schema as published before typed failures.
const PRE_TYPED_FAILURE_TARGET_IR_CDDL: &[u8] =
    include_bytes!("../../../fixtures/target-ir/pre-typed-failure-target-ir.cddl");

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

/// Core schema as published before function `preconditions`.
const PRE_PRECONDITION_CORE_CDDL: &[u8] =
    include_bytes!("../../../fixtures/core/pre-precondition-core.cddl");
/// Current Core schema.
const CORE_CDDL: &[u8] = include_bytes!("../../../docs/abi/edict-core.cddl");

/// A contract pack assembled from the authoritative ABI sources, except for
/// the Core and target-profile schemas, which the caller selects.
fn pack_with(
    core_cddl: &'static [u8],
    target_profile_cddl: &'static [u8],
    target_ir_cddl: &'static [u8],
) -> ProviderContractPack {
    assemble_provider_contract_pack(ProviderContractPackInput {
        common_cddl: include_bytes!("../../../docs/abi/edict-common.cddl"),
        core_cddl,
        lawpack_cddl: include_bytes!("../../../docs/abi/edict-lawpack.cddl"),
        lawpack_adapter_cddl: include_bytes!("../../../docs/abi/edict-lawpack-adapter.cddl"),
        target_profile_cddl,
        authority_facts_cddl: include_bytes!("../../../docs/abi/edict-authority-facts.cddl"),
        result_projection_cddl: include_bytes!("../../../docs/abi/edict-result-projection.cddl"),
        target_ir_cddl,
        source_closure_cddl: Some(include_bytes!(
            "../../../docs/abi/edict-source-closure.cddl"
        )),
        contract_resources: canonical_target_profile_contract_resources(),
    })
    .expect("pack assembles")
}

/// The two latest publications that admit only v1 catalogs, each reproduced
/// from its frozen inputs and checked against its published bytes.
fn frozen_v1_catalog_packs() -> [(&'static str, ProviderContractPack); 2] {
    let packs = [
        (
            "bounded-state-fold-v1",
            pack_with(
                PRE_PRECONDITION_CORE_CDDL,
                PRE_CATALOG_V2_TARGET_PROFILE_CDDL,
                PRE_TYPED_FAILURE_TARGET_IR_CDDL,
            ),
            include_bytes!(
                "../../../fixtures/provider-contracts/bounded-state-fold-v1/edict-provider-contracts.cddl"
            )
            .as_slice(),
            include_bytes!("../../../fixtures/provider-contracts/bounded-state-fold-v1/manifest.json")
                .as_slice(),
        ),
        (
            "function-preconditions-v1",
            pack_with(
                PRE_TYPED_FAILURE_CORE_CDDL,
                PRE_CATALOG_V2_TARGET_PROFILE_CDDL,
                PRE_TYPED_FAILURE_TARGET_IR_CDDL,
            ),
            include_bytes!(
                "../../../fixtures/provider-contracts/function-preconditions-v1/edict-provider-contracts.cddl"
            )
            .as_slice(),
            include_bytes!(
                "../../../fixtures/provider-contracts/function-preconditions-v1/manifest.json"
            )
            .as_slice(),
        ),
    ];
    packs.map(|(name, pack, cddl, manifest)| {
        assert_eq!(
            pack.cddl_bytes(),
            cddl,
            "frozen inputs no longer reproduce the published {name} schema"
        );
        assert_eq!(
            pack.manifest_bytes(),
            manifest,
            "frozen inputs no longer reproduce the published {name} manifest"
        );
        (name, pack)
    })
}

/// The pure-catalog publication remains an exact frozen pair after the
/// cumulative typed-failure publication becomes current.
#[test]
fn pure_intrinsic_catalog_publication_is_reproduced_from_frozen_failure_inputs() {
    let pack = assemble_provider_contract_pack(ProviderContractPackInput {
        common_cddl: include_bytes!("../../../docs/abi/edict-common.cddl"),
        core_cddl: PRE_TYPED_FAILURE_CORE_CDDL,
        lawpack_cddl: include_bytes!("../../../docs/abi/edict-lawpack.cddl"),
        lawpack_adapter_cddl: include_bytes!("../../../docs/abi/edict-lawpack-adapter.cddl"),
        target_profile_cddl: PRE_EFFECT_COST_TARGET_PROFILE_CDDL,
        authority_facts_cddl: include_bytes!("../../../docs/abi/edict-authority-facts.cddl"),
        result_projection_cddl: include_bytes!("../../../docs/abi/edict-result-projection.cddl"),
        target_ir_cddl: PRE_TYPED_FAILURE_TARGET_IR_CDDL,
        source_closure_cddl: Some(include_bytes!(
            "../../../docs/abi/edict-source-closure.cddl"
        )),
        contract_resources: canonical_target_profile_contract_resources(),
    })
    .expect("frozen pure-catalog publication assembles");

    assert_eq!(
        pack.cddl_bytes(),
        include_bytes!(
            "../../../fixtures/provider-contracts/pure-intrinsic-catalog-v1/edict-provider-contracts.cddl"
        ),
        "pure-catalog CDDL bytes remain frozen"
    );
    assert_eq!(
        pack.manifest_bytes(),
        include_bytes!(
            "../../../fixtures/provider-contracts/pure-intrinsic-catalog-v1/manifest.json"
        ),
        "pure-catalog manifest bytes remain frozen"
    );
}

/// Each frozen v1-catalog pack refuses a v2 catalog that compiler admission
/// accepts.
#[test]
fn frozen_packs_refuse_the_catalog_the_compiler_admits() {
    let catalog = catalog_v2();
    let facts = compiler_admission(&catalog).expect("compiler admits the v2 catalog");
    assert_eq!(facts.len(), 1, "one required pure intrinsic is admitted");

    for (name, pack) in frozen_v1_catalog_packs() {
        assert_eq!(
            pack.validate_contract(CATALOG_CONTRACT, &catalog),
            Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
            "{name} must not admit a v2 catalog"
        );
    }
}

/// Each frozen v1-catalog pack still admits a v1 catalog, which compiler
/// admission refuses.
#[test]
fn frozen_packs_admit_the_v1_catalog_the_compiler_refuses() {
    let catalog = catalog_v1();
    assert_eq!(
        compiler_admission(&catalog),
        Err(vec![TargetPureIntrinsicFailureKind::MalformedCatalog]),
        "the compiler admits pure intrinsics only from a v2 catalog"
    );

    for (name, pack) in frozen_v1_catalog_packs() {
        assert_eq!(
            pack.validate_contract(CATALOG_CONTRACT, &catalog),
            Ok(()),
            "{name} must still admit a v1 catalog"
        );
    }
}

// --- The catalog v2 publication ---------------------------------------------

/// The current publication, assembled from the authoritative ABI sources.
fn catalog_v2_pack() -> ProviderContractPack {
    pack_with(
        CORE_CDDL,
        include_bytes!("../../../docs/abi/edict-target-profile.cddl"),
        include_bytes!("../../../docs/abi/edict-target-ir.cddl"),
    )
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
        "a v1 catalog belongs to the frozen v1-catalog packs"
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

// --- The catalog v3 (effect cost template) publication (#341) ---------------

/// Target-profile schema as published before catalog v3.
const PRE_EFFECT_COST_TARGET_PROFILE_CDDL: &[u8] =
    include_bytes!("../../../fixtures/target-profile/pre-effect-cost-target-profile.cddl");

/// Echo's `/v3` catalog, vendored byte for byte from Echo #1029.
const ECHO_V3_CATALOG: &[u8] =
    include_bytes!("../../../fixtures/echo-target-intrinsics-v3/resource.target-intrinsics-3.cbor");

fn echo_v3_catalog() -> V {
    decode_canonical_cbor(ECHO_V3_CATALOG).expect("vendored Echo catalog is canonical CBOR")
}

/// The typed-failures publication, reproduced from its frozen inputs.
fn typed_failures_pack() -> ProviderContractPack {
    let pack = pack_with(
        CORE_CDDL,
        PRE_EFFECT_COST_TARGET_PROFILE_CDDL,
        include_bytes!("../../../docs/abi/edict-target-ir.cddl"),
    );
    assert_eq!(
        pack.cddl_bytes(),
        include_bytes!(
            "../../../fixtures/provider-contracts/typed-failures-v1/edict-provider-contracts.cddl"
        ),
        "frozen inputs no longer reproduce the published typed-failures-v1 schema"
    );
    assert_eq!(
        pack.manifest_bytes(),
        include_bytes!("../../../fixtures/provider-contracts/typed-failures-v1/manifest.json"),
        "frozen inputs no longer reproduce the published typed-failures-v1 manifest"
    );
    pack
}

/// A small v3 catalog: the v2 catalog with a cost template on its effect.
fn catalog_v3() -> V {
    let mut catalog = catalog_v2();
    *field_mut(&mut catalog, "apiVersion") = text("edict.target-profile.intrinsics/v3");
    let V::Map(effect) = field_mut(field_mut(&mut catalog, "intrinsics"), "echo.dpo@1.replace")
    else {
        panic!("effect entry is a map")
    };
    effect.push((
        text("effectCostRule"),
        map(vec![
            ("steps", map(vec![("const", V::Integer(1))])),
            ("bytes", map(vec![("const", V::Integer(344))])),
        ]),
    ));
    catalog
}

fn effect_rule_mut(catalog: &mut V) -> &mut V {
    field_mut(
        field_mut(field_mut(catalog, "intrinsics"), "echo.dpo@1.replace"),
        "effectCostRule",
    )
}

/// Oracle: amblicit#341 "Final effect cost-template format"; Echo's catalog
/// passes the compiler's admission (`effect_cost_templates.rs`).
#[test]
fn current_pack_admits_echo_v3_catalog() {
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &echo_v3_catalog()),
        Ok(())
    );
}

/// The frozen typed-failures publication keeps refusing a v3 catalog.
#[test]
fn typed_failures_publication_refuses_a_v3_catalog() {
    assert_eq!(
        typed_failures_pack().validate_contract(CATALOG_CONTRACT, &echo_v3_catalog()),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    );
}

/// A v2 catalog is admitted unchanged by the current pack.
#[test]
fn current_pack_still_admits_a_v2_catalog() {
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog_v2()),
        Ok(())
    );
}

#[test]
fn current_pack_admits_a_minimal_v3_catalog() {
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog_v3()),
        Ok(())
    );
}

/// Catalog v3 requires a template on every effect intrinsic.
#[test]
fn current_pack_refuses_a_v3_effect_without_a_template() {
    let mut catalog = catalog_v3();
    remove_field(
        field_mut(field_mut(&mut catalog, "intrinsics"), "echo.dpo@1.replace"),
        "effectCostRule",
    );
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    );
}

/// Malformed templates: a one-term sum, an unknown measure, a missing half,
/// and a negative constant.
#[test]
fn current_pack_refuses_malformed_v3_templates() {
    let malformed = [
        map(vec![
            (
                "steps",
                map(vec![(
                    "sum",
                    V::Array(vec![map(vec![("const", V::Integer(1))])]),
                )]),
            ),
            ("bytes", map(vec![("const", V::Integer(0))])),
        ]),
        map(vec![
            (
                "steps",
                map(vec![("measure", map(vec![("unknown", text("steps"))]))]),
            ),
            ("bytes", map(vec![("const", V::Integer(0))])),
        ]),
        map(vec![("steps", map(vec![("const", V::Integer(1))]))]),
        map(vec![
            ("steps", map(vec![("const", V::Integer(-1))])),
            ("bytes", map(vec![("const", V::Integer(0))])),
        ]),
    ];
    let pack = catalog_v2_pack();
    for (index, rule) in malformed.into_iter().enumerate() {
        let mut catalog = catalog_v3();
        *effect_rule_mut(&mut catalog) = rule;
        assert_eq!(
            pack.validate_contract(CATALOG_CONTRACT, &catalog),
            Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch),
            "malformed template {index}"
        );
    }
}

/// A malformed `parameters` declaration is refused.
#[test]
fn current_pack_refuses_a_malformed_v3_parameter() {
    let mut catalog = catalog_v3();
    let V::Map(fields) = &mut catalog else {
        panic!("catalog is a map")
    };
    fields.push((
        text("parameters"),
        map(vec![(
            "maxEdgeScans",
            map(vec![("path", texts(&[])), ("kind", text("uint"))]),
        )]),
    ));
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    );
}

/// `/v3`-only keys do not make a `/v2` catalog valid.
#[test]
fn current_pack_refuses_a_v2_catalog_with_a_template() {
    let mut catalog = catalog_v3();
    *field_mut(&mut catalog, "apiVersion") = text("edict.target-profile.intrinsics/v2");
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    );
}

#[test]
fn current_pack_refuses_a_v2_catalog_with_parameters() {
    let mut catalog = catalog_v2();
    let V::Map(fields) = &mut catalog else {
        panic!("catalog is a map")
    };
    fields.push((text("parameters"), V::Map(Vec::new())));
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    );
}

/// A v3 catalog declaring `count` well-formed parameters.
fn catalog_v3_with_parameters(count: usize) -> V {
    let mut catalog = catalog_v3();
    let V::Map(fields) = &mut catalog else {
        panic!("catalog is a map")
    };
    let declarations = (0..count)
        .map(|index| {
            (
                text(&format!("p{index}")),
                map(vec![
                    ("path", texts(&["graphReads"])),
                    ("kind", text("uint")),
                ]),
            )
        })
        .collect();
    fields.push((text("parameters"), V::Map(declarations)));
    catalog
}

/// Oracle: the decoder's limit, `MAX_DECLARATIONS` = 64 in
/// `crates/amblicit-syntax/src/target_ir/effect_parameters.rs`.
#[test]
fn current_pack_admits_v3_parameters_at_the_decoder_limit() {
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog_v3_with_parameters(64)),
        Ok(())
    );
}

#[test]
fn current_pack_refuses_v3_parameters_above_the_decoder_limit() {
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog_v3_with_parameters(65)),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    );
}

/// Oracle: the decoder refuses `then` after no item step (`effect_costs.rs`).
#[test]
fn current_pack_refuses_then_after_no_item_step() {
    let mut catalog = catalog_v3();
    *effect_rule_mut(&mut catalog) = map(vec![
        (
            "steps",
            map(vec![(
                "measure",
                map(vec![(
                    "bytesMax",
                    map(vec![(
                        "path",
                        map(vec![
                            ("side", text("input")),
                            ("fields", texts(&["rows"])),
                            ("item", V::Integer(0)),
                            ("then", texts(&["a"])),
                        ]),
                    )]),
                )]),
            )]),
        ),
        ("bytes", map(vec![("const", V::Integer(0))])),
    ]);
    assert_eq!(
        catalog_v2_pack().validate_contract(CATALOG_CONTRACT, &catalog),
        Err(ProviderContractInstanceValidationErrorKind::SchemaMismatch)
    );
}
