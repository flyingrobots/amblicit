//! Core and Target IR forms of typed function failures.
//!
//! Oracle: the "Core and Target representation" part of the language spec's
//! Typed Function Failures section (EDICT-CORE-FAIL-001,
//! EDICT-TARGET-FAIL-001). Forged Core is built by editing compiled Core so
//! each case breaks exactly one rule of the integrity judgment.
use std::collections::BTreeMap;

use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, encode_core_module, encode_target_ir_artifact,
    lower_to_target_ir, parse_module, validate_core_module_type_integrity, CanonicalErrorKind,
    CanonicalValue, CompilerContext, CoreBudget, CoreExpr, CoreModule, CoreNode,
    CoreTypeIntegrityFailureKind, ResourceRef, TargetIrArtifact, TargetIrLoweringFacts,
    TargetIrOrderEntry, TargetLoweringFailureKind, TargetLoweringStatus,
};

const TOO_SMALL: &str = "rope.example@1.ClosureTooSmall";
const CONFLICT: &str = "rope.example@1.ConflictingContent";

#[path = "typed_failure_lowering/charged_intrinsics.rs"]
mod charged_intrinsics;

#[path = "typed_failure_lowering/terminal_work.rs"]
mod terminal_work;

#[path = "typed_failure_lowering/option_contract.rs"]
mod option_contract;

#[path = "typed_failure_lowering/predicate_cost.rs"]
mod predicate_cost;

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

/// List-capable modules meter called helpers, including their terminal failures.
#[test]
fn typed_failures_lower_when_list_work_is_active() {
    let source = BASE.replace(
        "let kept = retain(value);",
        "let xs: List<U64, max=1> = [value]; let length = listLen(xs); let kept = retain(value);",
    );
    let compiled = core(&source);
    validate_core_module_type_integrity(&compiled).expect("typed-failure Core is valid");
    assert!(
        compiled
            .required_core_capabilities
            .iter()
            .any(|capability| capability == "core.list/v1"),
        "the witness must activate Target's independent work meter"
    );
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(&compiled, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{domain}: {report:#?}"
        );
        assert!(report.artifact.is_some(), "{domain}");
    }
}

/// A terminal failure takes its Option type from context and produces no value.
#[test]
fn contextual_option_failure_branches_lower_as_bottom() {
    let source = BASE.replace(
        "return if value < 10u64 then value else fail ClosureTooSmall;",
        "return default(if value < 10u64 then some(value) else fail ClosureTooSmall, 0u64);",
    );
    let compiled = core(&source);
    validate_core_module_type_integrity(&compiled)
        .expect("contextually typed failure Core is valid");
    assert!(
        compiled
            .required_core_capabilities
            .iter()
            .any(|capability| capability == "core.option/v1"),
        "the witness must exercise the typed Option operand"
    );
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(&compiled, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{domain}: {report:#?}"
        );
        assert!(report.artifact.is_some(), "{domain}");
    }
}

/// The successful arm types a terminal-failure conditional used as an operand.
#[test]
fn terminal_failure_conditional_can_feed_a_comparison() {
    let source = BASE.replace(
        "intent evaluate",
        "fn compare(flag: Bool, value: U64) -> Bool fails ClosureTooSmall { return (if flag then value else fail ClosureTooSmall) == value; }\nintent evaluate",
    );
    let compiled = core(&source);
    validate_core_module_type_integrity(&compiled)
        .expect("contextually typed failure Core is valid");
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(&compiled, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{domain}: {report:#?}"
        );
        assert!(report.artifact.is_some(), "{domain}");
    }
}

#[test]
fn terminal_failure_match_can_feed_a_comparison() {
    let source = BASE.replace(
        "intent evaluate",
        "enum FailureChoice { Pass, Stop }\nfn compare(choice: FailureChoice, value: U64) -> Bool fails ClosureTooSmall { return (match choice { Pass => value, Stop => fail ClosureTooSmall }) == value; }\nintent evaluate",
    );
    let compiled = core(&source);
    validate_core_module_type_integrity(&compiled).expect("contextually typed match Core is valid");
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(&compiled, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{domain}: {report:#?}"
        );
        assert!(report.artifact.is_some(), "{domain}");
    }
}

#[test]
fn option_operations_accept_terminal_failure_operands_without_weakening_signatures() {
    let source = BASE.replace(
        "intent evaluate",
        "fn present(flag: Bool, o: Option<U64>) -> Bool fails ClosureTooSmall { return isSome(if flag then o else fail ClosureTooSmall); }\nfn directDefault(value: U64) -> U64 fails ClosureTooSmall { return default<U64>(fail ClosureTooSmall, value); }\nintent evaluate",
    );
    let compiled = core(&source);
    validate_core_module_type_integrity(&compiled).expect("terminal Option operand Core is valid");
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(&compiled, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{domain}: {report:#?}"
        );
        assert!(report.artifact.is_some(), "{domain}");

        let mut wrong_fallback = compiled.clone();
        let CoreExpr::Call { args, .. } = &mut wrong_fallback
            .functions
            .get_mut("directDefault")
            .expect("helper")
            .body
            .result
        else {
            panic!("default call")
        };
        args[1] = CoreExpr::Const(amblicit_syntax::CoreValue::Bool(true));
        validate_core_module_type_integrity(&wrong_fallback)
            .expect("fallback forgery reaches Target's Option signature judgment");
        let report = lower_to_target_ir(&wrong_fallback, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:#?}"
        );
        assert!(report.artifact.is_none());
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "{report:#?}"
        );

        let mut missing_capability = compiled.clone();
        missing_capability
            .required_core_capabilities
            .retain(|capability| capability != "core.option/v1");
        let report = lower_to_target_ir(&missing_capability, &facts(domain));
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:#?}"
        );
        assert!(report.artifact.is_none());
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "{report:#?}"
        );
    }
}

#[test]
fn terminal_failure_string_branches_can_feed_concatenation() {
    for helper in [
        "fn concatenate(flag: Bool, value: String<max=2>) -> String<max=4> fails ClosureTooSmall { return (if flag then value else fail ClosureTooSmall) + value; }\n",
        "enum FailureChoice { Pass, Stop }\nfn concatenate(choice: FailureChoice, value: String<max=2>) -> String<max=4> fails ClosureTooSmall { return (match choice { Pass => value, Stop => fail ClosureTooSmall }) + value; }\n",
    ] {
        let source = BASE.replace("intent evaluate", &format!("{helper}intent evaluate"));
        let compiled = core(&source);
        validate_core_module_type_integrity(&compiled).expect("terminal String branch Core is valid");
        for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
            let report = lower_to_target_ir(&compiled, &facts(domain));
            assert_eq!(report.status, TargetLoweringStatus::Lowered, "{domain}: {report:#?}");
            assert!(report.artifact.is_some(), "{domain}");
        }
    }
}

/// Source reserves contextual result storage conservatively. This boundary is
/// Target's own reconstruction, not a claim that both minimum budgets coincide.
#[test]
fn terminal_failure_work_keeps_an_exact_target_step_boundary() {
    let source = BASE.replace(
        "let kept = retain(value);",
        "let xs: List<U64, max=1> = [value]; let length = listLen(xs); let kept = retain(value);",
    );
    let compiled = core(&source);
    assert!(compiled
        .required_core_capabilities
        .iter()
        .any(|capability| capability == "core.list/v1"));
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let at = |steps| {
            let mut candidate = compiled.clone();
            candidate
                .intents
                .get_mut("evaluate")
                .expect("intent")
                .core_evaluation_budget
                .max_steps = steps;
            lower_to_target_ir(&candidate, &facts(domain))
        };
        let ample = at(100_000);
        assert_eq!(ample.status, TargetLoweringStatus::Lowered, "{ample:#?}");
        assert!(ample.artifact.is_some());
        let mut low = 0_u64;
        let mut high = 100_000_u64;
        while low < high {
            let middle = low + (high - low) / 2;
            let report = at(middle);
            if report.status == TargetLoweringStatus::Lowered {
                assert!(report.artifact.is_some());
                high = middle;
            } else {
                assert_eq!(
                    report.status,
                    TargetLoweringStatus::Unsupported,
                    "{report:#?}"
                );
                assert!(report.artifact.is_none());
                assert_eq!(report.failures.len(), 1, "{report:#?}");
                assert_eq!(
                    report.failures[0].kind,
                    TargetLoweringFailureKind::InvalidCoreIdentity
                );
                assert_eq!(report.failures[0].intent.as_deref(), Some("evaluate"));
                assert_eq!(report.failures[0].node_index, None);
                assert_eq!(
                    report.failures[0].detail,
                    "fold-capable Core work is unestablished or exceeds the evaluation budget"
                );
                low = middle + 1;
            }
        }
        assert!(high > 1, "terminal failure must not erase surrounding work");
        let boundary = at(high);
        assert_eq!(
            boundary.status,
            TargetLoweringStatus::Lowered,
            "{boundary:#?}"
        );
        assert!(boundary.artifact.is_some());
        let short = at(high - 1);
        assert_eq!(
            short.status,
            TargetLoweringStatus::Unsupported,
            "{short:#?}"
        );
        assert!(short.artifact.is_none());
        assert_eq!(short.failures.len(), 1, "{short:#?}");
        assert_eq!(
            short.failures[0].kind,
            TargetLoweringFailureKind::InvalidCoreIdentity
        );
        assert_eq!(short.failures[0].intent.as_deref(), Some("evaluate"));
        assert_eq!(short.failures[0].node_index, None);
        assert_eq!(
            short.failures[0].detail,
            "fold-capable Core work is unestablished or exceeds the evaluation budget"
        );
    }
}

#[test]
fn failure_arm_payloads_reference_only_prior_locals_in_execution_order() {
    let source = BASE.replace(
        "let value = both(input.value, input.other)",
        "let prior = input.value; let value = both(input.value, input.other)",
    );
    let compiled = core(&source);
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let report = lower_to_target_ir(&compiled, &facts(domain));
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
        let mut valid = report.artifact.expect("compiler artifact");
        let bindings = &valid.intents["evaluate"].pure_bindings;
        assert_eq!(bindings.len(), 3, "prior, attempt, then retained result");
        let prior = bindings[0].binding.clone();
        let failed = bindings[1].binding.clone();
        let later = bindings[2].binding.clone();
        let CoreExpr::Field { base, .. } = &bindings[0].value else {
            panic!("the prior binding reads the input")
        };
        let CoreExpr::Local { reference: input } = base.as_ref() else {
            panic!("the input is a local")
        };
        let input = input.clone();
        let mut outside = prior.clone();
        outside.id = "outside.failure.payload".into();
        outside.alpha_name = "$outsideFailurePayload".into();
        valid
            .intents
            .get_mut("evaluate")
            .expect("intent")
            .pure_bindings[1]
            .failure_arms
            .get_mut(TOO_SMALL)
            .expect("failure arm")
            .payload
            .insert("observed".into(), CoreExpr::Local { reference: prior });
        encode_target_ir_artifact(&valid).expect("a prior local is available in the failure arm");
        let mut valid_input = valid.clone();
        valid_input
            .intents
            .get_mut("evaluate")
            .expect("intent")
            .pure_bindings[1]
            .failure_arms
            .get_mut(TOO_SMALL)
            .expect("failure arm")
            .payload
            .insert("observed".into(), CoreExpr::Local { reference: input });
        encode_target_ir_artifact(&valid_input)
            .expect("the application input is available in the failure arm");
        for (case, reference) in [
            ("failed result itself", failed),
            ("later local", later),
            ("out-of-scope local", outside),
        ] {
            let mut forged = valid.clone();
            forged
                .intents
                .get_mut("evaluate")
                .expect("intent")
                .pure_bindings[1]
                .failure_arms
                .get_mut(TOO_SMALL)
                .expect("failure arm")
                .payload
                .insert("observed".into(), CoreExpr::Local { reference });
            assert_eq!(
                encode_target_ir_artifact(&forged).expect_err(case).kind(),
                CanonicalErrorKind::UnsupportedValue,
                "{domain}: {case}"
            );
        }
    }
}
