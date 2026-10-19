//! Source-owned function contracts, exercised through actual source compilation.
//! Runtime argument evaluation belongs to the dependent Echo witness.
use std::fmt::Write;

use edict_syntax::{
    compile_to_core, decode_canonical_cbor, digest_core_module, encode_core_module,
    lower_to_target_ir, parse_module, validate_core_module_type_integrity, CanonicalValue,
    CompilerContext, CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode,
    CoreTypeIntegrityFailureKind, CoreValue, PureFunctionFact, PureHelperCostFact, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus,
};

const JIM_BASELINE: &str =
    include_str!("../../../fixtures/lang/functions/range-assembly-baseline.edict");
const JIM_FUNCTIONS: &str = include_str!("../../../fixtures/lang/functions/range-assembly.edict");
const IMPORT: &str = "use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;";

fn context(steps: u64) -> CompilerContext {
    let budget = CoreBudget {
        max_steps: steps,
        max_allocated_bytes: u64::MAX,
        max_output_bytes: u64::MAX,
    };
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_operation_profile("text.replaceRange", "continuum.profile.read-only/v1")
        .with_budget("p.small", budget.clone())
        .with_budget("text.replaceRangeBudget", budget)
}

fn source(functions: &str, expression: &str) -> String {
    format!("package functions.example@1;\n{functions}\ntype Input = {{ value: U64, other: U64, }};\ntype Output = {{ value: U64, }};\nintent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\nreturn {{ value: {expression} }};\n}}")
}

fn target_source(functions: &str, expression: &str) -> String {
    source(functions, expression).replace(
        "\nreturn { value:",
        "\nlet executionAnchor = input.value;\nreturn { value:",
    )
}

fn compile_with(source: &str, context: &CompilerContext) -> CoreModule {
    let parsed = parse_module(source).expect("source-function syntax must parse");
    compile_to_core(&parsed, context).expect("closed pure source functions must compile")
}

fn compile(source: &str) -> CoreModule {
    compile_with(source, &context(4096))
}

fn field<'a>(value: &'a CanonicalValue, name: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(fields) = value else {
        panic!("expected canonical map")
    };
    fields
        .iter()
        .find_map(|(key, value)| (key == &CanonicalValue::Text(name.into())).then_some(value))
        .unwrap_or_else(|| panic!("missing canonical field {name}"))
}

fn canonical(core: &CoreModule) -> CanonicalValue {
    decode_canonical_cbor(&encode_core_module(core).unwrap()).unwrap()
}

fn definition<'a>(value: &'a CanonicalValue, name: &str) -> &'a CanonicalValue {
    field(field(value, "functions"), name)
}

fn array(value: &CanonicalValue) -> &[CanonicalValue] {
    let CanonicalValue::Array(values) = value else {
        panic!("expected canonical array")
    };
    values
}

fn facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: "echo.dpo@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        },
        target_ir_domain: "echo.span-ir/v1".into(),
        operation_profiles: vec!["continuum.profile.read-only/v1".into()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn reject(functions: &str, expression: &str, kind: CompilerErrorKind) {
    let authored = source(functions, expression);
    let parsed = parse_module(&authored).expect("negative type/authority case must reach compiler");
    let errors = compile_to_core(&parsed, &context(4096)).unwrap_err();
    assert!(errors.iter().any(|error| error.kind == kind), "{errors:?}");
    assert!(errors.iter().all(|error| error.span.end <= authored.len()));
}

#[test]
fn source_function_identity_is_not_an_imported_fact() {
    let core = compile(&target_source(
        "fn retain(value: U64) -> U64 { return value; }",
        "retain(input.value)",
    ));
    assert_eq!(core.imports, Vec::<edict_syntax::CoreImport>::new());
    let value = canonical(&core);
    let function = definition(&value, "retain");
    assert_eq!(
        field(function, "returnType"),
        &CanonicalValue::Text("U64".into())
    );
    assert_eq!(array(field(function, "params")).len(), 1);
    let result = field(field(function, "body"), "result");
    assert_eq!(field(result, "kind"), &CanonicalValue::Text("local".into()));
    assert_eq!(field(result, "ref"), &array(field(function, "params"))[0]);
    let CoreExpr::Record { fields } = &core.intents["evaluate"].body.result else {
        panic!("record result")
    };
    let CoreExpr::Call {
        callee,
        args,
        type_args,
    } = &fields["value"]
    else {
        panic!("source call survives")
    };
    assert_eq!(callee, "functions.example@1.retain");
    assert_eq!(args.len(), 1);
    assert_eq!(type_args.as_slice(), [] as [String; 0]);
    let target = lower_to_target_ir(&core, &facts());
    assert_eq!(target.status, TargetLoweringStatus::Lowered, "{target:?}");
    assert_eq!(
        target.artifact.unwrap().intents["evaluate"].result,
        core.intents["evaluate"].body.result
    );
}

#[test]
fn jim_range_assembly_calls_an_authored_function_through_core_and_target() {
    let baseline = compile(JIM_BASELINE);
    let core = compile(JIM_FUNCTIONS);
    assert_ne!(
        digest_core_module(&baseline).unwrap(),
        digest_core_module(&core).unwrap()
    );
    let value = canonical(&core);
    let function = definition(&value, "assembleFragments");
    assert_eq!(array(field(function, "params")).len(), 2);
    let bindings = array(field(field(function, "body"), "bindings"));
    assert_eq!(bindings.len(), 1);
    assert_eq!(
        field(field(&bindings[0], "value"), "callee"),
        &CanonicalValue::Text("core.bytes.concat".into())
    );
    let CoreNode::Let { value, .. } = &core.intents["assembleRange"].body.nodes[0] else {
        panic!("authored let")
    };
    let CoreExpr::Call { callee, args, .. } = value else {
        panic!("source-owned call")
    };
    assert_eq!(callee, "jedit.text.replace_range@1.assembleFragments");
    for (argument, name) in args.iter().zip(["firstFragment", "secondFragment"]) {
        assert!(matches!(argument, CoreExpr::Field { field, .. } if field == name));
    }
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert_eq!(
        report.artifact.unwrap().intents["assembleRange"].pure_bindings[0].value,
        *value
    );
}

#[test]
fn forward_calls_and_ordered_function_locals_keep_lexical_frames() {
    let core = compile(&target_source(
        "fn outer(value: U64) -> U64 { let first = inner(value); let second = inner(first); return second; }\nfn inner(value: U64) -> U64 { return value; }",
        "outer(input.value)",
    ));
    let value = canonical(&core);
    let function = definition(&value, "outer");
    let bindings = array(field(field(function, "body"), "bindings"));
    assert_eq!(bindings.len(), 2);
    assert_eq!(
        field(field(&bindings[0], "value"), "callee"),
        &CanonicalValue::Text("functions.example@1.inner".into())
    );
    let second_args = array(field(field(&bindings[1], "value"), "args"));
    assert_eq!(
        field(&second_args[0], "ref"),
        field(&bindings[0], "binding")
    );
    assert_eq!(
        field(field(field(function, "body"), "result"), "ref"),
        field(&bindings[1], "binding")
    );
    assert_eq!(
        lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
}

#[test]
fn source_function_local_alpha_renaming_preserves_semantic_identity() {
    let original = source(
        "fn retain(value: U64) -> U64 { let copied = value; return copied; }",
        "retain(input.value)",
    );
    let renamed = original
        .replace("retain(value: U64)", "retain(item: U64)")
        .replace(
            "let copied = value; return copied;",
            "let saved = item; return saved;",
        );
    assert_eq!(
        encode_core_module(&compile(&original)).unwrap(),
        encode_core_module(&compile(&renamed)).unwrap()
    );
}

#[test]
fn source_function_body_and_unused_definition_change_identity() {
    let original = source(
        "fn choose(left: U64, right: U64) -> U64 { return left; }",
        "choose(input.value, input.other)",
    );
    let changed = original.replace("return left;", "return right;");
    assert_ne!(
        digest_core_module(&compile(&original)).unwrap(),
        digest_core_module(&compile(&changed)).unwrap()
    );
    let unused = original.replace(
        "type Input",
        "fn unused(value: U64) -> U64 { return value; }\ntype Input",
    );
    assert_ne!(
        digest_core_module(&compile(&original)).unwrap(),
        digest_core_module(&compile(&unused)).unwrap()
    );
}

#[test]
fn source_calls_remain_in_conditional_and_prebody_positions() {
    let authored = source(
        "fn retain(value: U64) -> U64 { return value; }",
        "if input.value == 0u64 then retain(input.other) else input.value",
    )
    .replace(
        "basis none",
        "basis retain(input.value) where retain(input.value) <= input.other",
    );
    let core = compile(&authored);
    let intent = &core.intents["evaluate"];
    assert!(matches!(intent.basis, Some(CoreExpr::Call { .. })));
    assert_eq!(intent.input_constraints.len(), 1);
    assert!(
        intent.body.nodes.is_empty(),
        "function calls must not be hoisted into body bindings"
    );
    let CoreExpr::Record { fields } = &intent.body.result else {
        panic!("record result")
    };
    let CoreExpr::If {
        then_value,
        else_value,
        ..
    } = &fields["value"]
    else {
        panic!("conditional survives")
    };
    assert!(matches!(**then_value, CoreExpr::Call { .. }));
    assert!(matches!(**else_value, CoreExpr::Field { .. }));
    let report = lower_to_target_ir(&core, &facts());
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == TargetLoweringFailureKind::NoTargetSteps));
    let executable = compile(&authored.replace(
        "return { value:",
        "let executionAnchor = input.value;\nreturn { value:",
    ));
    assert_eq!(
        lower_to_target_ir(&executable, &facts()).status,
        TargetLoweringStatus::Lowered
    );
}

#[test]
fn caller_expressions_remain_single_ordered_arguments_even_when_unused() {
    let core = compile(&source(
        "fn retain(value: U64) -> U64 { return value; }\nfn first(left: U64, right: U64) -> U64 { return left; }",
        "first(retain(input.value), retain(input.other))",
    ));
    let CoreExpr::Record { fields } = &core.intents["evaluate"].body.result else {
        panic!("record result")
    };
    let CoreExpr::Call { args, .. } = &fields["value"] else {
        panic!("first call")
    };
    assert_eq!(args.len(), 2);
    for (argument, expected) in args.iter().zip(["value", "other"]) {
        let CoreExpr::Call { args, .. } = argument else {
            panic!("argument call evaluated at its occurrence")
        };
        assert!(matches!(&args[0], CoreExpr::Field { field, .. } if field == expected));
    }
    assert_eq!(core.intents["evaluate"].body.nodes, Vec::<CoreNode>::new());
}

#[test]
fn source_function_arity_and_return_types_are_checked() {
    reject(
        "fn retain(value: U64) -> U64 { return value; }",
        "retain(input.value, input.other)",
        CompilerErrorKind::TypeMismatch,
    );
    reject(
        "fn retain(value: U32) -> U32 { return value; }",
        "retain(input.value)",
        CompilerErrorKind::TypeMismatch,
    );
    reject(
        "fn retain(value: U64) -> U32 { return value; }",
        "input.value",
        CompilerErrorKind::TypeMismatch,
    );
}

#[test]
fn unused_functions_cannot_capture_callers_or_forward_locals() {
    reject(
        "fn hidden(value: U64) -> U64 { return input.value; }",
        "input.value",
        CompilerErrorKind::UnresolvedType,
    );
    reject(
        "fn hidden(value: U64) -> U64 { let first = second; let second = value; return first; }",
        "input.value",
        CompilerErrorKind::UnresolvedType,
    );
}

#[test]
fn source_function_duplicate_binders_and_names_are_rejected() {
    for functions in [
        "fn hidden(value: U64, value: U64) -> U64 { return value; }",
        "fn hidden(value: U64) -> U64 { let value = value; return value; }",
        "fn hidden(value: U64) -> U64 { return value; } fn hidden(value: U64) -> U64 { return value; }",
    ] {
        reject(functions, "input.value", CompilerErrorKind::SurfaceValidation);
    }
}

#[test]
fn source_function_returns_are_terminal_and_required() {
    reject(
        "fn hidden(value: U64) -> U64 { let copy = value; }",
        "input.value",
        CompilerErrorKind::TypeMismatch,
    );
    reject(
        "fn hidden(value: U64) -> U64 { return value; let copy = value; }",
        "input.value",
        CompilerErrorKind::UnsupportedSourceShape,
    );
}

#[test]
fn pure_functions_reject_assertions_and_effect_statements_without_erasure() {
    for statement in [
        "assert value == value;",
        "require value == value else domain.Refused;",
        "target.read(value);",
        "for item in value bounded 1 { let copy = item; }",
    ] {
        reject(
            &format!("fn hidden(value: U64) -> U64 {{ {statement} return value; }}"),
            "input.value",
            CompilerErrorKind::UnsupportedSourceShape,
        );
    }
}

#[test]
fn source_function_recursion_is_rejected_even_when_unused() {
    reject(
        "fn cycle(value: U64) -> U64 { return cycle(value); }",
        "input.value",
        CompilerErrorKind::UnsupportedSourceShape,
    );
    reject("fn first(value: U64) -> U64 { return second(value); } fn second(value: U64) -> U64 { return first(value); }", "input.value", CompilerErrorKind::UnsupportedSourceShape);
}

fn chain(length: usize) -> String {
    let mut functions = String::new();
    for index in 0..length {
        let result = if index + 1 == length {
            "value".into()
        } else {
            format!("step{}(value)", index + 1)
        };
        writeln!(
            functions,
            "fn step{index}(value: U64) -> U64 {{ return {result}; }}"
        )
        .unwrap();
    }
    functions
}

#[test]
fn source_function_call_depth_accepts_128_and_rejects_129() {
    compile(&source(&chain(128), "step0(input.value)"));
    reject(
        &chain(129),
        "step0(input.value)",
        CompilerErrorKind::InvalidBound,
    );
}

#[test]
fn shared_suffix_depth_is_independent_of_definition_order() {
    let functions = chain(128);
    let extra = "fn zroot(value: U64) -> U64 { return step0(value); }\n";
    for declarations in [format!("{functions}{extra}"), format!("{extra}{functions}")] {
        reject(
            &declarations,
            "zroot(input.value)",
            CompilerErrorKind::InvalidBound,
        );
    }
}

fn helper_context(cost: u64, budget: u64) -> CompilerContext {
    let lawpack = ResourceRef {
        coordinate: "example.bounds@1".into(),
        digest: Some(format!("sha256:{}", "1".repeat(64))),
    };
    context(budget)
        .with_pure_function(
            "helpers.bump",
            PureFunctionFact {
                lawpack: lawpack.clone(),
                coordinate: "example.bounds@1.bump".into(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".into()],
                return_type: "U64".into(),
                cost_template: "example.bounds@1.cost".into(),
            },
        )
        .with_pure_helper_cost(
            "helpers.cost",
            PureHelperCostFact {
                lawpack,
                coordinate: "example.bounds@1.cost".into(),
                budget: CoreBudget {
                    max_steps: cost,
                    max_allocated_bytes: 0,
                    max_output_bytes: 0,
                },
            },
        )
}

#[test]
fn transitive_imported_cost_counts_repeated_calls_and_unused_arguments() {
    let functions = format!("{IMPORT}\nfn costly(value: U64) -> U64 {{ return helpers.bump(value); }}\nfn first(left: U64, right: U64) -> U64 {{ return left; }}");
    let positive = source(&functions, "costly(input.value)");
    compile_with(&positive, &helper_context(10, 128));
    let repeated = source(
        &functions,
        "first(costly(input.value), costly(input.other))",
    );
    let parsed = parse_module(&repeated).unwrap();
    let errors = compile_to_core(&parsed, &helper_context(u64::MAX / 2 + 1, u64::MAX)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
}

#[test]
fn transitive_totality_cannot_be_bypassed_by_a_source_function() {
    reject(
        "fn subtract(left: U64, right: U64) -> U64 { return left - right; }",
        "subtract(input.value, input.other)",
        CompilerErrorKind::UnsupportedSourceShape,
    );
    reject("fn cut(bytes: Bytes<max=8>, start: U64, end: U64) -> Bytes<max=8> { return slice(bytes, start, end); }", "input.value", CompilerErrorKind::UnsupportedSourceShape);
}

#[test]
fn function_free_core_omits_the_new_table() {
    let core = compile(JIM_BASELINE);
    let CanonicalValue::Map(fields) = canonical(&core) else {
        panic!("module map")
    };
    assert!(!fields
        .iter()
        .any(|(key, _)| key == &CanonicalValue::Text("functions".into())));
}

#[test]
fn boolean_functions_compose_as_values_and_lazy_predicates() {
    let authored = target_source(
        "fn isZero(value: U64) -> Bool { return value == 0u64; }\nfn choose(flag: Bool, value: U64) -> U64 { return if flag then value else 0u64; }\nfn both(left: Bool, right: Bool) -> Bool { return left && right; }",
        "choose(both(isZero(input.value), !isZero(input.other)), input.value)",
    ).replace("basis none", "basis none where isZero(input.value) || isZero(input.other)");
    let core = compile(&authored);
    assert_eq!(
        lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
    assert!(matches!(
        core.functions["both"].body.result,
        CoreExpr::If { .. }
    ));
    assert_eq!(core.intents["evaluate"].input_constraints.len(), 1);
}

#[test]
fn source_function_zero_arguments_and_nominal_records_preserve_types() {
    let authored = target_source(
        "type Boxed = { value: U64, };\nfn zero() -> U64 { return 0u64; }\nfn boxed(value: U64) -> Boxed { return { value: value }; }\nfn unbox(value: Boxed) -> U64 { return value.value; }",
        "unbox(boxed(zero()))",
    );
    let core = compile(&authored);
    assert_eq!(core.functions["zero"].params.len(), 0);
    assert_eq!(
        lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
}

#[test]
fn independent_target_checks_function_results_calls_and_totality() {
    let original = compile(&target_source(
        "fn retain(value: U64) -> U64 { let copy = value; return copy; }",
        "retain(input.value)",
    ));
    let mut mutations = Vec::new();
    let mut wrong_binding = original.clone();
    wrong_binding
        .functions
        .get_mut("retain")
        .unwrap()
        .body
        .bindings[0]
        .value = CoreExpr::Const(CoreValue::Bool(true));
    mutations.push(wrong_binding);
    let mut wrong_result = original.clone();
    wrong_result
        .functions
        .get_mut("retain")
        .unwrap()
        .body
        .result = CoreExpr::Const(CoreValue::Bool(true));
    mutations.push(wrong_result);
    let mut unknown_authority = original.clone();
    unknown_authority
        .functions
        .get_mut("retain")
        .unwrap()
        .body
        .result = CoreExpr::Call {
        callee: "foreign.pack@1.retain".into(),
        type_args: vec![],
        args: vec![],
    };
    mutations.push(unknown_authority);
    let mut wrong_argument = original.clone();
    let CoreExpr::Record { fields } = &mut wrong_argument
        .intents
        .get_mut("evaluate")
        .unwrap()
        .body
        .result
    else {
        panic!("record")
    };
    let CoreExpr::Call { args, .. } = fields.get_mut("value").unwrap() else {
        panic!("call")
    };
    args[0] = CoreExpr::Const(CoreValue::Bool(true));
    mutations.push(wrong_argument);
    let mut partial = original.clone();
    let function = partial.functions.get_mut("retain").unwrap();
    function.body.result = CoreExpr::Call {
        callee: "core.integer.subtract".into(),
        type_args: vec!["U64".into()],
        args: vec![
            CoreExpr::Const(CoreValue::Int {
                width: "U64".into(),
                value: "0".into(),
            }),
            CoreExpr::Local {
                reference: function.params[0].clone(),
            },
        ],
    };
    mutations.push(partial);
    for changed in mutations {
        let report = lower_to_target_ir(&changed, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:?}"
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "{report:?}"
        );
        assert!(report.artifact.is_none());
    }
}

#[test]
fn public_core_function_scope_and_cycle_checks_do_not_trust_the_compiler() {
    let original = compile(&target_source(
        "fn retain(value: U64) -> U64 { let copy = value; return copy; }",
        "retain(input.value)",
    ));
    let mut captured = original.clone();
    let function = captured.functions.get_mut("retain").unwrap();
    let mut forged = function.params[0].clone();
    forged.id = "caller-only".into();
    function.body.result = CoreExpr::Local { reference: forged };
    let mut forward = original.clone();
    let function = forward.functions.get_mut("retain").unwrap();
    function.body.bindings[0].value = CoreExpr::Local {
        reference: function.body.locals[0].clone(),
    };
    for changed in [captured, forward] {
        let failure = validate_core_module_type_integrity(&changed).unwrap_err();
        assert_eq!(
            failure.kind(),
            CoreTypeIntegrityFailureKind::InvalidDefinition
        );
        assert!(encode_core_module(&changed).is_err());
    }
    let mut recursive = original;
    let function = recursive.functions.get_mut("retain").unwrap();
    function.body.result = CoreExpr::Call {
        callee: "functions.example@1.retain".into(),
        type_args: vec![],
        args: vec![CoreExpr::Local {
            reference: function.params[0].clone(),
        }],
    };
    assert_eq!(
        validate_core_module_type_integrity(&recursive)
            .unwrap_err()
            .kind(),
        CoreTypeIntegrityFailureKind::ReferenceCycle
    );
}

#[test]
fn source_functions_charge_all_imported_cost_dimensions_and_own_storage() {
    let functions = format!("{IMPORT}\nfn costly(value: U64) -> U64 {{ return helpers.bump(value); }}\nfn twice(value: U64) -> U64 {{ let first = costly(value); return costly(first); }}");
    let parsed = parse_module(&source(&functions, "twice(input.value)")).unwrap();
    for dimension in ["allocated", "output"] {
        let mut context = helper_context(0, u64::MAX);
        let lawpack = ResourceRef {
            coordinate: "example.bounds@1".into(),
            digest: Some(format!("sha256:{}", "1".repeat(64))),
        };
        context = context.with_pure_helper_cost(
            "helpers.cost",
            PureHelperCostFact {
                lawpack,
                coordinate: "example.bounds@1.cost".into(),
                budget: CoreBudget {
                    max_steps: 0,
                    max_allocated_bytes: if dimension == "allocated" {
                        u64::MAX / 2 + 1
                    } else {
                        0
                    },
                    max_output_bytes: if dimension == "output" {
                        u64::MAX / 2 + 1
                    } else {
                        0
                    },
                },
            },
        );
        let errors = compile_to_core(&parsed, &context).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::InvalidBound),
            "{dimension}: {errors:?}"
        );
    }
    let parsed = parse_module(&source(
        "fn retain(value: U64) -> U64 { return value; }",
        "retain(input.value)",
    ))
    .unwrap();
    let constrained = context(4096).with_budget(
        "p.small",
        CoreBudget {
            max_steps: 4096,
            max_allocated_bytes: 1,
            max_output_bytes: u64::MAX,
        },
    );
    assert!(compile_to_core(&parsed, &constrained)
        .unwrap_err()
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}

#[test]
fn source_function_byte_comparison_work_is_in_the_static_budget() {
    let authored = source(
        "fn same(left: Bytes<max=1024>, right: Bytes<max=1024>) -> U64 { return if left == right then 0u64 else 1u64; }",
        "same(input.value, input.other)",
    ).replace("type Input = { value: U64, other: U64, };", "type Input = { value: Bytes<max=1024>, other: Bytes<max=1024>, };");
    let parsed = parse_module(&authored).unwrap();
    let errors = compile_to_core(&parsed, &context(100)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    compile_with(&authored, &context(4096));
}

#[test]
fn repeated_source_diamonds_have_checked_cost_without_expansion() {
    let mut functions = "fn leaf(value: U64) -> U64 { return value; }\n".to_owned();
    let mut previous = "leaf".to_owned();
    for index in 0..64 {
        let name = format!("diamond{index}");
        writeln!(functions, "fn {name}(value: U64) -> U64 {{ let first = {previous}(value); return {previous}(first); }}").unwrap();
        previous = name;
    }
    let parsed = parse_module(&source(&functions, "diamond63(input.value)")).unwrap();
    let errors = compile_to_core(&parsed, &context(u64::MAX)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
}

#[test]
fn source_function_imported_calls_require_owned_cost_evidence_even_if_unused() {
    let functions =
        format!("{IMPORT}\nfn costly(value: U64) -> U64 {{ return helpers.bump(value); }}");
    let parsed = parse_module(&source(&functions, "input.value")).unwrap();
    let missing_fact = compile_to_core(&parsed, &context(4096)).unwrap_err();
    assert!(missing_fact
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedFunction));
    let lawpack = ResourceRef {
        coordinate: "example.bounds@1".into(),
        digest: Some(format!("sha256:{}", "1".repeat(64))),
    };
    let without_cost = context(4096).with_pure_function(
        "helpers.bump",
        PureFunctionFact {
            lawpack,
            coordinate: "example.bounds@1.bump".into(),
            type_parameters: vec![],
            parameter_types: vec!["U64".into()],
            return_type: "U64".into(),
            cost_template: "example.bounds@1.cost".into(),
        },
    );
    let missing_cost = compile_to_core(&parsed, &without_cost).unwrap_err();
    assert!(
        missing_cost
            .iter()
            .any(|error| error.kind == CompilerErrorKind::MissingContextFact),
        "{missing_cost:?}"
    );
}

#[test]
fn require_only_source_function_program_binds_the_complete_core_identity() {
    let authored = source(
        "fn retain(value: U64) -> U64 { return value; }",
        "input.value",
    )
    .replace(
        "\nreturn { value:",
        "\nrequire retain(input.value) == input.value else domain.Refused;\nreturn { value:",
    );
    let mut target = facts();
    target.obstruction_coordinates.push("domain.Refused".into());
    let core = compile(&authored);
    assert_eq!(core.imports, Vec::<edict_syntax::CoreImport>::new());
    assert!(core.intents["evaluate"].basis.is_none());
    assert!(core.intents["evaluate"]
        .body
        .nodes
        .iter()
        .all(|node| matches!(node, CoreNode::Require { .. })));
    let report = lower_to_target_ir(&core, &target);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let closure = report
        .artifact
        .unwrap()
        .semantic_closure
        .expect("source-owned code always binds Core identity");
    assert_eq!(
        closure.source_core.digest,
        Some(digest_core_module(&core).unwrap().to_review_string())
    );
}

#[test]
fn source_function_costs_do_not_depend_on_diagnostic_spans() {
    use edict_syntax::ast::{Decl, Expr, Stmt};
    let authored = source(
        "fn retain(bytes: Bytes<max=1024>) -> Bytes<max=1024> { let copy = bytes; return copy; }\nfn unused(value: U64) -> U64 { return value; }",
        "len(retain(input.value))",
    ).replace("type Input = { value: U64, other: U64, };", "type Input = { value: Bytes<max=1024>, other: U64, };");
    let parsed = parse_module(&authored).unwrap();
    let constrained = context(4096).with_budget(
        "p.small",
        CoreBudget {
            max_steps: 4096,
            max_allocated_bytes: 6500,
            max_output_bytes: 4096,
        },
    );
    let original = compile_to_core(&parsed, &constrained).unwrap_err();
    assert!(original
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
    let mut changed = parsed;
    let mut shared = None;
    for declaration in &changed.decls {
        if let Decl::Function(function) = declaration {
            if function.name == "unused" {
                let Stmt::Return {
                    value: Expr::Ident { span, .. },
                    ..
                } = &function.body.stmts[0]
                else {
                    panic!("scalar return")
                };
                shared = Some(*span);
            }
        }
    }
    for declaration in &mut changed.decls {
        if let Decl::Function(function) = declaration {
            if function.name == "retain" {
                for statement in &mut function.body.stmts {
                    let (Stmt::Let {
                        value: Expr::Ident { span, .. },
                        ..
                    }
                    | Stmt::Return {
                        value: Expr::Ident { span, .. },
                        ..
                    }) = statement
                    else {
                        panic!("simple binding or return")
                    };
                    *span = shared.unwrap();
                }
            }
        }
    }
    let changed_errors = compile_to_core(&changed, &constrained).unwrap_err();
    assert!(changed_errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}

#[test]
fn source_function_costs_include_original_branch_yield_bindings() {
    let mut bindings = String::new();
    for index in 0..8 {
        let value = if index == 0 {
            "input.value".to_owned()
        } else {
            format!("copy{}", index - 1)
        };
        write!(bindings, "let copy{index} = {value};").unwrap();
    }
    let authored = source("fn retain(value: U64) -> U64 { return value; }", "input.other")
        .replace("type Input = { value: U64, other: U64, };", "type Input = { value: Bytes<max=1024>, other: U64, };")
        .replace("return { value: input.other };", &format!("let selected = if input.other == 0u64 {{ {bindings} yield copy7; }} else {{ yield input.value; }}; return {{ value: retain(len(selected)) }};"));
    let parsed = parse_module(&authored).unwrap();
    let constrained = context(4096).with_budget(
        "p.small",
        CoreBudget {
            max_steps: 4096,
            max_allocated_bytes: 8000,
            max_output_bytes: 4096,
        },
    );
    assert!(compile_to_core(&parsed, &constrained)
        .unwrap_err()
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
    compile(&authored);
}

#[test]
fn independent_target_rejects_even_unused_source_imported_authority_collisions() {
    let bundle = edict_syntax::decode_lawpack_bundle(
        include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor"),
        include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor"),
    )
    .unwrap();
    let adapter = edict_syntax::decode_lawpack_adapter(
        &bundle,
        "echo.dpo@1",
        include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor"),
    )
    .unwrap();
    let authored = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict")
        .replace("package examples.hello_echo@1;", "package hello.echo@1;")
        .replace("intent createGreeting(", "intent invokeGreeting(");
    let module = parse_module(&authored).unwrap();
    let preparation =
        edict_syntax::prepare_lawpack_compilation(&module, &bundle, &adapter).unwrap();
    let mut core = compile_to_core(&module, preparation.compiler_context()).unwrap();
    let control = lower_to_target_ir(&core, preparation.target_ir_facts());
    assert_eq!(control.status, TargetLoweringStatus::Lowered, "{control:?}");
    let owned = compile(&source(
        "fn createGreeting(value: U64) -> U64 { return value; }",
        "input.value",
    ));
    core.functions = owned.functions;
    validate_core_module_type_integrity(&core).unwrap();
    let report = lower_to_target_ir(&core, preparation.target_ir_facts());
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:?}"
    );
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
    assert!(report.artifact.is_none());
}

#[test]
fn boolean_predicate_reification_storage_is_budgeted() {
    let predicates = ["flag"; 16].join(" && ");
    let authored = format!("package functions.example@1; fn every(flag: Bool) -> Bool {{ return {predicates}; }} type Input = {{ value: U64, other: U64, }}; intent evaluate(input: Input) returns Bool profile p.read basis none budget <= p.small {{ return every(true); }}");
    let parsed = parse_module(&authored).unwrap();
    // Sixteen reached Bool-value predicates require sixteen synthetic true
    // operands in addition to their local reads and the reified result.
    let constrained = context(4096).with_budget(
        "p.small",
        CoreBudget {
            max_steps: 4096,
            max_allocated_bytes: 2000,
            max_output_bytes: 64,
        },
    );
    let errors = compile_to_core(&parsed, &constrained).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound),
        "{errors:?}"
    );
    compile(&authored);
}

#[test]
fn predicate_operand_diagnostics_are_independent_of_call_syntax() {
    let functions = format!("{IMPORT}\nfn retain(value: U64) -> U64 {{ return value; }}\nfn yes(value: U64) -> Bool {{ return value == value; }}");
    for predicate in [
        "yes(input.value)",
        "if true then yes(input.value) else false",
    ] {
        let authored = source(&functions, "input.value")
            .replace("basis none", &format!("basis none where {predicate}"));
        compile_with(&authored, &helper_context(1, 4096));
    }
    for predicate in [
        "input.value",
        "retain(input.value)",
        "helpers.bump(input.value)",
    ] {
        let authored = source(&functions, "input.value")
            .replace("basis none", &format!("basis none where {predicate}"));
        let start = authored.find(&format!("where {predicate}")).unwrap() + "where ".len();
        let errors = compile_to_core(&parse_module(&authored).unwrap(), &helper_context(1, 4096))
            .unwrap_err();
        assert_eq!(errors.len(), 1, "{predicate}: {errors:?}");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::ExpectedPredicate,
            "{predicate}: {errors:?}"
        );
        assert_eq!(
            errors[0].span,
            edict_syntax::Span::new(start, start + predicate.len())
        );
    }
    let malformed =
        source(&functions, "input.value").replace("basis none", "basis none where retain(true)");
    let errors =
        compile_to_core(&parse_module(&malformed).unwrap(), &helper_context(1, 4096)).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].kind,
        CompilerErrorKind::TypeMismatch,
        "an invalid argument must retain its intrinsic typing diagnostic: {errors:?}"
    );
}

#[test]
fn typed_non_boolean_conditional_predicate_reports_expected_predicate() {
    let functions = "fn retain(value: U64) -> U64 { return value; }";
    let predicate = "if true then retain(input.value) else input.other";
    let authored = source(functions, "input.value")
        .replace("basis none", &format!("basis none where {predicate}"));
    let start = authored.find(&format!("where {predicate}")).unwrap() + "where ".len();
    let errors = compile_to_core(&parse_module(&authored).unwrap(), &context(4096)).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].kind,
        CompilerErrorKind::ExpectedPredicate,
        "{errors:?}"
    );
    assert_eq!(
        errors[0].span,
        edict_syntax::Span::new(start, start + predicate.len())
    );
}

#[test]
fn rejected_function_statements_identify_the_offending_statement_span() {
    for (body, offending) in [
        (
            "assert value == value; return value;",
            "assert value == value;",
        ),
        ("return value; let copy = value;", "let copy = value;"),
        ("target.read(value); return value;", "target.read(value);"),
    ] {
        let authored = source(
            &format!("fn hidden(value: U64) -> U64 {{ {body} }}"),
            "input.value",
        );
        let start = authored.find(offending).unwrap();
        let errors =
            compile_to_core(&parse_module(&authored).unwrap(), &context(4096)).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::UnsupportedSourceShape,
            "{errors:?}"
        );
        assert_eq!(
            errors[0].span,
            edict_syntax::Span::new(start, start + offending.len()),
            "the function declaration is not the offending statement: {errors:?}"
        );
    }
}

#[test]
fn graph_diagnostic_origin_distinguishes_global_work_from_a_function_named_work() {
    let authored = source(
        "fn work(value: U64) -> U64 { return work(value); }",
        "input.value",
    );
    let parsed = parse_module(&authored).unwrap();
    let edict_syntax::ast::Decl::Function(function) = &parsed.decls[0] else {
        panic!("function")
    };
    let errors = compile_to_core(&parsed, &context(4096)).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, CompilerErrorKind::UnsupportedSourceShape);
    assert_eq!(
        errors[0].span, function.span,
        "a real fn work owns its recursion failure"
    );

    let mut functions = String::new();
    for index in 0..128 {
        let next = if index == 127 {
            "work".to_owned()
        } else {
            format!("f{:03}", index + 1)
        };
        writeln!(
            functions,
            "fn f{index:03}(value: U64) -> U64 {{ return {next}(value); }}"
        )
        .unwrap();
    }
    functions.push_str("fn work(value: U64) -> U64 { return value; }");
    let authored = source(&functions, "input.value");
    let parsed = parse_module(&authored).unwrap();
    let function = parsed
        .decls
        .iter()
        .find_map(|declaration| match declaration {
            edict_syntax::ast::Decl::Function(function) if function.name == "work" => {
                Some(function)
            }
            _ => None,
        })
        .unwrap();
    let errors = compile_to_core(&parsed, &context(4096)).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, CompilerErrorKind::InvalidBound);
    assert_eq!(
        errors[0].span, function.span,
        "a real fn work owns its call-depth failure"
    );

    let mut functions =
        "fn work(value: U64) -> U64 { return value; }\nfn huge(value: U64) -> U64 {\n".to_owned();
    for index in 0..65_536 {
        writeln!(functions, "let copy{index} = value;").unwrap();
    }
    functions.push_str("return value; }");
    let authored = source(&functions, "input.value");
    let errors =
        compile_to_core(&parse_module(&authored).unwrap(), &context(u64::MAX)).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, CompilerErrorKind::InvalidBound);
    assert_eq!(
        errors[0].span,
        edict_syntax::Span::new(0, 0),
        "module-wide work exhaustion must not blame an unrelated fn work: {errors:?}"
    );
}

#[test]
fn graph_expression_depth_diagnostic_identifies_its_owning_function() {
    // This deep-source diagnostic witness needs more than Cargo's default
    // test-thread stack. It makes no default-stack or CLI resource claim.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let nested = format!("{}value{}", "expression(".repeat(129), ")".repeat(129));
            let functions = format!("fn expression(value: U64) -> U64 {{ return value; }}\nfn nested(value: U64) -> U64 {{ return {nested}; }}");
            let authored = source(&functions, "input.value");
            let parsed = parse_module(&authored).unwrap();
            let function = parsed.decls.iter().find_map(|declaration| match declaration {
                edict_syntax::ast::Decl::Function(function) if function.name == "nested" => Some(function),
                _ => None,
            }).unwrap();
            let errors = compile_to_core(&parsed, &context(4096)).unwrap_err();
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert_eq!(errors[0].kind, CompilerErrorKind::InvalidBound);
            assert_eq!(errors[0].span, function.span,
                "an expression-depth sentinel is not a function identifier: {errors:?}");
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn request_value_shapes_are_unsupported_in_source_accounting_not_overflow() {
    for (functions, rejects_return_operand) in [
        ("fn hidden(value: ExternalActionRequest<U64>) -> U64 { return 0u64; }", false),
        ("fn hidden(value: ExternalActionRequest<U64>) -> ExternalActionRequest<U64> { return value; }", true),
        ("fn hidden(value: List<ExternalActionRequest<U64>, max=0>) -> U64 { return 0u64; }", false),
        ("type Wrapped = { request: ExternalActionRequest<U64> }; fn hidden(value: Wrapped) -> U64 { return 0u64; }", false),
    ] {
        let authored = source(functions, "input.value");
        let parsed = parse_module(&authored).unwrap();
        let function = parsed.decls.iter().find_map(|declaration| match declaration {
            edict_syntax::ast::Decl::Function(function) if function.name == "hidden" => Some(function),
            _ => None,
        }).expect("hidden function in fixture");
        let expected_span = if rejects_return_operand {
            // Expression accounting rejects the request identity before
            // signature accounting reaches the declaration's bounds.
            let [edict_syntax::ast::Stmt::Return {
                value: edict_syntax::ast::Expr::Ident { span, .. },
                ..
            }] = function.body.stmts.as_slice() else {
                panic!("request identity must return its parameter")
            };
            *span
        } else {
            function.span
        };
        let errors = compile_to_core(&parsed, &context(4096)).unwrap_err();
        assert_eq!(errors.len(), 1, "{functions}: {errors:?}");
        assert_eq!(errors[0].kind, CompilerErrorKind::UnsupportedSourceShape,
            "unsupported request-containing shapes must not claim arithmetic overflow: {errors:?}");
        assert_eq!(errors[0].span, expected_span,
            "the diagnostic must identify the rejected source boundary: {errors:?}");
    }
    for (input, output, expression) in [
        ("ExternalActionRequest<U64>", "U64", "0u64"),
        (
            "ExternalActionRequest<U64>",
            "ExternalActionRequest<U64>",
            "input",
        ),
        ("List<ExternalActionRequest<U64>, max=0>", "U64", "0u64"),
    ] {
        let baseline = format!("package functions.example@1;\nintent evaluate(input: {input}) returns {output} profile p.read basis none budget <= p.small {{ return {expression}; }}");
        compile_with(&baseline, &context(4096));
        let authored = baseline.replace(
            "intent evaluate",
            "fn unused(value: U64) -> U64 { return value; }\nintent evaluate",
        );
        let errors =
            compile_to_core(&parse_module(&authored).unwrap(), &context(4096)).unwrap_err();
        assert_eq!(errors.len(), 1, "{authored}: {errors:?}");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::UnsupportedSourceShape,
            "{errors:?}"
        );
    }
    let overflow = source(
        "fn huge(value: String<max=18446744073709551615>) -> U64 { return 0u64; }",
        "input.value",
    );
    let errors = compile_to_core(&parse_module(&overflow).unwrap(), &context(4096)).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].kind,
        CompilerErrorKind::InvalidBound,
        "genuine numeric overflow retains its stable kind: {errors:?}"
    );
}
