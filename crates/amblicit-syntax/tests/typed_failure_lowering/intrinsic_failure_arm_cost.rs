//! Mutually exclusive terminal obstruction payloads pay for one selected arm.
use super::facts;
use amblicit_syntax::{
    admit_target_pure_intrinsics, compile_to_core, encode_canonical_cbor, lower_to_target_ir,
    parse_module, CanonicalValue as V, CompilerContext, CoreBudget, RequiredPureIntrinsic,
    ResourceRef, TargetLoweringFailureKind, TargetLoweringStatus, TargetPureIntrinsicFact,
};

const INTRINSIC: &str = "echo.dpo@1.contentAddress";
const INTRINSIC_VERSION: &str = "echo.content-address/v1";
const CATALOG: &str = "echo.dpo@1.intrinsics";
const BYTE_BOUND: u64 = 100_000;
const ONE_CALL_COST: u64 = 100_071;

fn text(value: &str) -> V {
    V::Text(value.to_owned())
}

fn map(entries: Vec<(&str, V)>) -> V {
    V::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn admitted() -> Vec<TargetPureIntrinsicFact> {
    let catalog = map(vec![
        ("apiVersion", text("edict.target-profile.intrinsics/v2")),
        (
            "intrinsics",
            map(vec![(
                INTRINSIC,
                map(vec![
                    ("intrinsicClass", text("pure")),
                    ("typeParameters", V::Array(Vec::new())),
                    (
                        "argumentTypes",
                        V::Array(
                            [
                                "Bytes<exact=32>",
                                "Bytes<exact=32>",
                                "Bytes<exact=32>",
                                "Bytes<max=1048576>",
                                "List<Record<edgeType:Bytes<exact=32>,to:Bytes<exact=32>>,max=16>",
                            ]
                            .into_iter()
                            .map(text)
                            .collect(),
                        ),
                    ),
                    ("returnType", text("Bytes<exact=32>")),
                    ("guardSupport", V::Bool(false)),
                    (
                        "footprintTemplate",
                        text("echo.dpo@1.contentAddress.footprint"),
                    ),
                    ("costTemplate", text("echo.dpo@1.contentAddress.cost")),
                    ("writeClass", text("none")),
                    ("contractVersion", text(INTRINSIC_VERSION)),
                    (
                        "costRule",
                        map(vec![
                            ("base", V::Integer(71)),
                            (
                                "perArgumentMax",
                                V::Array([0, 0, 0, 1, 67].map(V::Integer).to_vec()),
                            ),
                        ]),
                    ),
                ]),
            )]),
        ),
    ]);
    let catalog_bytes = encode_canonical_cbor(&catalog).expect("catalog encodes");
    let target = facts("echo.span-ir/v2").target_profile;
    let catalog_ref = ResourceRef {
        coordinate: CATALOG.to_owned(),
        digest: Some(
            amblicit_syntax::digest_target_intrinsics_catalog(CATALOG, &catalog_bytes)
                .expect("catalog identity"),
        ),
    };
    admit_target_pure_intrinsics(
        &target,
        &catalog_ref,
        &catalog_bytes,
        &[RequiredPureIntrinsic {
            coordinate: INTRINSIC.to_owned(),
            contract_version: INTRINSIC_VERSION.to_owned(),
        }],
    )
    .expect("catalog admits the size-charged intrinsic")
}

fn source(left_fields: &str, right_fields: &str) -> String {
    format!(
        r#"package rope.intrinsic.arms@1;
use target echo.dpo@1 digest "sha256:1111111111111111111111111111111111111111111111111111111111111111" as echo;
failure Left;
failure Right;
type Input = {{ warp: Bytes<exact=32>, atom: Bytes<max={BYTE_BOUND}>, choose: Bool }};
type Output = {{}};
fn stop(choose: Bool) -> U64 fails Left, Right {{
  return if choose then fail Left else fail Right;
}}
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{
  let stopped = stop(input.choose) else {{
    Left => text.Left({{ {left_fields} }}),
    Right => text.Right({{ {right_fields} }}),
  }};
  return {{}};
}}"#
    )
}

fn address_field(name: &str) -> String {
    format!("{name}: echo.contentAddress(input.warp, input.warp, input.warp, input.atom, [])")
}

fn compile(
    left_fields: &str,
    right_fields: &str,
    source_budget: u64,
) -> amblicit_syntax::CoreModule {
    let intrinsics = admitted();
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: source_budget,
                max_allocated_bytes: 100_000_000,
                max_output_bytes: 1_000_000,
            },
        );
    let [intrinsic] = intrinsics.as_slice() else {
        panic!("exactly one admitted intrinsic")
    };
    let context = context.with_target_pure_intrinsic(intrinsic.clone());
    compile_with_context(left_fields, right_fields, &context)
}

fn compile_with_context(
    left_fields: &str,
    right_fields: &str,
    context: &CompilerContext,
) -> amblicit_syntax::CoreModule {
    let module = parse_module(&source(left_fields, right_fields)).expect("source parses");
    compile_to_core(&module, context)
        .unwrap_or_else(|errors| panic!("intrinsic-arm source compiles: {errors:#?}"))
}

fn lower_at_budget(
    core: &amblicit_syntax::CoreModule,
    intrinsics: &[TargetPureIntrinsicFact],
    budget: u64,
    expected: TargetLoweringStatus,
) {
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let mut candidate = core.clone();
        candidate
            .intents
            .get_mut("evaluate")
            .expect("intent")
            .core_evaluation_budget
            .max_steps = budget;
        let report = lower_to_target_ir(
            &candidate,
            &facts(domain).with_target_pure_intrinsics(intrinsics),
        );
        assert_eq!(
            report.status, expected,
            "{domain}, {budget} steps: {report:#?}"
        );
        if expected == TargetLoweringStatus::Lowered {
            assert!(
                report.artifact.is_some(),
                "{domain}: expected Target IR artifact"
            );
        } else {
            assert!(
                report.artifact.is_none(),
                "{domain}: refusal emits no artifact"
            );
            assert_eq!(report.failures.len(), 1, "{domain}: {report:#?}");
            assert_eq!(
                report.failures[0].kind,
                TargetLoweringFailureKind::InvalidCoreIdentity
            );
            assert_eq!(
                report.failures[0].detail,
                "target pure-intrinsic cost is unestablished or exceeds the step budget"
            );
        }
    }
}

#[test]
fn mutually_exclusive_intrinsic_payloads_charge_only_the_maximum_arm() {
    let field = address_field("address");
    let core = compile(&field, &field, 110_000);
    let intrinsics = admitted();
    // Catalog oracle: 71 base + 100,000 bytes * coefficient 1 + empty-list
    // charge 0 = 100,071 per arm. The selected arm is charged once.
    lower_at_budget(&core, &intrinsics, 110_000, TargetLoweringStatus::Lowered);
    lower_at_budget(
        &core,
        &intrinsics,
        ONE_CALL_COST,
        TargetLoweringStatus::Lowered,
    );
    lower_at_budget(
        &core,
        &intrinsics,
        ONE_CALL_COST - 1,
        TargetLoweringStatus::Unsupported,
    );
}

#[test]
fn every_intrinsic_field_in_the_selected_arm_contributes_to_its_maximum_in_either_arm_order() {
    const TWO_CALL_COST: u64 = 200_142;
    let first = address_field("first");
    let second = address_field("second");
    let intrinsics = admitted();
    // Two fields in the larger arm each cost 100,071, so the selected maximum
    // arm costs 200,142. The mutually exclusive one-field arm is not added.
    for (left, right) in [
        (format!("{first}, {second}"), first.clone()),
        (first.clone(), format!("{first}, {second}")),
        (format!("{second}, {first}"), first),
    ] {
        let core = compile(&left, &right, 400_000);
        lower_at_budget(
            &core,
            &intrinsics,
            TWO_CALL_COST,
            TargetLoweringStatus::Lowered,
        );
        lower_at_budget(
            &core,
            &intrinsics,
            TWO_CALL_COST - 1,
            TargetLoweringStatus::Unsupported,
        );
    }
}
