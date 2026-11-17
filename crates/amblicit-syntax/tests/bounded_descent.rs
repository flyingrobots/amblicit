//! Iterative descent and rebuild over depth-bounded arena trees (#262).
//!
//! Oracle: the reference fixture is the textbook AVL join and split written
//! with an explicit path stack; the owner ruling of 2026-10-21 requires it to
//! need no recursive type, recursive function or new Core node.
use amblicit_syntax::{
    compile_to_core, lower_to_target_ir, parse_module, CompilerContext, CompilerErrorKind,
    CoreBudget, CoreModule, ParseErrorKind, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringFailureKind, TargetLoweringStatus,
};

const ROPE: &str = include_str!("../../../fixtures/lang/bounded-descent/avl-rope.amblicit");
const CEILING: u64 = u64::MAX / 4;

fn budget(steps: u64, bytes: u64) -> CoreBudget {
    CoreBudget {
        max_steps: steps,
        max_allocated_bytes: bytes,
        max_output_bytes: CEILING,
    }
}

fn compile_rope(
    source: &str,
    budget: CoreBudget,
) -> Result<CoreModule, Vec<amblicit_syntax::CompilerError>> {
    let module = parse_module(source).expect("reference rope parses");
    compile_to_core(
        &module,
        &CompilerContext::new()
            .with_operation_profile("rope.read", "continuum.profile.read-only/v1")
            .with_budget("rope.split", budget),
    )
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v3".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

/// Smallest value in `0..=CEILING` that `accepts`, which must be monotone.
fn minimum(mut accepts: impl FnMut(u64) -> bool) -> u64 {
    assert!(accepts(CEILING), "generous-budget control");
    let (mut low, mut high) = (0, CEILING);
    while low < high {
        let middle = low + (high - low) / 2;
        if accepts(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    high
}

fn compile_errors(body: &str) -> Vec<amblicit_syntax::CompilerError> {
    let source = format!("package descent.refusals@1; {body}");
    compile_to_core(
        &parse_module(&source).expect("refusal fixture parses"),
        &CompilerContext::new(),
    )
    .expect_err("recursive or unbounded shape must refuse")
}

#[test]
fn iterative_avl_rope_compiles_and_lowers_with_fold_and_list_operations() {
    let core = compile_rope(ROPE, budget(CEILING, CEILING))
        .unwrap_or_else(|errors| panic!("reference rope compiles: {errors:#?}"));
    let capabilities: Vec<&str> = core
        .required_core_capabilities
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(
        capabilities,
        ["core.failure/v1", "core.fold/v1", "core.list/v1"],
        "{capabilities:?}"
    );
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
}

#[test]
fn rope_budget_boundary_is_rederived_by_native_target() {
    for steps_axis in [true, false] {
        let axis = |value| {
            if steps_axis {
                budget(value, CEILING)
            } else {
                budget(CEILING, value)
            }
        };
        let source_min = minimum(|value| compile_rope(ROPE, axis(value)).is_ok());
        let control = compile_rope(ROPE, axis(source_min)).expect("source minimum compiles");
        assert_eq!(
            lower_to_target_ir(&control, &facts()).status,
            TargetLoweringStatus::Lowered,
            "native Target admits the source minimum, steps={steps_axis}"
        );
        let native_min = minimum(|value| {
            let mut core = control.clone();
            core.intents
                .get_mut("splitRope")
                .unwrap()
                .core_evaluation_budget = axis(value);
            lower_to_target_ir(&core, &facts()).status == TargetLoweringStatus::Lowered
        });
        assert!(
            source_min >= native_min,
            "source {source_min} < native {native_min}"
        );
        let mut forged = control.clone();
        forged
            .intents
            .get_mut("splitRope")
            .unwrap()
            .core_evaluation_budget = axis(native_min - 1);
        let refused = lower_to_target_ir(&forged, &facts());
        assert_eq!(refused.status, TargetLoweringStatus::Unsupported);
        assert!(
            refused
                .failures
                .iter()
                .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "{:?}",
            refused.failures
        );
    }
}

#[test]
fn raising_the_split_bound_raises_the_minimum_step_budget() {
    let split_fold = "fold(initial, splitStep, 18u64)";
    assert_eq!(
        ROPE.matches(split_fold).count(),
        1,
        "fixture has one split fold"
    );
    let min_steps = |bound: u64| {
        let source = ROPE.replace(split_fold, &format!("fold(initial, splitStep, {bound}u64)"));
        minimum(|value| compile_rope(&source, budget(value, CEILING)).is_ok())
    };
    let (shorter, reference, longer) = (min_steps(17), min_steps(18), min_steps(19));
    assert!(
        shorter < reference && reference < longer,
        "{shorter} {reference} {longer}"
    );
}

#[test]
fn recursive_shapes_are_refused_with_stable_kinds() {
    use CompilerErrorKind as K;
    for (body, kind) in [
        ("fn h(x: U64) -> U64 { return if x == 0u64 then 0u64 else h(0u64); }", K::UnsupportedSourceShape),
        ("fn a(x: U64) -> U64 { return b(x); } fn b(x: U64) -> U64 { return a(x); }", K::UnsupportedSourceShape),
        ("fn s(x: U64) -> U64 { return fold(x, s, 2u64); }", K::UnsupportedSourceShape),
        ("fn s(x: List<U8,max=2>) -> List<U8,max=2> { return listAppend(x, 1u8); } fn r(x: List<U8,max=2>) -> List<U8,max=2> { return fold(x, s, 2u64); }", K::UnsupportedSourceShape),
        ("fn s(x: List<U8,max=2>) -> List<U8,max=4> { return listConcat(x, x); } fn r(x: List<U8,max=2>) -> List<U8,max=4> { return fold(x, s, 2u64); }", K::TypeMismatch),
        ("fn s(x: U64) -> U64 { return x; } fn r(x: U64, depth: U64) -> U64 { return fold(x, s, depth); }", K::InvalidBound),
    ] {
        let errors = compile_errors(body);
        assert!(errors.iter().any(|e| e.kind == kind), "{body}: expected {kind:?}, got {errors:?}");
    }
}

#[test]
fn depth_clauses_and_termination_measures_do_not_parse() {
    for source in [
        "package descent.syntax@1; type Tree = variant { Empty, Leaf(U8) } depth <= 4;",
        "package descent.syntax@1; fn h(x: U64) -> U64 decreases x { return x; }",
    ] {
        let error = parse_module(source).expect_err("recursion syntax is not v1");
        assert_eq!(
            error.kind,
            ParseErrorKind::ExpectedToken,
            "{source}: {error:?}"
        );
    }
}

#[test]
fn recursive_types_report_one_integrity_diagnostic() {
    for types in [
        "type Tree = variant { Empty, Branch(Tree) };",
        "type Pair = { left: Tree }; type Tree = variant { Empty, Branch(Pair) };",
    ] {
        let errors = compile_errors(&format!("{types} fn f(t: Tree) -> Bool {{ return true; }}"));
        let kinds: Vec<_> = errors.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            [CompilerErrorKind::InvalidCoreTypeIntegrity],
            "{types}: {errors:?}"
        );
    }
}
