//! Ordered Core attempts and their v2 Target IR projection witness.
//!
//! Oracle: ordered Core nodes define which failure is observed first. A
//! canonically encoded Target IR with reordered independent attempts must not
//! verify against the Core-bound result projection.

use amblicit_syntax::{
    compile_to_core, emit_result_projection, encode_target_ir_artifact, parse_module,
    verify_result_projection, CompilerContext, CoreBudget, ResultProjectionFailureKind,
    TargetIrLoweringFacts, TargetIrOrderEntry, TargetLoweringStatus,
};

const SOURCE: &str = r"package rope.example@1;
failure ClosureTooSmall;
failure ConflictingContent;
type Input = { value: U64, other: U64, };
type Output = { value: U64, };
fn checked(value: U64) -> U64 fails ClosureTooSmall {
  return if value < 10u64 then value else fail ClosureTooSmall;
}
fn both(value: U64, other: U64) -> U64 fails ConflictingContent, ClosureTooSmall {
  let first = expect checked(value);
  return if first == other then first else fail ConflictingContent;
}
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let first = both(input.value, input.other) else {
    ClosureTooSmall => text.ClosureInsufficient,
    ConflictingContent => text.ContentIdentityMismatch,
  };
  let second = both(input.other, input.value) else {
    ClosureTooSmall => text.ClosureInsufficient,
    ConflictingContent => text.ContentIdentityMismatch,
  };
  return { value: first };
}
";

fn core_and_v2_target(
    source: &str,
) -> (
    amblicit_syntax::CoreModule,
    amblicit_syntax::TargetIrArtifact,
) {
    let budget = CoreBudget {
        max_steps: 100_000,
        max_allocated_bytes: u64::MAX,
        max_output_bytes: u64::MAX,
    };
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget("p.small", budget);
    let module = parse_module(source).expect("attempt-order source parses");
    let core = compile_to_core(&module, &context)
        .unwrap_or_else(|errors| panic!("attempt-order source compiles: {errors:#?}"));
    let facts = TargetIrLoweringFacts {
        target_profile: amblicit_syntax::ResourceRef {
            coordinate: "echo.dpo@1".to_owned(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v2".to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    };
    let report = amblicit_syntax::lower_to_target_ir(&core, &facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
    (core, report.artifact.expect("v2 Target IR"))
}

#[test]
fn result_projection_verification_refuses_reordered_independent_attempts() {
    let (core, target) = core_and_v2_target(SOURCE);
    let projection = emit_result_projection(&core, &target, "evaluate")
        .expect("original Core order emits a projection");
    verify_result_projection(
        &core,
        &target,
        "evaluate",
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("projection verifies against original Core order");

    let mut reordered = target.clone();
    let intent = reordered.intents.get_mut("evaluate").expect("intent");
    assert_eq!(intent.pure_bindings.len(), 2, "two independent attempts");
    let first_id = intent.pure_bindings[0].id.clone();
    let second_id = intent.pure_bindings[1].id.clone();
    let order = intent
        .execution_order
        .as_mut()
        .expect("ordered v2 sequence");
    let position = |id: &str, order: &[TargetIrOrderEntry]| {
        order
            .iter()
            .position(|entry| {
                matches!(entry, TargetIrOrderEntry::Instruction(found) if found.as_str() == id)
            })
            .expect("attempt instruction is in the order")
    };
    let first_position = position(&first_id, order);
    let second_position = position(&second_id, order);
    order.swap(first_position, second_position);

    encode_target_ir_artifact(&reordered)
        .expect("the reordered but internally valid v2 Target IR encodes");
    assert_eq!(
        verify_result_projection(
            &core,
            &reordered,
            "evaluate",
            projection.canonical_bytes(),
            projection.digest(),
        )
        .expect_err("projection must bind the Core attempt order")
        .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

#[test]
fn result_projection_verification_refuses_attempt_reordered_across_requirement() {
    let source = SOURCE.replace(
        "  let second = both(input.other, input.value)",
        "  require input.value == input.other else text.ContentIdentityMismatch;\n  let second = both(input.other, input.value)",
    );
    let (core, target) = core_and_v2_target(&source);
    let projection = emit_result_projection(&core, &target, "evaluate")
        .expect("original Core order emits a projection");
    verify_result_projection(
        &core,
        &target,
        "evaluate",
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("projection verifies against original Core order");

    let mut reordered = target.clone();
    let intent = reordered.intents.get_mut("evaluate").expect("intent");
    let first_id = intent.pure_bindings[0].id.clone();
    let requirement_id = intent.requirements[0].id.clone();
    let order = intent
        .execution_order
        .as_mut()
        .expect("ordered v2 sequence");
    let position = |id: &str, order: &[TargetIrOrderEntry]| {
        order
            .iter()
            .position(|entry| {
                matches!(entry, TargetIrOrderEntry::Instruction(found) if found.as_str() == id)
            })
            .expect("instruction is in the order")
    };
    let first_position = position(&first_id, order);
    let requirement_position = position(&requirement_id, order);
    order.swap(first_position, requirement_position);

    encode_target_ir_artifact(&reordered)
        .expect("the reordered but internally valid v2 Target IR encodes");
    assert_eq!(
        verify_result_projection(
            &core,
            &reordered,
            "evaluate",
            projection.canonical_bytes(),
            projection.digest(),
        )
        .expect_err("projection must bind Attempt and Require Core order")
        .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

#[test]
fn result_projection_verification_refuses_rebound_requirement_rows_in_core_order() {
    let source = SOURCE.replace(
        "  let first = both(input.value, input.other)",
        "  require input.value == input.other else text.ContentIdentityMismatch;\n  require input.value < 10u64 else text.ClosureInsufficient;\n  let first = both(input.value, input.other)",
    );
    let (core, target) = core_and_v2_target(&source);
    let projection = emit_result_projection(&core, &target, "evaluate")
        .expect("original requirements emit a projection");
    verify_result_projection(
        &core,
        &target,
        "evaluate",
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("original requirements verify against Core");

    let mut substituted = target.clone();
    let intent = substituted.intents.get_mut("evaluate").expect("intent");
    assert_eq!(intent.requirements.len(), 2, "two independent requirements");
    assert_ne!(
        intent.requirements[0].predicate,
        intent.requirements[1].predicate
    );
    assert_ne!(
        intent.requirements[0].on_failure,
        intent.requirements[1].on_failure
    );
    let first_id = intent.requirements[0].id.clone();
    let second_id = intent.requirements[1].id.clone();
    // Preserve every canonical table ID and the original execution-order list;
    // exchange only the predicates and terminal obstruction semantics they name.
    intent.requirements.swap(0, 1);
    intent.requirements[0].id = first_id;
    intent.requirements[1].id = second_id;
    assert_eq!(
        intent.execution_order,
        target.intents["evaluate"].execution_order
    );

    encode_target_ir_artifact(&substituted)
        .expect("independent input-only requirements remain canonical and in scope");
    assert_eq!(
        verify_result_projection(
            &core,
            &substituted,
            "evaluate",
            projection.canonical_bytes(),
            projection.digest(),
        )
        .expect_err("preserving instruction IDs cannot substitute Core requirement semantics")
        .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}
