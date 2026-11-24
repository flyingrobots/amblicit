//! v2 source order binds the effect row's semantics, not only its table ID.
use super::*;
use amblicit_syntax::{encode_target_ir_artifact, ECHO_ORDERED_SPAN_IR_DOMAIN};

#[test]
fn ordered_projection_refuses_rebound_independent_effect_rows() {
    let prelude = SOURCE
        .split("type GreetingCreated")
        .next()
        .expect("fixture prelude");
    let source = format!(
        r"{prelude}
type PairInput = {{ first: hello.CreateGreetingInput, second: hello.CreateGreetingInput }};
type Output = {{}};
intent createGreeting(input: PairInput) returns Output
  profile hello.createGreeting basis input.first.basis budget <= hello.smallCreateBudget
{{
  let first: hello.GreetingReceipt = hello.createGreeting(input.first)
    else {{ alreadyExists(existing) => hello.AlreadyExists }};
  let second: hello.GreetingReceipt = hello.createGreeting(input.second)
    else {{ alreadyExists(existing) => hello.AlreadyExists }};
  return {{}};
}}
"
    );
    let bundle = decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("Hello Echo lawpack");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("adapter");
    let module = parse_module(&source).expect("independent effect source parses");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter).expect("prepare");
    let core = amblicit_syntax::compile_to_core(&module, preparation.compiler_context())
        .expect("independent effect source compiles");
    let mut facts = preparation.target_ir_facts().clone();
    ECHO_ORDERED_SPAN_IR_DOMAIN.clone_into(&mut facts.target_ir_domain);
    let report = lower_to_target_ir(&core, &facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
    let target = report.artifact.expect("ordered Target IR");
    let projection = emit_result_projection(&core, &target, "createGreeting")
        .expect("original effects emit an empty-record projection");
    verify_result_projection(
        &core,
        &target,
        "createGreeting",
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("original effects verify against Core");

    let mut substituted = target.clone();
    let intent = substituted
        .intents
        .get_mut("createGreeting")
        .expect("intent");
    assert_eq!(intent.steps.len(), 2, "two independent effect sites");
    assert_ne!(intent.steps[0].input, intent.steps[1].input);
    let first_id = intent.steps[0].id.clone();
    let second_id = intent.steps[1].id.clone();
    intent.steps.swap(0, 1);
    intent.steps[0].id = first_id;
    intent.steps[1].id = second_id;
    assert_eq!(
        intent.execution_order,
        target.intents["createGreeting"].execution_order
    );
    encode_target_ir_artifact(&substituted)
        .expect("independent input-only effect rows remain canonical and in scope");
    assert_eq!(
        verify_result_projection(
            &core,
            &substituted,
            "createGreeting",
            projection.canonical_bytes(),
            projection.digest(),
        )
        .expect_err("canonical IDs cannot substitute Core effect semantics")
        .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}
