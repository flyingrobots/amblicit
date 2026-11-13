//! Core and Target IR forms of typed function failures.
//!
//! Oracle: the "Core and Target representation" part of the language spec's
//! Typed Function Failures section (EDICT-CORE-FAIL-001,
//! EDICT-TARGET-FAIL-001). Forged Core is built by editing compiled Core so
//! each case breaks exactly one rule of the integrity judgment.
use std::collections::BTreeMap;

use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, encode_core_module, encode_target_ir_artifact,
    lower_to_target_ir, parse_module, validate_core_module_type_integrity, CanonicalValue,
    CompilerContext, CoreBudget, CoreExpr, CoreModule, CoreNode, CoreTypeIntegrityFailureKind,
    ResourceRef, TargetIrArtifact, TargetIrLoweringFacts, TargetIrOrderEntry,
    TargetLoweringFailureKind, TargetLoweringStatus,
};

const TOO_SMALL: &str = "rope.example@1.ClosureTooSmall";
const CONFLICT: &str = "rope.example@1.ConflictingContent";

const BASE: &str = r"package rope.example@1;
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
fn retain(value: U64) -> U64 { return value; }
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let value = both(input.value, input.other) else {
    ClosureTooSmall => text.ClosureInsufficient,
    ConflictingContent => text.ContentIdentityMismatch,
  };
  let kept = retain(value);
  return { value: kept };
}
";

fn core(source: &str) -> CoreModule {
    let budget = CoreBudget {
        max_steps: 100_000,
        max_allocated_bytes: u64::MAX,
        max_output_bytes: u64::MAX,
    };
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget("p.small", budget);
    let module = parse_module(source).expect("typed-failure source parses");
    compile_to_core(&module, &context)
        .unwrap_or_else(|errors| panic!("typed-failure source compiles: {errors:#?}"))
}

fn canonical(core: &CoreModule) -> CanonicalValue {
    decode_canonical_cbor(&encode_core_module(core).expect("Core encodes")).expect("Core decodes")
}

fn entry<'a>(value: &'a CanonicalValue, key: &str) -> Option<&'a CanonicalValue> {
    let CanonicalValue::Map(entries) = value else {
        panic!("canonical map expected, got {value:?}")
    };
    entries
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.to_owned())).then_some(value))
}

fn field<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    entry(value, key).unwrap_or_else(|| panic!("canonical field `{key}` is present"))
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

fn facts(domain: &str) -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: domain.into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn lowered(domain: &str) -> TargetIrArtifact {
    let report = lower_to_target_ir(&core(BASE), &facts(domain));
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
    report.artifact.expect("lowered artifact")
}

fn forged_refusal(core: &CoreModule) -> (CoreTypeIntegrityFailureKind, String) {
    let failure = validate_core_module_type_integrity(core)
        .expect_err("the Core integrity judgment refuses forged failure constructs");
    (failure.kind(), failure.path().to_owned())
}

#[test]
fn core_failure_constructs_round_trip_canonically() {
    let value = canonical(&core(BASE));
    let functions = field(&value, "functions");
    assert_eq!(
        field(field(functions, "both"), "failures"),
        &CanonicalValue::Array(vec![text(TOO_SMALL), text(CONFLICT)])
    );
    assert_eq!(entry(field(functions, "retain"), "failures"), None);
    let result = field(field(field(functions, "checked"), "body"), "result");
    let fail = field(result, "else");
    assert_eq!(field(fail, "kind"), &text("fail"));
    assert_eq!(field(fail, "failure"), &text(TOO_SMALL));
    let nodes = field(
        field(field(field(&value, "intents"), "evaluate"), "body"),
        "nodes",
    );
    let CanonicalValue::Array(nodes) = nodes else {
        panic!("nodes are an array")
    };
    assert_eq!(field(&nodes[0], "kind"), &text("attempt"));
    assert_eq!(field(field(&nodes[0], "value"), "kind"), &text("call"));
    let map = field(&nodes[0], "failureMap");
    assert_eq!(
        field(field(map, TOO_SMALL), "reasonKind"),
        &text("text.ClosureInsufficient")
    );
    assert_eq!(
        field(field(map, CONFLICT), "payload"),
        &CanonicalValue::Map(Vec::new())
    );
    assert_eq!(field(&nodes[1], "kind"), &text("let"));
    assert!(matches!(
        field(&value, "requiredCoreCapabilities"),
        CanonicalValue::Array(capabilities) if capabilities.contains(&text("core.failure/v1"))
    ));
}

#[test]
fn failure_free_modules_carry_no_failure_fields_or_capability() {
    let source = BASE
        .replace("failure ClosureTooSmall;\nfailure ConflictingContent;\n", "")
        .replace(" fails ClosureTooSmall", "")
        .replace(" fails ConflictingContent, ClosureTooSmall", "")
        .replace("else fail ClosureTooSmall", "else 0u64")
        .replace("else fail ConflictingContent", "else 0u64")
        .replace("expect checked", "checked")
        .replace(
            " else {\n    ClosureTooSmall => text.ClosureInsufficient,\n    ConflictingContent => text.ContentIdentityMismatch,\n  }",
            "",
        );
    let core = core(&source);
    let value = canonical(&core);
    let CanonicalValue::Map(functions) = field(&value, "functions") else {
        panic!("function table")
    };
    for (_, function) in functions {
        assert_eq!(entry(function, "failures"), None);
    }
    assert!(!core
        .required_core_capabilities
        .iter()
        .any(|capability| capability == "core.failure/v1"));
    assert!(matches!(
        core.intents["evaluate"].body.nodes[0],
        CoreNode::Let { .. }
    ));
}

#[test]
fn core_integrity_refuses_forged_function_failure_sets() {
    let compiled = core(BASE);
    validate_core_module_type_integrity(&compiled).expect("compiled Core is valid");
    let invalid = CoreTypeIntegrityFailureKind::InvalidDefinition;

    let mut unsorted = compiled.clone();
    unsorted
        .functions
        .get_mut("both")
        .expect("both")
        .failures
        .reverse();
    assert_eq!(
        forged_refusal(&unsorted),
        (invalid, "functions.both.failures".to_owned())
    );

    let mut inexact = compiled.clone();
    inexact
        .functions
        .get_mut("both")
        .expect("both")
        .failures
        .retain(|failure| failure != CONFLICT);
    assert_eq!(
        forged_refusal(&inexact),
        (invalid, "functions.both.failures".to_owned())
    );

    let mut overdeclared = compiled.clone();
    overdeclared
        .functions
        .get_mut("checked")
        .expect("checked")
        .failures
        .push(CONFLICT.to_owned());
    assert_eq!(
        forged_refusal(&overdeclared),
        (invalid, "functions.checked.failures".to_owned())
    );

    let mut failing_step = compiled;
    failing_step
        .required_core_capabilities
        .push("core.fold/v1".to_owned());
    failing_step
        .functions
        .get_mut("retain")
        .expect("retain")
        .body
        .result = CoreExpr::Fold {
        initial: Box::new(CoreExpr::Local {
            reference: failing_step.functions["retain"].params[0].clone(),
        }),
        step: "rope.example@1.checked".to_owned(),
        bound: 1,
        state_type: "U64".to_owned(),
    };
    assert_eq!(
        forged_refusal(&failing_step),
        (invalid, "functions.retain.body".to_owned())
    );
}

#[test]
fn core_integrity_refuses_forged_intent_failure_constructs() {
    let compiled = core(BASE);
    let invalid = CoreTypeIntegrityFailureKind::InvalidDefinition;

    let mut misplaced = compiled.clone();
    misplaced
        .intents
        .get_mut("evaluate")
        .expect("intent")
        .body
        .result = CoreExpr::Fail {
        failure: TOO_SMALL.to_owned(),
    };
    assert_eq!(
        forged_refusal(&misplaced),
        (invalid, "intents.evaluate.body.result".to_owned())
    );

    let mut unmapped = compiled.clone();
    let nodes = &mut unmapped
        .intents
        .get_mut("evaluate")
        .expect("intent")
        .body
        .nodes;
    let CoreNode::Attempt { binding, value, .. } = nodes[0].clone() else {
        panic!("attempt node")
    };
    nodes[0] = CoreNode::Let { binding, value };
    assert_eq!(
        forged_refusal(&unmapped),
        (invalid, "intents.evaluate.body.nodes[0]".to_owned())
    );

    let mut partial = compiled.clone();
    let CoreNode::Attempt { failure_map, .. } = &mut partial
        .intents
        .get_mut("evaluate")
        .expect("intent")
        .body
        .nodes[0]
    else {
        panic!("attempt node")
    };
    failure_map.remove(CONFLICT);
    assert_eq!(
        forged_refusal(&partial),
        (
            invalid,
            "intents.evaluate.body.nodes[0].failureMap".to_owned()
        )
    );

    let mut dead = compiled.clone();
    let nodes = &mut dead.intents.get_mut("evaluate").expect("intent").body.nodes;
    let CoreNode::Let { binding, value } = nodes[1].clone() else {
        panic!("failure-free let")
    };
    nodes[1] = CoreNode::Attempt {
        binding,
        value,
        failure_map: BTreeMap::new(),
    };
    assert_eq!(
        forged_refusal(&dead),
        (invalid, "intents.evaluate.body.nodes[1]".to_owned())
    );

    let mut uncapable = compiled;
    uncapable
        .required_core_capabilities
        .retain(|capability| capability != "core.failure/v1");
    assert_eq!(
        forged_refusal(&uncapable),
        (invalid, "requiredCoreCapabilities".to_owned())
    );
}

#[test]
fn attempt_lowers_to_failure_armed_pure_binding() {
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let artifact = lowered(domain);
        let intent = &artifact.intents["evaluate"];
        let attempt = &intent.pure_bindings[0];
        assert_eq!(
            attempt
                .failure_arms
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [TOO_SMALL, CONFLICT],
            "{domain}"
        );
        assert_eq!(
            attempt.failure_arms[CONFLICT].kind,
            "text.ContentIdentityMismatch"
        );
        assert_eq!(intent.pure_bindings[1].failure_arms, BTreeMap::new());
        assert_eq!(
            intent.execution_order.as_deref(),
            Some(
                [
                    TargetIrOrderEntry::Instruction(attempt.id.clone()),
                    TargetIrOrderEntry::Instruction(intent.pure_bindings[1].id.clone()),
                ]
                .as_slice()
            ),
            "{domain}"
        );
        let value = decode_canonical_cbor(
            &encode_target_ir_artifact(&artifact).expect("Target IR encodes"),
        )
        .expect("Target IR decodes");
        let bindings = field(field(field(&value, "intents"), "evaluate"), "pureBindings");
        let CanonicalValue::Array(bindings) = bindings else {
            panic!("pure bindings are an array")
        };
        let arms = field(&bindings[0], "failureArms");
        assert_eq!(
            field(field(arms, TOO_SMALL), "reasonKind"),
            &text("text.ClosureInsufficient")
        );
        assert_eq!(entry(&bindings[1], "failureArms"), None, "{domain}");
    }
}

#[test]
fn pre_step_guard_target_refuses_failure_mapped_bindings() {
    let report = lower_to_target_ir(&core(BASE), &facts("echo.span-ir/v1"));
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:#?}"
    );
    assert!(
        report.failures.iter().any(|failure| failure.kind
            == TargetLoweringFailureKind::UnsupportedCoreNode
            && failure.intent.as_deref() == Some("evaluate")
            && failure.node_index == Some(0)
            && failure.detail == "failure_mapped_binding"),
        "{report:#?}"
    );
}
