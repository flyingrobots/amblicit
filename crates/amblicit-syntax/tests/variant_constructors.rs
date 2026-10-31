//! Authored variant construction follows the existing Core expression contract.
use amblicit_syntax::{
    compile_to_core, decode_canonical_cbor, encode_core_module, parse_module, CanonicalValue as V,
    CompilerContext, CompilerErrorKind, CoreModule,
};

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            amblicit_syntax::CoreBudget {
                max_steps: 65536,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 65536,
            },
        )
}
fn facts() -> amblicit_syntax::TargetIrLoweringFacts {
    amblicit_syntax::TargetIrLoweringFacts {
        target_profile: amblicit_syntax::ResourceRef {
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
fn operation(function: &str, call: &str, input: &str) -> CoreModule {
    let source=format!("package choices.example@1; type Choice = variant {{ Empty, Data(U8) }}; type Other = variant {{ Empty, Data(U8) }}; {function} type Input = {{ marker: U8, {input} }}; type Output = {{ value: U8, }}; intent run(input: Input) returns Output profile p.read basis none budget <= p.small {{ let constructed: Choice = {call}; return {{ value: input.marker }}; }}");
    compile_to_core(&parse_module(&source).unwrap(), &context())
        .expect("operation with a checked constructor")
}

fn compile(declarations: &str) -> CoreModule {
    compile_to_core(
        &parse_module(&format!("package choices.example@1; {declarations}")).unwrap(),
        &CompilerContext::new(),
    )
    .expect("checked variant construction")
}
fn field<'a>(value: &'a V, key: &str) -> &'a V {
    let V::Map(fields) = value else { panic!("map") };
    fields
        .iter()
        .find_map(|(name, value)| (name == &V::Text(key.into())).then_some(value))
        .expect("field")
}
fn result(core: &CoreModule, function: &str) -> V {
    let encoded = decode_canonical_cbor(&encode_core_module(core).unwrap()).unwrap();
    field(field(field(&encoded, "functions"), function), "body").clone()
}

#[test]
fn constructors_preserve_type_case_payload_and_omitted_payload_in_canonical_core() {
    let core = compile("type Choice = variant { Empty, Data(U8) }; fn make(value: U8) -> Choice { return Choice::Data(value); } fn empty() -> Choice { return Choice::Empty; }");
    let make = result(&core, "make");
    let expression = field(&make, "result");
    assert_eq!(field(expression, "kind"), &V::Text("variant".into()));
    assert_eq!(
        field(expression, "type"),
        &V::Text("choices.example@1.Choice".into())
    );
    assert_eq!(field(expression, "case"), &V::Text("Data".into()));
    assert_eq!(
        field(field(expression, "payload"), "kind"),
        &V::Text("local".into())
    );
    let empty = result(&core, "empty");
    let V::Map(fields) = field(&empty, "result") else {
        panic!("map")
    };
    assert!(!fields
        .iter()
        .any(|(name, _)| name == &V::Text("payload".into())));
}

#[test]
fn constructors_context_check_case_payloads_and_bounds() {
    for (declarations, kind) in [
        ("type Choice = variant { Data(U8) }; fn make() -> Choice { return Choice::Missing; }", CompilerErrorKind::UnknownField),
        ("type Choice = variant { Data(U8) }; fn make() -> Choice { return Choice::Data; }", CompilerErrorKind::TypeMismatch),
        ("type Choice = variant { Empty }; fn make() -> Choice { return Choice::Empty(0u8); }", CompilerErrorKind::TypeMismatch),
        ("type Choice = variant { Data(U8) }; fn make() -> Choice { return Choice::Data(true); }", CompilerErrorKind::TypeMismatch),
        ("type Choice = variant { Data(U8) }; fn make() -> Choice { return Choice::Data(256); }", CompilerErrorKind::TypeMismatch),
        ("type Choice = variant { Data(U8) }; fn make() -> Choice { return Choice::Data(0u64); }", CompilerErrorKind::TypeMismatch),
        ("type Choice = variant { Data(Bytes<max=1>) }; fn make() -> Choice { return Choice::Data(b\"AB\"); }", CompilerErrorKind::TypeMismatch),
        ("type Choice = { value: U8, }; fn make() -> Choice { return Choice::Data(0u8); }", CompilerErrorKind::TypeMismatch),
    ] {
        let source=format!("package choices.example@1; {declarations}");
        let errors=compile_to_core(&parse_module(&source).unwrap(),&CompilerContext::new()).unwrap_err();
        assert!(errors.iter().any(|error|error.kind==kind),"{errors:?}");
    }
}

#[test]
fn nested_conditional_and_aliased_constructors_keep_their_declared_types() {
    let core = compile("type Inner = variant { Empty, Data(U64) }; type Alias = Inner; type Outer = variant { Item(Inner) }; fn make(flag: Bool) -> Outer { return Outer::Item(if flag then Alias::Empty else Inner::Data(4)); }");
    let make = result(&core, "make");
    let payload = field(field(&make, "result"), "payload");
    assert_eq!(field(payload, "kind"), &V::Text("if".into()));
    for arm in ["then", "else"] {
        assert_eq!(
            field(field(payload, arm), "type"),
            &V::Text("choices.example@1.Inner".into())
        );
    }
}

#[test]
fn variant_constructor_storage_bounds_fail_closed_on_overflow() {
    let source="package choices.example@1; type Choice = variant { Empty, Huge(Bytes<max=18446744073709551615>) }; fn make() -> Choice { return Choice::Empty; }";
    let errors =
        compile_to_core(&parse_module(source).unwrap(), &CompilerContext::new()).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}

#[test]
fn constructor_contextual_literals_alpha_names_and_case_identity_are_exact() {
    let source="package choices.example@1; type Choice = variant { Data(U8), Peer(U8) }; fn make(value: U8) -> Choice { return Choice::Data(value); } fn max() -> Choice { return Choice::Data(255); }";
    let core = compile_to_core(&parse_module(source).unwrap(), &CompilerContext::new()).unwrap();
    let body = result(&core, "max");
    let value = field(field(field(&body, "result"), "payload"), "value");
    assert_eq!(field(value, "width"), &V::Text("U8".into()));
    assert_eq!(field(value, "value"), &V::Integer(255));
    let renamed = source.replace("value", "renamed");
    let alpha = compile_to_core(&parse_module(&renamed).unwrap(), &CompilerContext::new()).unwrap();
    assert_eq!(
        encode_core_module(&core).unwrap(),
        encode_core_module(&alpha).unwrap()
    );
    let mut changed = core.clone();
    let amblicit_syntax::CoreExpr::Variant { case, .. } =
        &mut changed.functions.get_mut("make").unwrap().body.result
    else {
        panic!("constructor")
    };
    *case = "Peer".into();
    assert_ne!(
        amblicit_syntax::digest_core_module(&core).unwrap(),
        amblicit_syntax::digest_core_module(&changed).unwrap()
    );
}

#[test]
fn imported_constructors_retain_the_original_type_coordinate() {
    use amblicit_syntax::source_closure::{
        resolve_source_closure, source_module_digest, SourceClosureLimits,
    };
    use amblicit_syntax::source_linker::link_source_closure;
    let library="package library.choices@1; type Choice = variant { Empty, Data(U8) }; fn make(value: U8) -> Choice { return Choice::Data(value); }";
    let source=format!("package choices.example@1; use module library.choices@1 digest \"{}\" as choices; type Alias = choices.Choice;",source_module_digest(library).unwrap().to_review_string());
    let closure =
        resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
    let core = link_source_closure(&closure)
        .unwrap()
        .compile(&CompilerContext::new())
        .unwrap();
    let constructor = &core.functions.values().next().unwrap().body.result;
    assert!(
        matches!(constructor,amblicit_syntax::CoreExpr::Variant{ty,case,..}if ty=="library.choices@1.Choice"&&case=="Data")
    );
    encode_core_module(&core).unwrap();
}

#[test]
fn constructor_payloads_cannot_launder_nominal_identity_into_raw_bytes() {
    use amblicit_syntax::{ResourceRef, TypeShapeFact};
    let pin = format!("sha256:{}", "1".repeat(64));
    let context = CompilerContext::new().with_type_shape(TypeShapeFact {
        lawpack: ResourceRef {
            coordinate: "example.identities@1".into(),
            digest: Some(pin.clone()),
        },
        coordinate: "example.identities@1.Key".into(),
        definition: "Nominal<Bytes<exact=32>>".into(),
    });
    let source=format!("package choices.example@1; use lawpack example.identities@1 digest \"{pin}\" as identities; type Choice = variant {{ Key(identities.Key) }}; fn make(key: identities.Key) -> Choice {{ return Choice::Key(key); }}");
    let core = compile_to_core(&parse_module(&source).unwrap(), &context).unwrap();
    encode_core_module(&core).unwrap();
    let bad = source.replace("key: identities.Key", "key: Bytes<exact=32>");
    let errors = compile_to_core(&parse_module(&bad).unwrap(), &context).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn raw_constructor_metadata_is_checked_before_canonical_encoding() {
    use amblicit_syntax::{
        validate_core_module_type_integrity, CoreExpr, CoreTypeIntegrityFailureKind as Kind,
    };
    let original=compile("type Choice = variant { Empty, Data(U8) }; fn make(value: U8) -> Choice { return Choice::Data(value); }");
    for (ty, case, present, kind) in [
        (
            "choices.example@1.Ghost",
            "Data",
            true,
            Kind::UnresolvedNamedReference,
        ),
        ("U8", "Data", true, Kind::InvalidReference),
        (
            "choices.example@1.Choice",
            "Ghost",
            true,
            Kind::InvalidDefinition,
        ),
        (
            "choices.example@1.Choice",
            "Empty",
            true,
            Kind::InvalidDefinition,
        ),
        (
            "choices.example@1.Choice",
            "Data",
            false,
            Kind::InvalidDefinition,
        ),
    ] {
        let mut core = original.clone();
        let CoreExpr::Variant {
            ty: actual,
            case: tag,
            payload,
        } = &mut core.functions.get_mut("make").unwrap().body.result
        else {
            panic!("constructor")
        };
        *actual = ty.into();
        *tag = case.into();
        if !present {
            *payload = None;
        }
        assert_eq!(
            validate_core_module_type_integrity(&core)
                .unwrap_err()
                .kind(),
            kind
        );
        assert!(encode_core_module(&core).is_err());
    }
}

#[test]
fn target_rechecks_constructor_payload_types_and_exact_union_identity() {
    use amblicit_syntax::{CoreExpr, CoreValue, TargetLoweringStatus as Status};
    let core = operation(
        "fn make(value: U8) -> Choice { return Choice::Data(value); }",
        "make(input.value)",
        "value: U8,",
    );
    assert_eq!(
        amblicit_syntax::lower_to_target_ir(&core, &facts()).status,
        Status::Lowered
    );
    for (ty, value) in [
        ("choices.example@1.Choice", CoreValue::Bool(true)),
        (
            "choices.example@1.Choice",
            CoreValue::Int {
                width: "U64".into(),
                value: "1".into(),
            },
        ),
        (
            "choices.example@1.Other",
            CoreValue::Int {
                width: "U8".into(),
                value: "1".into(),
            },
        ),
    ] {
        let mut mutant = core.clone();
        mutant.functions.get_mut("make").unwrap().body.result = CoreExpr::Variant {
            ty: ty.into(),
            case: "Data".into(),
            payload: Some(Box::new(CoreExpr::Const(value))),
        };
        let report = amblicit_syntax::lower_to_target_ir(&mutant, &facts());
        assert_eq!(report.status, Status::Unsupported);
        assert!(report.artifact.is_none());
        assert!(report
            .failures
            .iter()
            .any(|error| error.kind
                == amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity));
    }
}

#[test]
fn target_rejects_union_identity_substitution_through_helper_calls() {
    use amblicit_syntax::{CoreExpr, TargetLoweringStatus as Status};
    let core=operation("fn make(value: U8) -> Choice { return Choice::Data(value); } fn alternate(value: U8) -> Other { return Other::Data(value); }", "make(input.value)", "value: U8,");
    assert_eq!(
        amblicit_syntax::lower_to_target_ir(&core, &facts()).status,
        Status::Lowered
    );
    let mut mutant = core;
    let parameter = mutant.functions["make"].params[0].clone();
    mutant.functions.get_mut("make").unwrap().body.result = CoreExpr::Call {
        callee: "choices.example@1.alternate".into(),
        type_args: Vec::new(),
        args: vec![CoreExpr::Local {
            reference: parameter,
        }],
    };
    let report = amblicit_syntax::lower_to_target_ir(&mutant, &facts());
    assert_eq!(report.status, Status::Unsupported, "{report:?}");
    assert!(report.artifact.is_none());
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind
            == amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity));
}

#[test]
fn variant_payloads_cannot_hide_capture_cycle_or_graph_depth() {
    use amblicit_syntax::{
        validate_core_module_type_integrity, CoreExpr, CoreTypeIntegrityFailureKind as Kind,
    };
    let original=compile("type Choice = variant { Data(U8) }; fn make(value: U8) -> Choice { return Choice::Data(value); }");
    let parameter = original.functions["make"].params[0].clone();
    let mut capture = original.clone();
    let mut forged = parameter.clone();
    forged.id = "foreign".into();
    capture.functions.get_mut("make").unwrap().body.result = CoreExpr::Variant {
        ty: "choices.example@1.Choice".into(),
        case: "Data".into(),
        payload: Some(Box::new(CoreExpr::Local { reference: forged })),
    };
    assert_eq!(
        validate_core_module_type_integrity(&capture)
            .unwrap_err()
            .kind(),
        Kind::InvalidDefinition
    );
    let mut cycle = original.clone();
    cycle.functions.get_mut("make").unwrap().body.result = CoreExpr::Variant {
        ty: "choices.example@1.Choice".into(),
        case: "Data".into(),
        payload: Some(Box::new(CoreExpr::Call {
            callee: "choices.example@1.make".into(),
            type_args: Vec::new(),
            args: vec![CoreExpr::Local {
                reference: parameter.clone(),
            }],
        })),
    };
    assert_eq!(
        validate_core_module_type_integrity(&cycle)
            .unwrap_err()
            .kind(),
        Kind::ReferenceCycle
    );
    let mut deep = original;
    let value = (0..129).fold(
        CoreExpr::Local {
            reference: parameter,
        },
        |value, _| CoreExpr::Variant {
            ty: "choices.example@1.Choice".into(),
            case: "Data".into(),
            payload: Some(Box::new(value)),
        },
    );
    deep.functions.get_mut("make").unwrap().body.result = value;
    assert_eq!(
        validate_core_module_type_integrity(&deep)
            .unwrap_err()
            .kind(),
        Kind::DepthExceeded
    );
}

#[test]
fn target_totality_checks_reach_guarded_lookups_in_variant_payloads() {
    use amblicit_syntax::{CoreExpr, CorePredicate, TargetLoweringStatus as Status};
    let core=operation("fn make(bytes: Bytes<max=8>, index: U64) -> Choice { return if index < len(bytes) then Choice::Data(at(bytes,index)) else Choice::Empty; }", "make(input.bytes,input.index)", "bytes: Bytes<max=8>, index: U64,");
    assert_eq!(
        amblicit_syntax::lower_to_target_ir(&core, &facts()).status,
        Status::Lowered
    );
    let mut mutant = core;
    let CoreExpr::If { predicate, .. } = &mut mutant.functions.get_mut("make").unwrap().body.result
    else {
        panic!("conditional")
    };
    **predicate = CorePredicate::True;
    let report = amblicit_syntax::lower_to_target_ir(&mutant, &facts());
    assert_eq!(report.status, Status::Unsupported);
    assert!(
        report
            .failures
            .iter()
            .any(|error| error.kind
                == amblicit_syntax::TargetLoweringFailureKind::InvalidCoreIdentity)
    );
    assert!(report.artifact.is_none());
}

#[test]
fn target_admits_only_the_known_variant_capability() {
    use amblicit_syntax::{TargetLoweringFailureKind, TargetLoweringStatus};
    let core = operation(
        "fn make(value: U8) -> Choice { return Choice::Data(value); }",
        "make(input.value)",
        "value: U8,",
    );
    assert_eq!(core.required_core_capabilities, vec!["core.variant/v1"]);
    let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert!(report.artifact.is_some());
    for capability in ["core.variant/v2", "core.unknown/v1"] {
        let mut mutant = core.clone();
        mutant
            .required_core_capabilities
            .push(capability.to_owned());
        let report = amblicit_syntax::lower_to_target_ir(&mutant, &facts());
        assert_eq!(report.status, TargetLoweringStatus::Unsupported);
        assert!(report.artifact.is_none());
        assert!(report.failures.iter().any(|failure| failure.kind
            == TargetLoweringFailureKind::UnsupportedCoreCapability
            && failure.detail == capability));
    }
}

#[test]
fn target_requires_variant_capability_for_helpers_and_direct_constructors() {
    use amblicit_syntax::{TargetLoweringFailureKind, TargetLoweringStatus};
    for (helper, expression) in [
        (
            "fn make(value: U8) -> Choice { return Choice::Data(value); }",
            "make(input.marker)",
        ),
        ("", "Choice::Data(input.marker)"),
    ] {
        let mut core = operation(helper, expression, "");
        assert_eq!(
            amblicit_syntax::lower_to_target_ir(&core, &facts()).status,
            TargetLoweringStatus::Lowered
        );
        core.required_core_capabilities.clear();
        let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Unsupported,
            "{report:?}"
        );
        assert!(report.artifact.is_none());
        assert!(
            report.failures.iter().any(|failure| failure.kind
                == TargetLoweringFailureKind::InvalidCoreIdentity
                && failure.detail == "required_core_capabilities.core.variant/v1"),
            "{report:?}"
        );
    }
}

fn variant_output_candidate(
    ty: &str,
    body: &str,
) -> (CoreModule, amblicit_syntax::TargetIrArtifact, String) {
    let source=format!("package choices.example@1; type Choice = variant {{ Empty, Data(U8) }}; type Envelope = {{ choice: Choice, }}; fn make(value: U8) -> {ty} {{ return {body}; }} type Input = {{ marker: U8, }}; type Output = {{ value: U8, }}; intent run(input: Input) returns Output profile p.read basis none budget <= p.small {{ let constructed: {ty} = make(input.marker); return {{ value: input.marker }}; }}");
    let mut core = compile_to_core(&parse_module(&source).unwrap(), &context()).unwrap();
    let baseline = amblicit_syntax::lower_to_target_ir(&core, &facts());
    assert_eq!(
        baseline.status,
        amblicit_syntax::TargetLoweringStatus::Lowered,
        "{baseline:?}"
    );
    assert!(baseline.result_projections.contains_key("run"));
    let result_type = core.functions["make"].return_type.clone();
    let intent = core.intents.get_mut("run").unwrap();
    let local = intent
        .body
        .locals
        .iter()
        .find(|local| local.ty == result_type)
        .unwrap()
        .clone();
    intent.output = result_type;
    intent.body.result = amblicit_syntax::CoreExpr::Local {
        reference: local.clone(),
    };
    let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
    assert_eq!(
        report.status,
        amblicit_syntax::TargetLoweringStatus::Lowered,
        "{report:?}"
    );
    let target = report.artifact.unwrap();
    let binding = target.intents["run"]
        .pure_bindings
        .iter()
        .find(|binding| binding.binding == local)
        .unwrap()
        .id
        .clone();
    (core, target, binding)
}

#[test]
fn variant_valued_result_projection_emission_refuses_direct_and_nested_values() {
    for (ty, body) in [
        ("Choice", "Choice::Data(value)"),
        ("Envelope", "{ choice: Choice::Data(value) }"),
        ("List<Choice,max=1>", "[Choice::Data(value)]"),
    ] {
        let (core, target, _) = variant_output_candidate(ty, body);
        let failure = amblicit_syntax::emit_result_projection(&core, &target, "run").unwrap_err();
        assert_eq!(
            failure.kind(),
            amblicit_syntax::ResultProjectionFailureKind::OutputShapeMismatch,
            "{ty}: {failure:?}"
        );
        let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
        assert_eq!(
            report.status,
            amblicit_syntax::TargetLoweringStatus::Lowered
        );
        assert!(!report.result_projections.contains_key("run"));
        assert_eq!(
            report.result_projection_failures["run"].kind(),
            amblicit_syntax::ResultProjectionFailureKind::OutputShapeMismatch
        );
    }
}

#[test]
fn variant_valued_result_projection_verifier_refuses_caller_encoded_candidates() {
    for (ty, body) in [
        ("Choice", "Choice::Data(value)"),
        ("Envelope", "{ choice: Choice::Data(value) }"),
        ("List<Choice,max=1>", "[Choice::Data(value)]"),
    ] {
        let (core, target, binding_id) = variant_output_candidate(ty, body);
        let candidate = amblicit_syntax::ResultProjection {
            api_version: amblicit_syntax::RESULT_PROJECTION_API_VERSION.to_owned(),
            operation_coordinate: format!("{}.run", core.coordinate),
            output_type: core.intents["run"].output.clone(),
            max_output_bytes: core.intents["run"].core_evaluation_budget.max_output_bytes,
            expression: amblicit_syntax::ResultProjectionExpr::Source {
                source: amblicit_syntax::ResultProjectionSource::PureBinding { binding_id },
                path: Vec::new(),
            },
        };
        let bytes = amblicit_syntax::encode_result_projection(&candidate).unwrap();
        let digest = amblicit_syntax::digest_result_projection(&candidate).unwrap();
        let failure =
            amblicit_syntax::verify_result_projection(&core, &target, "run", &bytes, digest)
                .unwrap_err();
        assert_eq!(
            failure.kind(),
            amblicit_syntax::ResultProjectionFailureKind::OutputShapeMismatch,
            "{ty}: {failure:?}"
        );
    }
}

#[test]
fn variant_helper_storage_obeys_exact_allocation_and_step_budgets() {
    // A call and its constructor each reserve the maximum variant value.
    // The U8 input/output frame and record return reserve 522 bytes in total.
    // Empty: 64 + 6*64 + 15 + 24 coordinate bytes + 5 label bytes = 492.
    // A Bytes<max=10> payload adds 10; a two-field payload reserves 339 bytes.
    for (extra, cases, allocated, steps) in [
        ("", "Empty", 1506, 45),
        ("", "Empty, Data(Bytes<max=10>)", 1526, 45),
        (
            "type Payload = { first: U8, last: Bytes<max=10>, };",
            "Empty, Data(Payload)",
            2056,
            61,
        ),
    ] {
        let source=format!("package choices.example@1; {extra} type Choice = variant {{ {cases} }}; fn make() -> Choice {{ return Choice::Empty; }} type Output = {{ value: U8, }}; intent run(input: U8) returns Output profile p.read basis none budget <= p.small {{ let value: Choice = make(); return {{ value: input }}; }}");
        let parsed = parse_module(&source).unwrap();
        for (max_allocated_bytes, max_steps, accepted) in [
            (allocated, steps, true),
            (allocated - 1, steps, false),
            (allocated, steps - 1, false),
        ] {
            let context = context().with_budget(
                "p.small",
                amblicit_syntax::CoreBudget {
                    max_steps,
                    max_allocated_bytes,
                    max_output_bytes: 65536,
                },
            );
            let result = compile_to_core(&parsed, &context);
            if accepted {
                result.expect("exact conservative compiler budget");
            } else {
                let errors = result.unwrap_err();
                assert_ne!(errors, [] as [amblicit_syntax::CompilerError; 0]);
                assert!(
                    errors
                        .iter()
                        .all(|error| error.kind == CompilerErrorKind::InvalidBound),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn target_rejects_distinct_variant_identity_at_the_helper_argument_site() {
    use amblicit_syntax::{CoreExpr, CoreNode, TargetLoweringFailureKind, TargetLoweringStatus};
    let mut core=operation("fn make(value: U8) -> Choice { return Choice::Data(value); } fn alternate(value: U8) -> Other { return Other::Data(value); } fn accept(value: Choice) -> Choice { return value; }","accept(make(input.marker))","");
    assert_eq!(
        amblicit_syntax::lower_to_target_ir(&core, &facts()).status,
        TargetLoweringStatus::Lowered
    );
    let value = core
        .intents
        .get_mut("run")
        .unwrap()
        .body
        .nodes
        .iter_mut()
        .find_map(|node| match node {
            CoreNode::Let { value, .. } => Some(value),
            _ => None,
        })
        .unwrap();
    let CoreExpr::Call { args, .. } = value else {
        panic!("accept call")
    };
    let CoreExpr::Call { callee, .. } = &mut args[0] else {
        panic!("constructor helper")
    };
    *callee = "choices.example@1.alternate".to_owned();
    let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:?}"
    );
    assert!(report.artifact.is_none());
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == TargetLoweringFailureKind::InvalidCoreIdentity));
}

#[test]
fn variant_typed_input_can_flow_through_a_helper_to_a_scalar_result_projection() {
    let source="package choices.example@1; type Choice = variant { Empty, Data(U8) }; type Input = { choice: Choice, marker: U8, }; type Output = { value: U8, }; fn identity(value: Choice) -> Choice { return value; } intent run(input: Input) returns Output profile p.read basis none budget <= p.small { let value: Choice = identity(input.choice); return { value: input.marker }; }";
    let core = compile_to_core(&parse_module(source).unwrap(), &context()).unwrap();
    assert_eq!(core.required_core_capabilities, vec!["core.variant/v1"]);
    let report = amblicit_syntax::lower_to_target_ir(&core, &facts());
    assert_eq!(
        report.status,
        amblicit_syntax::TargetLoweringStatus::Lowered,
        "{report:?}"
    );
    assert!(report.artifact.is_some());
    assert!(report.result_projections.contains_key("run"));
    assert!(report.result_projection_failures.is_empty());
}
