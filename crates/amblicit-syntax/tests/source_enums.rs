//! Source enum declarations: payload-free cases selected with `Name.CASE`.
//!
//! Oracle: SPEC Edict Language v1 `enum-decl` and `EDICT-LANG-ENUMVARIANT-001`.
//! Core has no enum kind, so the compiler's enum encoding is a named Core
//! `Variant` whose cases carry no payload.
use amblicit_syntax::source_closure::{
    resolve_source_closure, source_module_digest, SourceClosureLimits,
};
use amblicit_syntax::source_linker::link_source_closure;
use amblicit_syntax::{
    compile_to_core, digest_core_module, lower_to_target_ir, parse_module, CompilerContext,
    CompilerError, CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreType, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringStatus, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
};

const SPINE: &str = "enum Spine { Boundary, RightSpine, LeftSpine, Range }";

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.large",
            CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: 1_000_000,
                max_output_bytes: 1_000_000,
            },
        )
}

fn compile(body: &str) -> Result<CoreModule, Vec<CompilerError>> {
    let source = format!("package jedit.spines@1; {body}");
    compile_to_core(&parse_module(&source).expect("valid syntax"), &context())
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

/// A scalar intent gives Target a step to lower beside the helpers under test.
const WITNESS: &str = "type Input = { marker: U8 }; type Output = { marker: U8 }; intent witness(input: Input) returns Output profile p.read basis none budget <= p.large { let marker = input.marker; return { marker }; }";

fn refusal(body: &str, kind: CompilerErrorKind) {
    let errors = compile(body).expect_err(body);
    assert!(
        errors.iter().any(|error| error.kind == kind),
        "{body}: expected {kind:?}, got {errors:?}"
    );
}

#[test]
fn enum_declarations_lower_to_payload_free_core_variants() {
    let core = compile(&format!(
        "{SPINE} fn first() -> Spine {{ return Spine.Boundary; }} {WITNESS}"
    ))
    .expect("enum declaration and case selection compile");
    let CoreType::Variant { cases } = &core.types["Spine"] else {
        panic!("an enum is a Core variant: {:?}", core.types)
    };
    assert_eq!(
        cases.keys().map(String::as_str).collect::<Vec<_>>(),
        ["Boundary", "LeftSpine", "Range", "RightSpine"]
    );
    assert!(cases.values().all(Option::is_none), "{cases:?}");
    assert_eq!(
        core.required_core_capabilities,
        vec!["core.variant/v1".to_owned()]
    );
    let CoreExpr::Variant { ty, case, payload } = &core.functions["first"].body.result else {
        panic!("case selection is a payload-free variant expression")
    };
    assert_eq!(ty, "jedit.spines@1.Spine");
    assert_eq!(case, "Boundary");
    assert!(payload.is_none());
    digest_core_module(&core).expect("valid canonical Core");
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn enum_matches_cover_every_case_and_lower_to_core_matches() {
    let body = format!(
        "{SPINE} fn rank(spine: Spine) -> U8 {{ return match spine {{ Boundary => 0u8, RightSpine => 1u8, LeftSpine => 2u8, Range => 3u8 }}; }} fn ranked() -> U8 {{ return rank(Spine.Range); }} {WITNESS}"
    );
    let core = compile(&body).expect("exhaustive enum match");
    let CoreExpr::Match { arms, .. } = &core.functions["rank"].body.result else {
        panic!("match")
    };
    assert_eq!(
        arms.iter().map(|arm| arm.case.as_str()).collect::<Vec<_>>(),
        ["Boundary", "RightSpine", "LeftSpine", "Range"]
    );
    assert!(arms.iter().all(|arm| arm.binder.is_none()));
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn enum_cases_are_selected_through_transparent_aliases() {
    let core = compile(&format!(
        "{SPINE} type Side = Spine; fn left() -> Side {{ return Side.LeftSpine; }}"
    ))
    .expect("alias selects the same enum");
    assert!(!core.types.contains_key("Side"));
    let CoreExpr::Variant { ty, case, .. } = &core.functions["left"].body.result else {
        panic!("variant expression")
    };
    assert_eq!(
        (ty.as_str(), case.as_str()),
        ("jedit.spines@1.Spine", "LeftSpine")
    );
}

#[test]
fn enum_matches_refuse_missing_duplicate_unknown_and_bound_arms() {
    for arms in [
        "Boundary => 0u8, RightSpine => 1u8, LeftSpine => 2u8",
        "Boundary => 0u8, Boundary => 0u8, RightSpine => 1u8, LeftSpine => 2u8, Range => 3u8",
        "Boundary => 0u8, RightSpine => 1u8, LeftSpine => 2u8, Range => 3u8, Middle => 4u8",
        "Boundary(x) => 0u8, RightSpine => 1u8, LeftSpine => 2u8, Range => 3u8",
    ] {
        refusal(
            &format!("{SPINE} fn rank(spine: Spine) -> U8 {{ return match spine {{ {arms} }}; }}"),
            CompilerErrorKind::TypeMismatch,
        );
    }
}

#[test]
fn enum_and_variant_case_syntax_do_not_overlap() {
    refusal(
        &format!("{SPINE} fn first() -> Spine {{ return Spine::Boundary; }}"),
        CompilerErrorKind::TypeMismatch,
    );
    refusal(
        "type Choice = variant { Empty, Data(U8) }; fn empty() -> Choice { return Choice.Empty; }",
        CompilerErrorKind::TypeMismatch,
    );
    refusal(
        &format!("{SPINE} fn first() -> Spine {{ return Spine.Middle; }}"),
        CompilerErrorKind::UnknownField,
    );
}

#[test]
fn duplicate_enum_cases_refuse() {
    refusal(
        "enum Spine { Boundary, Boundary }",
        CompilerErrorKind::SurfaceValidation,
    );
}

#[test]
fn linked_enums_keep_their_module_coordinate() {
    let library = "package jedit.spine@1; enum Spine { Boundary, Range }";
    let root = format!(
        "package jedit.spines@1; use module jedit.spine@1 digest \"{}\" as spine; fn first() -> spine.Spine {{ return spine.Spine.Range; }}",
        source_module_digest(library).unwrap().to_review_string()
    );
    let closure =
        resolve_source_closure(&root, &[library], SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).expect("linked enum case path");
    let core = linked
        .compile(&CompilerContext::new())
        .expect("linked enum compiles");
    assert_eq!(core.functions.len(), 1, "{:?}", core.functions);
    let function = core.functions.values().next().expect("linked helper");
    let CoreExpr::Variant { ty, case, payload } = &function.body.result else {
        panic!("variant expression: {:?}", core.functions)
    };
    assert_eq!(
        (ty.as_str(), case.as_str()),
        ("jedit.spine@1.Spine", "Range")
    );
    assert!(payload.is_none());
}
