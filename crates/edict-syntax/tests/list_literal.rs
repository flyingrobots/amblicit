//! Bounded list literals: `[a, b]` checked against an expected `List<T, max=N>`.
use edict_syntax::{
    compile_to_core, digest_core_module, lower_to_target_ir, parse_module, CompilerContext,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode, CoreValue, ResourceRef,
    TargetIrLoweringFacts, TargetLoweringFailureKind, TargetLoweringStatus, WriteClass,
};

const EDGE: &str = "type Edge = { edgeType: Bytes<exact=32>, target: Bytes<exact=32>, };";

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: 1_000_000,
                max_output_bytes: 1_000_000,
            },
        )
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

/// An intent whose body binds `edges: <list type> = <expression>`.
fn source(list_type: &str, expression: &str) -> String {
    format!(
        "package list.example@1; {EDGE}
        type Input = {{ a: Edge, b: Edge, flag: Bool, }};
        type Output = {{ edges: {list_type}, }};
        intent build(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let edges: {list_type} = {expression};
          return {{ edges }};
        }}"
    )
}

fn compile(list_type: &str, expression: &str) -> CoreModule {
    compile_to_core(
        &parse_module(&source(list_type, expression)).unwrap(),
        &context(),
    )
    .unwrap()
}

fn first_binding(core: &CoreModule) -> &CoreExpr {
    match &core.intents["build"].body.nodes[0] {
        CoreNode::Let { value, .. } => value,
        _ => panic!("binding"),
    }
}

fn field_names(values: &[CoreExpr]) -> Vec<&str> {
    values
        .iter()
        .map(|value| match value {
            CoreExpr::Field { field, .. } => field.as_str(),
            _ => panic!("field element"),
        })
        .collect()
}

#[test]
fn list_literal_preserves_order_and_lowers_to_core_list() {
    let core = compile("List<Edge, max=2>", "[input.b, input.a]");
    let CoreExpr::List { values } = first_binding(&core) else {
        panic!("list expression")
    };
    assert_eq!(field_names(values), ["b", "a"]);
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert_eq!(
        report.artifact.unwrap().intents["build"].pure_bindings[0].value,
        *first_binding(&core)
    );
    assert_ne!(
        digest_core_module(&core).unwrap(),
        digest_core_module(&compile("List<Edge, max=2>", "[input.a, input.b]")).unwrap(),
        "element order is hash-significant"
    );
}

#[test]
fn list_literal_accepts_empty_partial_and_full_cardinality() {
    for (expression, expected) in [
        ("[]", 0),
        ("[input.a]", 1),
        ("[input.a, input.b,]", 2),
        ("[input.a, input.a]", 2),
    ] {
        let core = compile("List<Edge, max=2>", expression);
        let CoreExpr::List { values } = first_binding(&core) else {
            panic!("list expression")
        };
        assert_eq!(values.len(), expected, "{expression}");
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{expression}");
    }
}

#[test]
fn list_literal_elements_are_checked_against_the_item_type() {
    let core = compile("List<U64, max=3>", "[1u64, 2u64, 3u64]");
    let CoreExpr::List { values } = first_binding(&core) else {
        panic!("list expression")
    };
    assert!(values.iter().all(|value| matches!(
        value,
        CoreExpr::Const(CoreValue::Int { width, .. }) if width == "U64"
    )));
    let core = compile("List<U64, max=1>", "[7]");
    assert!(matches!(
        first_binding(&core),
        CoreExpr::List { values } if matches!(
            values.as_slice(),
            [CoreExpr::Const(CoreValue::Int { width, value })] if width == "U64" && value == "7"
        )
    ));
}

#[test]
fn list_literal_rejects_excess_cardinality_wrong_items_and_missing_context() {
    for (list_type, expression, expected) in [
        (
            "List<Edge, max=1>",
            "[input.a, input.b]",
            CompilerErrorKind::InvalidBound,
        ),
        (
            "List<Edge, max=0>",
            "[input.a]",
            CompilerErrorKind::InvalidBound,
        ),
        (
            "List<Edge, max=2>",
            "[input.a, input.flag]",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "List<U64, max=2>",
            "[1u32]",
            CompilerErrorKind::TypeMismatch,
        ),
        ("Edge", "[input.a]", CompilerErrorKind::TypeMismatch),
    ] {
        let errors = compile_to_core(
            &parse_module(&source(list_type, expression)).unwrap(),
            &context(),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|e| e.kind == expected),
            "{list_type} {expression}: {errors:?}"
        );
    }
    let unannotated = source("List<Edge, max=2>", "[input.a]").replace(
        "let edges: List<Edge, max=2> = [input.a];",
        "let edges = [input.a];",
    );
    let errors = compile_to_core(&parse_module(&unannotated).unwrap(), &context()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == CompilerErrorKind::UnsupportedSourceShape),
        "{errors:?}"
    );
}

#[test]
fn list_literals_flow_into_records_returns_and_loops() {
    let text = format!(
        "package list.flow@1; {EDGE}
        type Input = {{ a: Edge, }};
        type Holder = {{ edges: List<Edge, max=2>, }};
        intent build(input: Input) returns Holder
          profile p.read basis none budget <= p.small {{
          for edge in [input.a] bounded 1 {{ let seen = edge; }}
          return {{ edges: [input.a, input.a] }};
        }}"
    );
    let errors = compile_to_core(&parse_module(&text).unwrap(), &context());
    // A bare loop iterator has no expected list type, so it needs an annotation.
    assert!(errors.is_err());
    let text = text.replace(
        "for edge in [input.a] bounded 1 { let seen = edge; }",
        "let pending: List<Edge, max=1> = [input.a]; for edge in pending bounded 1 { let seen = edge; }",
    );
    let core = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap();
    assert!(matches!(
        &core.intents["build"].body.nodes[0],
        CoreNode::Let { value: CoreExpr::List { values }, .. } if values.len() == 1
    ));
    // Target lowering of loops is separate work; records and returns lower.
    let returned = text.replace(
        "let pending: List<Edge, max=1> = [input.a]; for edge in pending bounded 1 { let seen = edge; }",
        "let kept = input.a;",
    );
    let core = compile_to_core(&parse_module(&returned).unwrap(), &context()).unwrap();
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn source_functions_build_list_literals() {
    let text = format!(
        "package list.helper@1; {EDGE}
        fn pair(left: Edge, right: Edge) -> List<Edge, max=2> {{ return [left, right]; }}
        type Input = {{ a: Edge, b: Edge, }};
        type Output = {{ edges: List<Edge, max=2>, }};
        intent build(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let edges = pair(input.a, input.b);
          return {{ edges }};
        }}"
    );
    let core = compile_to_core(&parse_module(&text).unwrap(), &context()).unwrap();
    assert!(matches!(
        &core.functions["pair"].body.result,
        CoreExpr::List { values } if values.len() == 2
    ));
    let report = lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn authenticated_effects_are_rejected_inside_list_literals() {
    // `target.replace` is a registered write effect under a profile that
    // allows it, and compiles in A-normal position. Inside a list literal it is
    // refused exactly as inside a record literal: as an unresolved pure call.
    let context = CompilerContext::new()
        .with_operation_profile("p.effectful", "continuum.profile.write/v1")
        .with_operation_profile_write_classes("p.effectful", [WriteClass::Replace])
        .with_effect_write_class("target.replace", WriteClass::Replace)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 1_000_000,
                max_allocated_bytes: 1_000_000,
                max_output_bytes: 1_000_000,
            },
        );
    let effectful = |ty: &str, expression: &str| {
        format!(
            "package a.b@1; type Input = {{ id: String<max=16>, }};
            type Receipt = {{ id: String<max=16>, }};
            type Holder = {{ receipt: Receipt, }};
            type Output = {{ id: String<max=16>, }};
            intent t(input: Input) returns Output profile p.effectful basis none budget <= p.tiny {{
              let value: {ty} = {expression};
              return {{ id: input.id }};
            }}"
        )
    };
    let kinds = |text: String| {
        compile_to_core(&parse_module(&text).unwrap(), &context)
            .err()
            .map(|errors| errors.iter().map(|error| error.kind).collect::<Vec<_>>())
    };
    assert_eq!(
        kinds(effectful(
            "Receipt",
            "target.replace(input.id) else { rejected(reason) => domain.WriteRejected }",
        )),
        None,
        "the effect itself is authenticated"
    );
    let nested = kinds(effectful(
        "List<Receipt, max=1>",
        "[target.replace(input.id)]",
    ));
    assert_eq!(
        nested,
        Some(vec![CompilerErrorKind::UnresolvedFunction]),
        "an effect inside a list literal is refused"
    );
    assert_eq!(
        nested,
        kinds(effectful("Holder", "{ receipt: target.replace(input.id) }")),
        "list literals refuse effects exactly as record literals do"
    );
}

#[test]
fn target_refuses_forged_list_literals() {
    for case in 0..4 {
        let mut core = compile("List<Edge, max=2>", "[input.a, input.b]");
        let CoreNode::Let { binding, value } =
            &mut core.intents.get_mut("build").unwrap().body.nodes[0]
        else {
            panic!("binding")
        };
        let CoreExpr::List { values } = value else {
            panic!("list")
        };
        match case {
            // More elements than the declared maximum.
            0 => values.push(values[0].clone()),
            // An element of the wrong type.
            1 => {
                values[1] = CoreExpr::Const(CoreValue::Int {
                    width: "U64".into(),
                    value: "1".into(),
                });
            }
            // An element naming a local that is not in scope.
            2 => {
                let CoreExpr::Field { base, .. } = &mut values[0] else {
                    panic!("field")
                };
                let CoreExpr::Local { reference } = base.as_mut() else {
                    panic!("local")
                };
                reference.id = "foreign-input".into();
            }
            // A binding that claims a non-list type.
            3 => binding.ty = "U64".into(),
            _ => unreachable!(),
        }
        let report = lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "case {case}"
        );
        assert!(report.artifact.is_none(), "case {case}");
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.kind == TargetLoweringFailureKind::InvalidCoreIdentity),
            "case {case}: {:?}",
            report.failures
        );
    }
}

#[test]
fn a_list_literal_under_a_failed_call_reports_only_independent_errors() {
    // The unknown helper already fails, so the literal's missing expected type
    // is a consequence, not a second error. Its elements are still checked.
    let text = source("List<Edge, max=2>", "[input.a]").replace(
        "let edges: List<Edge, max=2> = [input.a];",
        "let edges: List<Edge, max=2> = missing.helper([input.a]);",
    );
    let kinds = |text: &str| {
        compile_to_core(&parse_module(text).unwrap(), &context())
            .unwrap_err()
            .iter()
            .map(|error| error.kind)
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds(&text), [CompilerErrorKind::UnresolvedFunction]);
    let element_error = text.replace("[input.a]", "[input.nope]");
    let element_kinds = kinds(&element_error);
    assert!(
        element_kinds.contains(&CompilerErrorKind::UnresolvedFunction) && element_kinds.len() == 2,
        "the unknown element field is still diagnosed: {element_kinds:?}"
    );
}
