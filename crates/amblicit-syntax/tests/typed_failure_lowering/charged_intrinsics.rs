//! Size-charged target intrinsics preserve explicit terminal failure paths.
use super::facts;
use amblicit_syntax::{
    admit_target_pure_intrinsics, compile_to_core, digest_target_intrinsics_catalog,
    encode_canonical_cbor, lower_to_target_ir, parse_module, CanonicalValue as V, CompilerContext,
    CompilerErrorKind, CoreBudget, RequiredPureIntrinsic, ResourceRef, TargetLoweringStatus,
    TargetPureIntrinsicFact,
};

const ID: &str = "Bytes<exact=32>";
const CATALOG: &str = "echo.dpo@1.intrinsics";
const INTRINSIC: &str = "echo.dpo@1.contentAddress";
const VERSION: &str = "echo.content-address/v1";

fn map(entries: Vec<(&str, V)>) -> V {
    V::Map(
        entries
            .into_iter()
            .map(|(key, value)| (text(key), value))
            .collect(),
    )
}

fn text(value: &str) -> V {
    V::Text(value.into())
}

fn admitted() -> Vec<TargetPureIntrinsicFact> {
    let catalog =
        map(vec![
            ("apiVersion", text("edict.target-profile.intrinsics/v2")),
            (
                "intrinsics",
                map(vec![(
                    INTRINSIC,
                    map(vec![
            ("intrinsicClass", text("pure")),
            ("typeParameters", V::Array(Vec::new())),
            ("argumentTypes", V::Array([
                ID, ID, ID, "Bytes<max=1048576>",
                "List<Record<edgeType:Bytes<exact=32>,to:Bytes<exact=32>>,max=16>",
            ].into_iter().map(text).collect())),
            ("returnType", text(ID)),
            ("guardSupport", V::Bool(false)),
            ("footprintTemplate", text("echo.dpo@1.contentAddress.footprint")),
            ("costTemplate", text("echo.dpo@1.contentAddress.cost")),
            ("writeClass", text("none")),
            ("contractVersion", text(VERSION)),
            ("costRule", map(vec![
                ("base", V::Integer(71)),
                ("perArgumentMax", V::Array([0, 0, 0, 1, 67].map(V::Integer).to_vec())),
            ])),
        ]),
                )]),
            ),
        ]);
    let bytes = encode_canonical_cbor(&catalog).expect("catalog encodes");
    let target = facts("echo.span-ir/v2").target_profile;
    let catalog = ResourceRef {
        coordinate: CATALOG.into(),
        digest: Some(digest_target_intrinsics_catalog(CATALOG, &bytes).expect("catalog identity")),
    };
    admit_target_pure_intrinsics(
        &target,
        &catalog,
        &bytes,
        &[RequiredPureIntrinsic {
            coordinate: INTRINSIC.into(),
            contract_version: VERSION.into(),
        }],
    )
    .expect("real intrinsic contract admits")
}

fn context(intrinsics: &[TargetPureIntrinsicFact]) -> CompilerContext {
    let mut context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 100_000_000,
                max_allocated_bytes: 100_000_000,
                max_output_bytes: 1_000_000,
            },
        );
    for intrinsic in intrinsics {
        context = context.with_target_pure_intrinsic(intrinsic.clone());
    }
    context
}

fn source(atom: &str, failures: &str, mapping: &str, bound: u64) -> String {
    format!(
        r#"package rope.charged@1;
use target echo.dpo@1 digest "sha256:1111111111111111111111111111111111111111111111111111111111111111" as echo;
failure Stop;
enum Choice {{ Live, End }}
type Input = {{ warp: Bytes<exact=32>, atom: Bytes<max={bound}>, choice: Choice }};
type Output = {{ node: Bytes<exact=32> }};
fn make(warp: Bytes<exact=32>, atom: Bytes<max={bound}>, choice: Choice) -> Bytes<exact=32> {failures} {{
  return echo.contentAddress(warp, warp, warp, {atom}, []);
}}
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{
  let xs: List<U64, max=1> = [0u64];
  let length = listLen(xs);
  let node = make(input.warp, input.atom, input.choice) {mapping};
  return {{ node }};
}}"#
    )
}

#[test]
fn terminal_failure_arms_preserve_size_charged_intrinsic_bounds() {
    let intrinsics = admitted();
    for (case, atom, failures, mapping) in [
        ("returning control", "atom", "", ""),
        (
            "conditional terminal arm",
            "if true then atom else fail Stop",
            "fails Stop",
            "else text.ClosureInsufficient",
        ),
        (
            "match terminal arm",
            "match choice { Live => atom, End => fail Stop }",
            "fails Stop",
            "else text.ClosureInsufficient",
        ),
    ] {
        let module = parse_module(&source(atom, failures, mapping, 8)).expect("source parses");
        let compiled = compile_to_core(&module, &context(&intrinsics))
            .unwrap_or_else(|errors| panic!("{case}: source compiles: {errors:#?}"));
        assert!(
            compiled
                .required_core_capabilities
                .iter()
                .any(|capability| capability == "core.list/v1"),
            "the witness must exercise both independent intrinsic cost layers"
        );
        for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
            let report = lower_to_target_ir(
                &compiled,
                &facts(domain).with_target_pure_intrinsics(&intrinsics),
            );
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "{domain}: {case}: {report:#?}"
            );
            assert!(report.artifact.is_some(), "{domain}: {case}");
        }
    }
}

#[test]
fn returning_intrinsic_operand_must_still_fit_the_catalog_ceiling() {
    let intrinsics = admitted();
    let module = parse_module(&source("atom", "", "", 2_097_152)).expect("source parses");
    let errors = compile_to_core(&module, &context(&intrinsics))
        .expect_err("returning bytes exceed the catalog ceiling");
    assert!(
        errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::TypeMismatch),
        "{errors:#?}"
    );
}
