//! Failed bindings retain their cause without hiding independent errors.
use edict_syntax::{
    compile_to_core, decode_lawpack_adapter, decode_lawpack_bundle, encode_core_module,
    parse_module, prepare_lawpack_compilation, CompilerError, CompilerErrorKind, CoreModule,
};

const SOURCE: &str = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");

fn compile(source: &str) -> Result<CoreModule, Vec<CompilerError>> {
    compile_with_surface(source, true)
}

fn compile_with_surface(source: &str, surface: bool) -> Result<CoreModule, Vec<CompilerError>> {
    compile_with_profile(source, surface, None)
}

fn compile_with_profile(
    source: &str,
    surface: bool,
    profile: Option<(&str, &str)>,
) -> Result<CoreModule, Vec<CompilerError>> {
    let module = parse_module(source).expect("recovery fixture parses");
    let bundle = decode_lawpack_bundle(
        include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor"),
        include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor"),
    )
    .expect("exact lawpack");
    let adapter = decode_lawpack_adapter(
        &bundle,
        "echo.dpo@1",
        include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor"),
    )
    .expect("exact adapter");
    let prepared = prepare_lawpack_compilation(&module, &bundle, &adapter)
        .expect("authenticated recovery context");
    let mut context = prepared.compiler_context().clone();
    if let Some((coordinate, meaning)) = profile {
        context = context
            .with_operation_profile(coordinate, meaning)
            .with_operation_profile_write_classes(coordinate, [edict_syntax::WriteClass::Read]);
    }
    if surface {
        compile_to_core(&module, &context)
    } else {
        let resolved = edict_syntax::resolve_module(&module, &context)?;
        let typed = edict_syntax::type_check(&resolved)?;
        edict_syntax::lower_core(&typed)
    }
}

fn insert_bindings(bindings: &str) -> String {
    assert_eq!(SOURCE.matches("  let receipt:").count(), 1);
    SOURCE.replace("  let receipt:", &format!("{bindings}\n  let receipt:"))
}

fn failed_call() -> String {
    SOURCE.replace("hello.createGreeting(input)", "hello.createGreeting(true)")
}

fn check_kinds(source: &str, kinds: &[CompilerErrorKind]) -> Vec<CompilerError> {
    let errors = compile(source).expect_err("invalid program never produces Core");
    assert_eq!(
        errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
        kinds
    );
    errors
}

#[test]
fn valid_recovery_control_retains_exact_core_bytes() {
    let core = compile(SOURCE).expect("valid effect binding compiles");
    assert_eq!(
        encode_core_module(&core).expect("canonical Core"),
        include_bytes!("../../../fixtures/lawpack/hello-echo/create-greeting.core.cbor")
    );
}

#[test]
fn failed_effect_and_dependent_receipt_report_only_the_cause() {
    let source = failed_call();
    let errors = check_kinds(&source, &[CompilerErrorKind::TypeMismatch]);
    assert!(source[errors[0].span.start..errors[0].span.end].contains("hello.createGreeting(true)"));
}

#[test]
fn poisoned_locals_propagate_through_chained_and_nested_bindings() {
    let source = insert_bindings("  let failed = missingCause;\n  let chained = failed;\n  let nested = { item: chained, };\n  let dependent = nested.item;");
    let errors = check_kinds(&source, &[CompilerErrorKind::UnresolvedType]);
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "missingCause"
    );
}

#[test]
fn poisoned_record_sibling_does_not_hide_independent_expression_error() {
    let source = failed_call().replace("message: input.message", "message: input.absent");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::TypeMismatch,
            CompilerErrorKind::UnknownField,
        ],
    );
    assert!(source[errors[1].span.start..errors[1].span.end].contains("input.absent"));
}

#[test]
fn poisoned_record_sibling_does_not_hide_an_independent_type_mismatch() {
    let source = failed_call().replace("message: input.message", "message: true");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::TypeMismatch,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(&source[errors[1].span.start..errors[1].span.end], "true");
}

#[test]
fn independent_later_binding_errors_survive_recovery() {
    let source = failed_call().replace(
        "  return {",
        "  let separate: Bool = input.message;\n  return {",
    );
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::TypeMismatch,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert!(source[errors[1].span.start..errors[1].span.end].contains("let separate:"));
}

#[test]
fn failed_shadow_does_not_use_outer_value_or_escape_its_branch() {
    let source = insert_bindings("  let outer = 1u64;\n  if true {\n    let outer = missingShadow;\n    let dependent: Bool = outer;\n  }\n  let independent: Bool = outer;");
    // Source shadowing stays rejected by the front end. The public type-check
    // phase still must recover safely when explicitly given this resolved AST.
    check_kinds(&source, &[CompilerErrorKind::SurfaceValidation]);
    let errors =
        compile_with_surface(&source, false).expect_err("invalid typed shadow produces no Core");
    assert_eq!(
        errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch
        ]
    );
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "missingShadow"
    );
    assert!(source[errors[1].span.start..errors[1].span.end].contains("let independent:"));
}

#[test]
fn failed_branch_local_does_not_poison_an_unknown_outer_name() {
    let source = insert_bindings("  if true {\n    let branchOnly = missingBranch;\n    let dependent = branchOnly;\n  }\n  let independent = branchOnly;");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "missingBranch"
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "branchOnly"
    );
}

#[test]
fn failed_return_is_distinct_from_a_genuinely_absent_return() {
    let wrong = SOURCE.replace("message: input.message", "message: true");
    check_kinds(&wrong, &[CompilerErrorKind::TypeMismatch]);
    let start = SOURCE.find("  return {").expect("return begins");
    let absent = format!("{} }}\n", &SOURCE[..start]);
    let errors = check_kinds(&absent, &[CompilerErrorKind::TypeMismatch]);
    assert_eq!(
        errors[0].span,
        parse_module(&absent)
            .expect("absent-return source")
            .decls
            .iter()
            .find_map(|decl| match decl {
                edict_syntax::ast::Decl::Intent(intent) => Some(intent.span),
                _ => None,
            })
            .expect("intent")
    );
}

#[test]
fn poisoned_condition_does_not_hide_independent_branch_errors() {
    let source = insert_bindings("  let failed = missingCondition;\n  if failed { let left = missingLeft; } else { let right = missingRight; }");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    let origins: Vec<_> = errors
        .iter()
        .map(|error| &source[error.span.start..error.span.end])
        .collect();
    assert_eq!(origins, ["missingCondition", "missingLeft", "missingRight"]);
}

#[test]
fn poisoned_concatenation_does_not_hide_an_independent_operand_error() {
    let source = insert_bindings(
        "  let failed = missingConcatCause;\n  let combined = failed + missingConcatSibling;",
    );
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingConcatSibling"
    );
}

#[test]
fn poisoned_concatenation_does_not_hide_an_independent_operand_type_error() {
    let source =
        insert_bindings("  let failed = missingConcatTypeCause;\n  let combined = failed + true;");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(&source[errors[1].span.start..errors[1].span.end], "true");
}

#[test]
fn failed_yield_branch_does_not_hide_an_independent_other_branch() {
    let source = insert_bindings("  let chosen: Bool = if true {\n    let failed = missingYield;\n    yield failed;\n  } else {\n    let independent = missingElse;\n    yield true;\n  };");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "missingYield"
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingElse"
    );
}

#[test]
fn poisoned_obstruction_shorthand_keeps_only_its_cause() {
    let valid =
        insert_bindings("  let failed = true;\n  require true else example.Failure({ failed });");
    compile(&valid).expect("valid obstruction shorthand control");
    let source = valid.replace("let failed = true;", "let failed = missingPayloadCause;");
    let errors = check_kinds(&source, &[CompilerErrorKind::UnresolvedType]);
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "missingPayloadCause"
    );
    let unknown = insert_bindings("  require true else example.Failure({ unknownPayload });");
    let errors = check_kinds(&unknown, &[CompilerErrorKind::UnresolvedType]);
    assert_eq!(
        &unknown[errors[0].span.start..errors[0].span.end],
        "unknownPayload"
    );
}

#[test]
fn poisoned_yield_condition_keeps_both_branch_errors() {
    let valid = insert_bindings("  let failed = true;\n  let chosen: Bool = if failed { let left = true; yield left; } else { let right = false; yield right; };");
    compile(&valid).expect("valid annotated yield control");
    let source = valid
        .replace("let failed = true;", "let failed = missingYieldCondition;")
        .replace("let left = true;", "let left = missingYieldLeft;")
        .replace("let right = false;", "let right = missingYieldRight;");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    let origins: Vec<_> = errors
        .iter()
        .map(|error| &source[error.span.start..error.span.end])
        .collect();
    assert_eq!(
        origins,
        [
            "missingYieldCondition",
            "missingYieldLeft",
            "missingYieldRight"
        ]
    );
}

#[test]
fn poisoned_logical_operand_keeps_the_independent_peer() {
    for op in ["&&", "||"] {
        let valid = insert_bindings(&format!(
            "  let failed = true;\n  if failed {op} false {{ let branch = true; }}"
        ));
        compile(&valid).expect("valid logical predicate control");
        let source = valid
            .replace("let failed = true;", "let failed = missingLogicalCause;")
            .replace(
                &format!("failed {op} false"),
                &format!("failed {op} missingLogicalPeer"),
            );
        let errors = check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::UnresolvedType,
            ],
        );
        assert_eq!(
            &source[errors[1].span.start..errors[1].span.end],
            "missingLogicalPeer"
        );
    }
}

#[test]
fn source_function_failed_locals_preserve_later_causes() {
    let valid = "package recovery.functions@1; fn recover() -> Bool { let failed = true; let dependent = failed; let independent = false; return dependent; } intent evaluate(input: Bool) returns Bool profile p.read basis none budget <= p.large { let anchor = input; return anchor; }";
    compile_function_recovery(valid).expect("valid source-function recovery control");
    let source = valid
        .replace("let failed = true;", "let failed = missingFunctionCause;")
        .replace(
            "let independent = false;",
            "let independent = missingFunctionSibling;",
        );
    let errors =
        compile_function_recovery(&source).expect_err("invalid function never produces Core");
    assert_eq!(
        errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType
        ]
    );
    let origins: Vec<_> = errors
        .iter()
        .map(|error| &source[error.span.start..error.span.end])
        .collect();
    assert_eq!(origins, ["missingFunctionCause", "missingFunctionSibling"]);
}

fn compile_function_recovery(source: &str) -> Result<CoreModule, Vec<CompilerError>> {
    let module = parse_module(source).expect("function recovery source parses");
    let context = edict_syntax::CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.large",
            edict_syntax::CoreBudget {
                max_steps: 4096,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        );
    compile_to_core(&module, &context)
}

#[test]
fn poisoned_require_predicate_keeps_failure_payload_causes() {
    for arm in [
        "example.Failure({ payload: true })",
        "continue obstructed { reason: example.Failure, payload: true }",
    ] {
        let valid = insert_bindings(&format!(
            "  let failed = true;\n  require failed else {arm};"
        ));
        compile(&valid).expect("valid require failure arm control");
        let source = valid
            .replace("let failed = true;", "let failed = missingRequireCause;")
            .replace("payload: true", "payload: missingRequirePayload");
        let errors = check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::UnresolvedType,
            ],
        );
        assert_eq!(
            &source[errors[1].span.start..errors[1].span.end],
            "missingRequirePayload"
        );
    }
}

#[test]
fn invalid_require_reason_keeps_independent_payload_cause() {
    let valid = insert_bindings(
        "  require true else continue obstructed { reason: example.Failure, payload: true };",
    );
    compile(&valid).expect("valid continuing obstruction control");
    let source = valid
        .replace("reason: example.Failure", "reason: true")
        .replace("payload: true", "payload: missingIndependentPayload");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnsupportedSourceShape,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingIndependentPayload"
    );
}

#[test]
fn poisoned_record_retains_non_record_annotation_mismatch() {
    let valid = insert_bindings("  let failed = true;\n  let chosen: Bool = { item: failed };");
    check_kinds(&valid, &[CompilerErrorKind::TypeMismatch]);
    let compatible = valid.replace("chosen: Bool", "chosen");
    compile(&compatible).expect("valid record family control");
    let source = valid.replace("let failed = true;", "let failed = missingRecordCause;");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert!(source[errors[1].span.start..errors[1].span.end].contains("{ item: failed }"));
}

#[test]
fn poisoned_comparison_operand_keeps_independent_peer() {
    for op in ["==", "!=", "<", "<=", ">", ">="] {
        let valid = insert_bindings(&format!(
            "  let failed = 1u64;\n  if failed {op} 2u64 {{ let branch = true; }}"
        ));
        compile(&valid).expect("valid typed comparison control");
        let source = valid
            .replace("let failed = 1u64;", "let failed = missingComparisonCause;")
            .replace(
                &format!("failed {op} 2u64"),
                &format!("failed {op} missingComparisonPeer"),
            );
        let errors = check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::UnresolvedType,
            ],
        );
        assert_eq!(
            &source[errors[1].span.start..errors[1].span.end],
            "missingComparisonPeer"
        );
    }
}

#[test]
fn poisoned_pure_conditionals_preserve_independent_arms() {
    let valid = insert_bindings(
        "  let failed = true;\n  let chosen: Bool = if failed then true else false;",
    );
    compile(&valid).expect("valid annotated conditional control");
    let source = valid
        .replace(
            "let failed = true;",
            "let failed = missingConditionalCause;",
        )
        .replace("then true else false", "then missingThen else missingElse");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    let origins: Vec<_> = errors
        .iter()
        .map(|e| &source[e.span.start..e.span.end])
        .collect();
    assert_eq!(
        origins,
        ["missingConditionalCause", "missingThen", "missingElse"]
    );
    for annotation in [": Bool", ""] {
        let valid = insert_bindings(&format!(
            "  let failed = true;\n  let chosen{annotation} = if true then failed else false;"
        ));
        compile(&valid).expect("valid conditional arm control");
        let source = valid
            .replace("let failed = true;", "let failed = missingArmCause;")
            .replace("else false", "else missingIndependentArm");
        let errors = check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::UnresolvedType,
            ],
        );
        assert_eq!(
            &source[errors[1].span.start..errors[1].span.end],
            "missingIndependentArm"
        );
    }
}

#[test]
fn poisoned_conditional_arm_keeps_independent_expected_type_error() {
    let valid = insert_bindings(
        "  let failed = true;\n  let chosen: Bool = if true then failed else false;",
    );
    compile(&valid).expect("valid conditional type control");
    let source = valid
        .replace("let failed = true;", "let failed = missingArmTypeCause;")
        .replace("else false", "else \"wrong\"");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "\"wrong\""
    );
}

#[test]
fn contextual_yield_inference_preserves_both_branch_causes() {
    for (left, right) in [
        ("{ first: 0, second: 1u64 }", "{ first: 1u64, second: 0 }"),
        ("0", "1u64"),
        ("1u64", "0"),
        ("{ first: 1u64 }", "{ first: 0 }"),
    ] {
        let valid = insert_bindings(&format!("  let chosen = if true {{ let left = true; yield {left}; }} else {{ let right = false; yield {right}; }};"));
        compile(&valid).expect("valid contextual yield control");
        let source = valid
            .replace("let left = true;", "let left = missingContextualThen;")
            .replace("let right = false;", "let right = missingContextualElse;");
        let errors = check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::UnresolvedType,
            ],
        );
        let origins: Vec<_> = errors
            .iter()
            .map(|e| &source[e.span.start..e.span.end])
            .collect();
        assert_eq!(origins, ["missingContextualThen", "missingContextualElse"]);
    }
}

#[test]
fn poisoned_reason_roots_keep_only_independent_causes() {
    let valid = insert_bindings(
        "  require true else continue obstructed { reason: example.Failure, payload: true };",
    );
    compile(&valid).expect("stable reason coordinate control");
    for reason in ["failed", "failed.detail"] {
        let source = insert_bindings(&format!("  let failed = missingReasonCause;\n  require true else continue obstructed {{ reason: {reason}, payload: missingReasonPeer }};"));
        let errors = check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::UnresolvedType,
            ],
        );
        assert_eq!(
            &source[errors[1].span.start..errors[1].span.end],
            "missingReasonPeer"
        );
    }
    let live = insert_bindings(
        "  let live = true;\n  require true else continue obstructed { reason: live };",
    );
    check_kinds(&live, &[CompilerErrorKind::UnsupportedSourceShape]);
}

#[test]
fn poisoned_record_retains_missing_required_field_mismatch() {
    let valid = insert_bindings(
        "  let failed = true;\n  let chosen: RecoveryExpected = { item: failed, required: true };",
    )
    .replace(
        "intent createGreeting",
        "type RecoveryExpected = { item: Bool, required: Bool, };\nintent createGreeting",
    );
    compile(&valid).expect("valid required record keys control");
    let all_keys = valid.replace("let failed = true;", "let failed = missingRecordKeyCause;");
    check_kinds(&all_keys, &[CompilerErrorKind::UnresolvedType]);
    let source = all_keys.replace(", required: true", "");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "{ item: failed }"
    );
}

#[test]
fn poisoned_record_retains_poisoned_extra_field_mismatch() {
    let valid = insert_bindings(
        "  let failed = true;\n  let chosen: RecoveryExpected = { item: true, required: true };",
    )
    .replace(
        "intent createGreeting",
        "type RecoveryExpected = { item: Bool, required: Bool, };\nintent createGreeting",
    );
    compile(&valid).expect("valid closed record control");
    let source = valid
        .replace("let failed = true;", "let failed = missingExtraKeyCause;")
        .replace("required: true }", "required: true, extra: failed }");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(&source[errors[1].span.start..errors[1].span.end], "failed");
}

#[test]
fn poisoned_subtraction_operand_preserves_independent_peer() {
    let valid = insert_bindings("  let failed = 9u64;\n  let distance: U64 = failed - 0u64;");
    compile(&valid).expect("valid proven subtraction control");
    let source = valid
        .replace("let failed = 9u64;", "let failed = missingSubtractCause;")
        .replace("failed - 0u64", "failed - missingSubtractPeer");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingSubtractPeer"
    );
    let source = source.replace("missingSubtractPeer", "false");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(&source[errors[1].span.start..errors[1].span.end], "false");
    let source = source.replace("distance: U64", "distance");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnsupportedSourceShape,
        ],
    );
    assert_eq!(&source[errors[1].span.start..errors[1].span.end], "false");
}

#[test]
fn poisoned_payload_fields_retain_duplicate_key_errors() {
    let valid = insert_bindings(
        "  let failed = true;\n  require true else example.Failure({ failed, other: true });",
    );
    compile(&valid).expect("valid unique payload control");
    for payload in [
        "{ failed, failed: true }",
        "{ payload: failed, payload: true }",
        "{ failed: failed, failed }",
    ] {
        let live = valid.replace("{ failed, other: true }", payload);
        check_kinds(
            &live,
            &[CompilerErrorKind::DuplicateObstructionPayloadField],
        );
        let source = live.replace("let failed = true;", "let failed = missingDuplicateCause;");
        check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::DuplicateObstructionPayloadField,
            ],
        );
    }
}

#[test]
fn source_call_arguments_preserve_later_causes() {
    check_call_argument_recovery(true);
}

#[test]
fn imported_call_arguments_preserve_later_causes() {
    check_call_argument_recovery(false);
}

fn check_call_argument_recovery(source_owned: bool) {
    let declaration = if source_owned {
        "fn pair(left: Bool, right: Bool) -> Bool { return left; }"
    } else {
        "use lawpack recovery.helpers@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;"
    };
    let call = if source_owned { "pair" } else { "helpers.pair" };
    let valid = format!("package recovery.calls@1; {declaration} intent evaluate(input: Bool) returns Bool profile p.read basis none budget <= p.large {{ let failed = true; let result = {call}(failed, false); return result; }}");
    compile_call_recovery(&valid).expect("valid source/imported call control");
    let source = valid
        .replace("let failed = true;", "let failed = missingArgumentCause;")
        .replace("(failed, false)", "(failed, missingArgumentPeer)");
    let errors = compile_call_recovery(&source).expect_err("invalid call never produces Core");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType
        ]
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingArgumentPeer"
    );
    let source = source.replace("missingArgumentPeer", "1u64");
    let errors =
        compile_call_recovery(&source).expect_err("invalid argument type never produces Core");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch
        ]
    );
    assert_eq!(&source[errors[1].span.start..errors[1].span.end], "1u64");
}

fn compile_call_recovery(source: &str) -> Result<CoreModule, Vec<CompilerError>> {
    compile_call_recovery_with_parameter(source, "Bool")
}

fn compile_call_recovery_with_parameter(
    source: &str,
    parameter: &str,
) -> Result<CoreModule, Vec<CompilerError>> {
    compile_call_recovery_phase(source, parameter, true)
}

fn compile_call_recovery_phase(
    source: &str,
    parameter: &str,
    surface: bool,
) -> Result<CoreModule, Vec<CompilerError>> {
    let module = parse_module(source).expect("call recovery source parses");
    let lawpack = edict_syntax::ResourceRef {
        coordinate: "recovery.helpers@1".into(),
        digest: Some(format!("sha256:{}", "1".repeat(64))),
    };
    let budget = edict_syntax::CoreBudget {
        max_steps: 4096,
        max_allocated_bytes: u64::MAX,
        max_output_bytes: u64::MAX,
    };
    let mut context = edict_syntax::CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget("p.large", budget.clone());
    if source.contains("use lawpack") {
        context = context
            .with_pure_function(
                "helpers.pair",
                edict_syntax::PureFunctionFact {
                    lawpack: lawpack.clone(),
                    coordinate: "recovery.helpers@1.pair".into(),
                    type_parameters: vec![],
                    parameter_types: vec![parameter.into(), "Bool".into()],
                    return_type: "Bool".into(),
                    cost_template: "recovery.helpers@1.cost".into(),
                },
            )
            .with_pure_helper_cost(
                "helpers.cost",
                edict_syntax::PureHelperCostFact {
                    lawpack,
                    coordinate: "recovery.helpers@1.cost".into(),
                    budget: edict_syntax::CoreBudget {
                        max_steps: 1,
                        max_allocated_bytes: 64,
                        max_output_bytes: 1,
                    },
                },
            );
    }
    if surface {
        compile_to_core(&module, &context)
    } else {
        let resolved = edict_syntax::resolve_module(&module, &context)?;
        let typed = edict_syntax::type_check(&resolved)?;
        edict_syntax::lower_core(&typed)
    }
}

fn valid_recovery_request() -> String {
    format!(
        r#"package recovery.requests@1;
use capability workspace.snapshot.observe@1 digest "sha256:{operation}" as snapshot;
type RequestInput = {{ payload: Bytes<max=1024>, scope: Bytes<max=32>, basis: Bytes<max=32>, }};
intent evaluate(input: RequestInput) returns ExternalActionRequest<Bytes<max=65536>>
  profile p.read basis input.basis budget <= p.large {{
  request pending: ExternalActionRequest<Bytes<max=65536>> = snapshot(input.payload)
    input schema workspace.snapshot.input@1 digest "sha256:{schema}"
    settlement schema workspace.snapshot.settlement@1 digest "sha256:{settlement}"
    authority input.scope basis input.basis
    budget maxSettlementBytes 65536u64 maxAttempts 4u32
    reconcile workspace.snapshot.reconcile@1 digest "sha256:{law}";
  return pending;
}}"#,
        operation = "a".repeat(64),
        schema = "b".repeat(64),
        settlement = "c".repeat(64),
        law = "d".repeat(64)
    )
}

#[test]
fn failed_request_binding_suppresses_dependent_return_only() {
    let valid = valid_recovery_request();
    compile_function_recovery(&valid).expect("valid request control compiles");
    let source = valid.replace("snapshot(input.payload)", "snapshot(missingRequestCause)");
    let errors =
        compile_function_recovery(&source).expect_err("failed request cannot produce Core");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [CompilerErrorKind::UnresolvedType]
    );
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "missingRequestCause"
    );
    let source = source.replace(
        "return pending;",
        "let independent = missingRequestPeer; return pending;",
    );
    let errors =
        compile_function_recovery(&source).expect_err("independent request error retained");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType
        ]
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingRequestPeer"
    );
}

#[test]
fn invalid_annotations_preserve_independent_initializer_causes() {
    let valid = insert_bindings("  let chosen: Bool = true;");
    compile(&valid).expect("valid annotated initializer control");
    let source = valid.replace(
        "chosen: Bool = true",
        "chosen: MissingRecoveryType = missingAnnotationPeer",
    );
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingAnnotationPeer"
    );
    let valid = "package recovery.annotations@1; fn recover() -> Bool { let chosen: Bool = true; return true; } intent evaluate(input: Bool) returns Bool profile p.read basis none budget <= p.large { let anchor = input; return anchor; }";
    compile_function_recovery(valid).expect("valid function annotation control");
    let source = valid.replace(
        "chosen: Bool = true",
        "chosen: MissingRecoveryType = missingFunctionAnnotationPeer",
    );
    let errors = compile_function_recovery(&source).expect_err("invalid function annotation");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType
        ]
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingFunctionAnnotationPeer"
    );
}

#[test]
fn invalid_annotation_yields_preserve_both_branch_causes() {
    let valid = insert_bindings("  let chosen: U64 = if true { let left = true; yield 0; } else { let right = false; yield 1; };");
    compile(&valid).expect("valid annotated integer yield control");
    let source = valid
        .replace("chosen: U64", "chosen: MissingRecoveryType")
        .replace("let left = true;", "let left = missingAnnotationLeft;")
        .replace("let right = false;", "let right = missingAnnotationRight;");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingAnnotationLeft"
    );
    assert_eq!(
        &source[errors[2].span.start..errors[2].span.end],
        "missingAnnotationRight"
    );
}

#[test]
fn unavailable_annotations_do_not_invent_literal_width() {
    let source = insert_bindings("  let chosen: MissingRecoveryType = 0;");
    check_kinds(&source, &[CompilerErrorKind::UnresolvedType]);
    let independent =
        insert_bindings("  let chosen: MissingRecoveryType = 0;\n  let independent = 1;");
    let errors = check_kinds(
        &independent,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(&independent[errors[1].span.start..errors[1].span.end], "1");
    let source = source.replace(" = 0;", " = 18446744073709551616u64;");
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ],
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "18446744073709551616u64"
    );
}

#[test]
fn poisoned_slice_operands_preserve_independent_causes() {
    let valid = "package recovery.slices@1; intent evaluate(input: Bytes<max=4>) returns Bool profile p.read basis none budget <= p.large { let failed = input; let piece = slice(failed, 0u64, 0u64); return true; }";
    compile_function_recovery(valid).expect("valid empty byte slice control");
    let source = valid
        .replace("let failed = input;", "let failed = missingBytesCause;")
        .replace(
            "slice(failed, 0u64, 0u64)",
            "slice(failed, missingStart, missingEnd)",
        );
    let errors = compile_function_recovery(&source).expect_err("invalid slice cannot produce Core");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType
        ]
    );
    let origins: Vec<_> = errors
        .iter()
        .map(|e| &source[e.span.start..e.span.end])
        .collect();
    assert_eq!(origins, ["missingBytesCause", "missingStart", "missingEnd"]);
    let source = source.replace("missingEnd", "false");
    let errors = compile_function_recovery(&source).expect_err("invalid endpoint type retained");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch
        ]
    );
    assert_eq!(&source[errors[2].span.start..errors[2].span.end], "false");
}

#[test]
fn poisoned_loop_iterators_preserve_independent_body_causes() {
    let valid = "package recovery.loops@1; intent evaluate(input: List<Bool, max=4>) returns Bool profile p.read basis none budget <= p.large { let failed = input; for item in failed bounded 4 { let dependent = item; let independent = false; } return true; }";
    compile_function_recovery(valid).expect("valid bounded loop control");
    let source = valid
        .replace("let failed = input;", "let failed = missingIteratorCause;")
        .replace(
            "let independent = false;",
            "let independent = missingLoopBody;",
        );
    let errors =
        compile_function_recovery(&source).expect_err("invalid iterator cannot produce Core");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType
        ]
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingLoopBody"
    );
    let source = source.replace("return true;", "let outside = item; return true;");
    let errors =
        compile_function_recovery(&source).expect_err("loop binder cannot escape its body");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType
        ]
    );
    assert_eq!(&source[errors[2].span.start..errors[2].span.end], "item");
    let wrong_family = valid
        .replace("let failed = input;", "let failed = true;")
        .replace(
            "let independent = false;",
            "let independent = missingWrongFamilyBody;",
        );
    let errors = compile_function_recovery(&wrong_family)
        .expect_err("invalid iterator family still checks body");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::TypeMismatch,
            CompilerErrorKind::UnresolvedType
        ]
    );
    let wrong_bound = valid
        .replace("bounded 4", "bounded 1")
        .replace("let independent = false;", "let independent: U64 = item;");
    let errors = compile_function_recovery(&wrong_bound)
        .expect_err("known item type survives a bad bound for diagnostics");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::InvalidBound,
            CompilerErrorKind::TypeMismatch
        ]
    );
}

#[test]
fn failed_effect_inputs_preserve_independent_map_errors() {
    let valid = insert_bindings("  let failed = input;");
    compile(&valid).expect("valid mapped effect control");
    let duplicate = "else { alreadyExists(existing) => hello.AlreadyExists, alreadyExists(other) => hello.AlreadyExists }";
    let source = valid
        .replace("let failed = input;", "let failed = missingEffectCause;")
        .replace(
            "hello.createGreeting(input)",
            "hello.createGreeting(failed)",
        )
        .replace(
            "else { alreadyExists(existing) => hello.AlreadyExists }",
            duplicate,
        );
    check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::DuplicateObstructionFailure,
        ],
    );
    let source = SOURCE
        .replace(
            "let receipt: hello.GreetingReceipt",
            "let receipt: MissingRecoveryType",
        )
        .replace(
            "hello.createGreeting(input)",
            "hello.createGreeting(missingEffectArgument)",
        )
        .replace(
            "else { alreadyExists(existing) => hello.AlreadyExists }",
            duplicate,
        );
    check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::DuplicateObstructionFailure,
        ],
    );
}

#[test]
fn unavailable_outer_annotations_preserve_intrinsic_family_errors() {
    for (valid_expression, invalid_expression, kind) in [
        (
            "if true then true else false",
            "if 0 then true else false",
            CompilerErrorKind::ExpectedPredicate,
        ),
        (
            "\"x\" + \"x\"",
            "0 + \"x\"",
            CompilerErrorKind::TypeMismatch,
        ),
        ("len(input)", "len(0)", CompilerErrorKind::TypeMismatch),
        (
            "slice(input, 0u64, 0u64)",
            "slice(0, 0u64, 0u64)",
            CompilerErrorKind::TypeMismatch,
        ),
    ] {
        let valid = format!("package recovery.intrinsics@1; intent evaluate(input: Bytes<max=4>) returns Bool profile p.read basis none budget <= p.large {{ let chosen = {valid_expression}; return true; }}");
        compile_function_recovery(&valid).expect("valid intrinsic family control");
        let invalid = valid.replace(valid_expression, invalid_expression);
        let errors =
            compile_function_recovery(&invalid).expect_err("intrinsically invalid operand");
        assert_eq!(errors.iter().map(|e| e.kind).collect::<Vec<_>>(), [kind]);
        let source = invalid.replace("let chosen =", "let chosen: MissingRecoveryType =");
        let errors = compile_function_recovery(&source)
            .expect_err("outer annotation cannot hide an intrinsic family error");
        assert_eq!(
            errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
            [CompilerErrorKind::UnresolvedType, kind]
        );
    }
}

#[test]
fn failed_request_clauses_preserve_independent_causes() {
    let valid = valid_recovery_request();
    compile_function_recovery(&valid).expect("valid request control compiles");
    for fail_annotation in [false, true] {
        let mut source = valid
            .replace("snapshot(input.payload)", "snapshot(missingOperation)")
            .replace(
                "authority input.scope basis input.basis",
                "authority missingAuthority basis missingBasis",
            )
            .replace(
                "maxSettlementBytes 65536u64 maxAttempts 4u32",
                "maxSettlementBytes missingBytes maxAttempts missingAttempts",
            );
        if fail_annotation {
            source = source.replace(
                "request pending: ExternalActionRequest<Bytes<max=65536>>",
                "request pending: MissingRequestType",
            );
        }
        let errors = compile_function_recovery(&source).expect_err("invalid request has no Core");
        assert_eq!(
            errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![CompilerErrorKind::UnresolvedType; 5 + usize::from(fail_annotation)]
        );
        let origins: Vec<_> = errors
            .iter()
            .skip(usize::from(fail_annotation))
            .map(|e| &source[e.span.start..e.span.end])
            .collect();
        assert_eq!(
            origins,
            [
                "missingOperation",
                "missingAuthority",
                "missingBasis",
                "missingBytes",
                "missingAttempts"
            ]
        );
    }
}

#[test]
fn unavailable_imported_parameter_preserves_argument_causes() {
    let valid = "package recovery.calls@1; use lawpack recovery.helpers@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers; intent evaluate(input: Bool) returns Bool profile p.read basis none budget <= p.large { let result = helpers.pair(true, false); return result; }";
    compile_call_recovery(valid).expect("valid imported helper control");
    for argument in ["missingArgument", "0", "18446744073709551616u64"] {
        let source = valid.replace("pair(true, false)", &format!("pair({argument}, false)"));
        let errors =
            compile_call_recovery_with_parameter(&source, "recovery.helpers@1.MissingType")
                .expect_err("unavailable signature never produces Core");
        let expected = match argument {
            "missingArgument" => vec![
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::UnresolvedType,
            ],
            "0" => vec![CompilerErrorKind::UnresolvedType],
            _ => vec![
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::TypeMismatch,
            ],
        };
        assert_eq!(errors.iter().map(|e| e.kind).collect::<Vec<_>>(), expected);
        if errors.len() > 1 {
            assert_eq!(&source[errors[1].span.start..errors[1].span.end], argument);
        }
    }
}

#[test]
fn poisoned_callees_do_not_resolve_outer_helpers() {
    for (declaration, root, callee) in [
        ("fn pair(left: Bool, right: Bool) -> Bool { return left; }", "pair", "pair"),
        ("use lawpack recovery.helpers@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;", "helpers", "helpers.pair"),
    ] {
        let valid = format!("package recovery.calls@1; {declaration} intent evaluate(input: Bool) returns Bool profile p.read basis none budget <= p.large {{ let result = {callee}(true, false); return result; }}");
        compile_call_recovery(&valid).expect("valid helper control");
        let source = valid.replace("let result =", &format!("let {root} = missingCalleeCause; let result =")).replace("(true, false)", "(1u64, missingArgumentPeer)");
        // Exercise the public type-check boundary directly, including ASTs
        // whose shadowing may be refused by the separate surface validator.
        let errors = compile_call_recovery_phase(&source, "Bool", false).expect_err("poisoned callee cannot produce Core");
        assert_eq!(errors.iter().map(|e| e.kind).collect::<Vec<_>>(), [CompilerErrorKind::UnresolvedType, CompilerErrorKind::UnresolvedType]);
        let origins: Vec<_> = errors.iter().map(|e| &source[e.span.start..e.span.end]).collect();
        assert_eq!(origins, ["missingCalleeCause", "missingArgumentPeer"]);
    }
}

#[test]
fn unavailable_annotations_preserve_comparison_width_errors() {
    for expression in [
        "0u64 == 1u64",
        "if 0u64 == 1u64 then true else false",
        "{ flag: 0u64 == 1u64 }",
    ] {
        let valid = insert_bindings(&format!("  let chosen = {expression};"));
        compile(&valid).expect("typed comparison control");
        let invalid = valid.replace("0u64 == 1u64", "0 == 1");
        check_kinds(
            &invalid,
            &[
                CompilerErrorKind::TypeMismatch,
                CompilerErrorKind::TypeMismatch,
            ],
        );
        let source = invalid.replace("let chosen =", "let chosen: MissingComparisonType =");
        check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::TypeMismatch,
                CompilerErrorKind::TypeMismatch,
            ],
        );
    }
}

#[test]
fn disallowed_effect_profile_preserves_independent_binding_errors() {
    compile(SOURCE).expect("valid effect profile control");
    let source = SOURCE.replace("profile hello.createGreeting", "profile recovery.read");
    let profile = Some(("recovery.read", "continuum.profile.read-only/v1"));
    let errors =
        compile_with_profile(&source, true, profile).expect_err("read-only profile cannot create");
    assert_eq!(
        errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
        [CompilerErrorKind::ProfileEffectMismatch]
    );
    let source = source.replace(
        "hello.createGreeting(input)",
        "hello.createGreeting(missingProfileArgument)",
    );
    let errors = compile_with_profile(&source, true, profile)
        .expect_err("invalid effect cannot produce Core");
    assert_eq!(
        errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::ProfileEffectMismatch,
            CompilerErrorKind::UnresolvedType
        ]
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingProfileArgument"
    );
}

#[test]
fn source_call_arity_failure_keeps_argument_causes() {
    check_call_arity_recovery(true);
}

#[test]
fn imported_call_arity_failure_keeps_argument_causes() {
    check_call_arity_recovery(false);
}

fn check_call_arity_recovery(source_owned: bool) {
    let declaration = if source_owned {
        "fn pair(left: Bool, right: Bool) -> Bool { return left; }"
    } else {
        "use lawpack recovery.helpers@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;"
    };
    let call = if source_owned { "pair" } else { "helpers.pair" };
    let valid = format!("package recovery.calls@1; {declaration} intent evaluate(input: Bool) returns Bool profile p.read basis none budget <= p.large {{ let failed = true; let result = {call}(failed, false); return result; }}");
    compile_call_recovery(&valid).expect("valid arity control");
    for (arguments, origins) in [
        ("missingArityPeer", vec!["missingArityPeer"]),
        (
            "failed, missingArityPeer, missingArityExtra",
            vec!["missingArityPeer", "missingArityExtra"],
        ),
        ("failed, false, 1", vec![]),
    ] {
        let source = valid
            .replace("let failed = true;", "let failed = missingArityCause;")
            .replace("(failed, false)", &format!("({arguments})"));
        let errors = compile_call_recovery(&source).expect_err("invalid arity cannot produce Core");
        let mut kinds = vec![
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch,
        ];
        kinds.extend(origins.iter().map(|_| CompilerErrorKind::UnresolvedType));
        assert_eq!(
            errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
            kinds
        );
        assert_eq!(
            &source[errors[0].span.start..errors[0].span.end],
            "missingArityCause"
        );
        for (error, origin) in errors[2..].iter().zip(origins) {
            assert_eq!(&source[error.span.start..error.span.end], origin);
        }
    }
}

#[test]
fn unresolved_helper_preserves_independent_argument_errors() {
    compile(SOURCE).expect("valid authenticated control");
    let source = insert_bindings(
        "  let failed = missingCallCause;\n  let result = absent.helper(failed, missingCallPeer, 1);",
    );
    let errors = check_kinds(
        &source,
        &[
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::UnresolvedFunction,
            CompilerErrorKind::UnresolvedType,
        ],
    );
    assert_eq!(
        &source[errors[2].span.start..errors[2].span.end],
        "missingCallPeer"
    );
}

#[test]
fn rejected_profile_in_yield_branch_keeps_independent_causes() {
    let valid = SOURCE.replace(
        "  let receipt: hello.GreetingReceipt = hello.createGreeting(input)\n    else { alreadyExists(existing) => hello.AlreadyExists };",
        "  let receipt: hello.GreetingReceipt = if true { let nested: hello.GreetingReceipt = hello.createGreeting(input) else { alreadyExists(existing) => hello.AlreadyExists }; let peer = true; yield nested; } else { let other: hello.GreetingReceipt = hello.createGreeting(input) else { alreadyExists(existing) => hello.AlreadyExists }; yield other; };",
    );
    assert_ne!(valid, SOURCE);
    compile(&valid).expect("valid mapped effects in both yield branches");
    let source = valid
        .replace("profile hello.createGreeting", "profile recovery.read")
        .replace("let peer = true;", "let peer = missingYieldPeer;");
    let errors = compile_with_profile(
        &source,
        true,
        Some(("recovery.read", "continuum.profile.read-only/v1")),
    )
    .expect_err("invalid nested effects cannot produce Core");
    assert_eq!(
        errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::ProfileEffectMismatch,
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::ProfileEffectMismatch,
        ]
    );
    assert_eq!(
        &source[errors[1].span.start..errors[1].span.end],
        "missingYieldPeer"
    );
}

#[test]
fn duplicate_obstruction_maps_preserve_independent_arm_errors() {
    compile(SOURCE).expect("valid mapped effect control");
    let valid_arm = "alreadyExists(existing) => hello.AlreadyExists";
    for invalid_arm in [
        "alreadyExists => hello.AlreadyExists",
        "alreadyExists(other) => hello.AlreadyExists(true)",
    ] {
        let single = SOURCE.replace(valid_arm, invalid_arm);
        check_kinds(&single, &[CompilerErrorKind::UnsupportedSourceShape]);
        for arms in [
            format!("{invalid_arm}, {valid_arm}"),
            format!("{valid_arm}, {invalid_arm}"),
        ] {
            let source = SOURCE.replace(valid_arm, &arms);
            let errors = check_kinds(
                &source,
                &[
                    CompilerErrorKind::DuplicateObstructionFailure,
                    CompilerErrorKind::UnsupportedSourceShape,
                ],
            );
            let span = errors[1].span;
            assert!(invalid_arm.contains(&source[span.start..span.end]));
        }
    }
}

#[test]
fn independent_string_bound_overflow_is_a_diagnostic() {
    let valid = insert_bindings("  let huge: String<max=18446744073709551359> = input.message; let joined = huge + input.message;");
    compile(&valid).expect("exact U64 maximum string bound sum control");
    let direct = insert_bindings(
        "  let huge: String<max=18446744073709551615> = input.message; let joined = huge + huge;",
    );
    check_kinds(&direct, &[CompilerErrorKind::InvalidBound]);
    for expression in ["failed + (huge + huge)", "(huge + huge) + failed"] {
        let source = insert_bindings(&format!("  let huge: String<max=18446744073709551615> = input.message; let failed = missingOverflowCause; let joined = {expression};"));
        let errors = check_kinds(
            &source,
            &[
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::InvalidBound,
            ],
        );
        assert_eq!(
            &source[errors[1].span.start..errors[1].span.end],
            "huge + huge"
        );
    }
}

#[test]
fn duplicate_payload_fields_preserve_independent_value_errors() {
    for template in [
        "example.Failure({ PAYLOAD })",
        "continue obstructed { reason: example.Failure, PAYLOAD }",
    ] {
        let valid = insert_bindings(&format!(
            "  require true else {};",
            template.replace("PAYLOAD", "payload: true, other: false")
        ));
        compile(&valid).expect("valid unique payload control");
        for fields in [
            "payload: true, payload: missingIndependentPayload",
            "missingIndependentPayload: true, missingIndependentPayload",
        ] {
            let source = insert_bindings(&format!(
                "  require true else {};",
                template.replace("PAYLOAD", fields)
            ));
            let errors = check_kinds(
                &source,
                &[
                    CompilerErrorKind::DuplicateObstructionPayloadField,
                    CompilerErrorKind::UnresolvedType,
                ],
            );
            assert_eq!(
                &source[errors[1].span.start..errors[1].span.end],
                "missingIndependentPayload"
            );
        }
    }
}

#[test]
fn unavailable_outer_annotations_preserve_utf8_family_errors() {
    let valid = "package recovery.utf8@1; intent evaluate(input: Bytes<max=4>) returns Bool profile p.read basis none budget <= p.large { let chosen = isUtf8(input); return chosen; }";
    compile_function_recovery(valid).expect("bounded UTF-8 predicate control");
    let invalid = valid.replace("isUtf8(input)", "isUtf8(0)");
    let errors = compile_function_recovery(&invalid).expect_err("UTF-8 needs bytes");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [CompilerErrorKind::TypeMismatch]
    );
    let missing = invalid.replace("let chosen =", "let chosen: MissingRecoveryType =");
    let errors = compile_function_recovery(&missing)
        .expect_err("missing outer type cannot suppress an independent UTF-8 operand error");
    assert_eq!(
        errors.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            CompilerErrorKind::UnresolvedType,
            CompilerErrorKind::TypeMismatch
        ]
    );
}

#[test]
fn poisoned_yield_arm_keeps_the_surviving_annotation_mismatch() {
    for (then_value, else_value) in [("failed", "1u64"), ("1u64", "failed")] {
        let source = insert_bindings(&format!(
            "  let failed = missingYieldTypeCause;\n  let chosen: Bool = if true {{ yield {then_value}; }} else {{ yield {else_value}; }};"
        ));
        let valid = source
            .replace("missingYieldTypeCause", "true")
            .replace("1u64", "false");
        compile(&valid).expect("valid annotated yield control");
        let errors = compile(&source).expect_err("invalid yields cannot produce Core");
        assert_eq!(
            errors.iter().map(|error| error.kind).collect::<Vec<_>>(),
            [
                CompilerErrorKind::UnresolvedType,
                CompilerErrorKind::TypeMismatch
            ],
            "{errors:?}",
        );
        assert_eq!(
            &source[errors[0].span.start..errors[0].span.end],
            "missingYieldTypeCause"
        );
        assert_eq!(&source[errors[1].span.start..errors[1].span.end], "1u64");
    }
}
