//! Source variant types preserve the existing Core tagged-union graph.
use amblicit_syntax::source_closure::{
    resolve_source_closure, source_module_digest, SourceClosureLimits,
};
use amblicit_syntax::source_linker::link_source_closure;
use amblicit_syntax::{
    compile_to_core, digest_core_module, parse_module, CompilerContext, CompilerErrorKind, CoreType,
};
use std::fmt::Write;

fn compile(declarations: &str) -> amblicit_syntax::CoreModule {
    let source = format!("package choices.example@1; {declarations}");
    compile_to_core(&parse_module(&source).unwrap(), &CompilerContext::new())
        .expect("acyclic variant declaration graph compiles")
}

fn refusal(declarations: &str, kind: CompilerErrorKind) {
    let source = format!("package choices.example@1; {declarations}");
    let errors = compile_to_core(&parse_module(&source).unwrap(), &CompilerContext::new())
        .expect_err("invalid variant graph refuses");
    assert!(errors.iter().any(|error| error.kind == kind), "{errors:?}");
}

#[test]
fn variant_cases_and_payloads_keep_exact_named_core_identity() {
    let core = compile("type Choice = variant { Empty, Data(Payload), Byte(U8) }; type Payload = { bytes: Bytes<max=4>, }; ");
    let CoreType::Variant { cases } = &core.types["Choice"] else {
        panic!("tagged union")
    };
    assert_eq!(cases.len(), 3);
    assert_eq!(cases["Empty"], None);
    assert_eq!(cases["Data"].as_deref(), Some("choices.example@1.Payload"));
    assert_eq!(cases["Byte"].as_deref(), Some("U8"));
    digest_core_module(&core).expect("valid canonical Core");
}

#[test]
fn variant_aliases_and_reordered_declarations_preserve_canonical_meaning() {
    let a = compile("type Choice = variant { Empty, Data(Payload) }; type Payload = { count: U64, }; type Alias = Choice; type Envelope = { choice: Alias, };");
    let b = compile("type Envelope = { choice: Alias, }; type Alias = Choice; type Payload = { count: U64, }; type Choice = variant { Data(Payload), Empty };");
    assert!(!a.types.contains_key("Alias"));
    let CoreType::Record { fields } = &a.types["Envelope"] else {
        panic!("record")
    };
    assert_eq!(fields["choice"], "choices.example@1.Choice");
    assert_eq!(
        digest_core_module(&a).unwrap(),
        digest_core_module(&b).unwrap()
    );
}

#[test]
fn linked_variant_payloads_keep_their_original_module_coordinates() {
    let library = "package library.choices@1; type Choice = variant { Empty, Data(Payload) }; type Payload = { value: U64, };";
    let root = format!("package choices.example@1; use module library.choices@1 digest \"{}\" as choices; type Envelope = {{ choice: choices.Choice, }};", source_module_digest(library).unwrap().to_review_string());
    let closure =
        resolve_source_closure(&root, &[library], SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).unwrap();
    let core = linked
        .compile(&CompilerContext::new())
        .expect("linked variant types");
    let CoreType::Record { fields } = &core.types["Envelope"] else {
        panic!("record")
    };
    assert_eq!(fields["choice"], "library.choices@1.Choice");
    let variant = core
        .types
        .values()
        .find_map(|ty| match ty {
            CoreType::Variant { cases } => Some(cases),
            _ => None,
        })
        .expect("imported variant");
    assert_eq!(
        variant["Data"].as_deref(),
        Some("library.choices@1.Payload")
    );
    digest_core_module(&core).unwrap();
}

#[test]
fn variant_payloads_must_resolve_even_when_unused() {
    refusal(
        "type Choice = variant { MissingPayload(Unknown) };",
        CompilerErrorKind::UnresolvedType,
    );
}

#[test]
fn variant_payloads_preserve_authenticated_nominal_contracts() {
    let pin = format!("sha256:{}", "1".repeat(64));
    let context = CompilerContext::new().with_type_shape(amblicit_syntax::TypeShapeFact {
        lawpack: amblicit_syntax::ResourceRef {
            coordinate: "example.identities@1".into(),
            digest: Some(pin.clone()),
        },
        coordinate: "example.identities@1.Key".into(),
        definition: "Nominal<Bytes<exact=32>>".into(),
    });
    let source = format!("package choices.example@1; use lawpack example.identities@1 digest \"{pin}\" as identities; type Choice = variant {{ Empty, Key(identities.Key) }};");
    let core = compile_to_core(&parse_module(&source).unwrap(), &context).unwrap();
    let CoreType::Variant { cases } = &core.types["Choice"] else {
        panic!("variant")
    };
    assert_eq!(cases["Key"].as_deref(), Some("example.identities@1.Key"));
    assert!(
        matches!(&core.types["example.identities@1.Key"], CoreType::Nominal { contract, representation }
        if contract == "example.identities@1.Key" && representation == "Bytes<exact=32>")
    );
    digest_core_module(&core).unwrap();
}

#[test]
fn variant_empty_and_duplicate_case_sets_refuse() {
    refusal(
        "type Choice = variant {};",
        CompilerErrorKind::UnsupportedSourceShape,
    );
    refusal(
        "type Choice = variant { Empty, Empty };",
        CompilerErrorKind::SurfaceValidation,
    );
    let duplicated =
        parse_module("package choices.example@1; type Choice = variant { Empty, Empty };").unwrap();
    let errors = amblicit_syntax::validate_surface(&duplicated).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == amblicit_syntax::SemanticErrorKind::DuplicateName));
}

#[test]
fn recursive_variant_payload_graphs_remain_rejected() {
    for source in [
        "type Tree = variant { Empty, Branch(Tree) };",
        "type Tree = variant { Empty, Branch(Payload) }; type Payload = { child: Tree, };",
    ] {
        refusal(source, CompilerErrorKind::InvalidCoreTypeIntegrity);
    }
}

#[test]
fn variant_payload_depth_is_bounded() {
    let mut declarations = "type Leaf = variant { Empty };".to_owned();
    for depth in 0..130 {
        let previous = if depth == 0 {
            "Leaf".to_owned()
        } else {
            format!("Node{}", depth - 1)
        };
        write!(
            declarations,
            "type Node{depth} = variant {{ Next({previous}) }};"
        )
        .unwrap();
    }
    refusal(&declarations, CompilerErrorKind::InvalidBound);
}

#[test]
fn payloadless_variant_cases_consume_the_source_work_budget() {
    let mut declarations = "type Choice = variant {".to_owned();
    for index in 0..65_537 {
        write!(declarations, "Case{index},").unwrap();
    }
    declarations.push_str("};");
    refusal(&declarations, CompilerErrorKind::InvalidBound);
}

#[test]
fn variant_construction_and_matching_remain_explicitly_unsupported() {
    for (function, expression) in [
        (
            "fn make() -> Choice { return Choice::Empty; }",
            "Choice::Empty",
        ),
        (
            "fn inspect(value: Choice) -> U64 { return match value { Empty => 0u64 }; }",
            "match value",
        ),
    ] {
        let source =
            format!("package choices.example@1; type Choice = variant {{ Empty }}; {function}");
        let errors =
            compile_to_core(&parse_module(&source).unwrap(), &CompilerContext::new()).unwrap_err();
        let start = source.find(expression).unwrap();
        assert!(
            errors.iter().any(
                |error| error.kind == CompilerErrorKind::UnsupportedSourceShape
                    && error.span.start == start
            ),
            "{errors:?}"
        );
    }
}

fn execution_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            amblicit_syntax::CoreBudget {
                max_steps: 4096,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: 64,
            },
        )
}

#[test]
fn source_variants_require_a_core_capability_before_target_lowering() {
    use amblicit_syntax::{
        lower_to_target_ir, ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind,
        TargetLoweringStatus, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
    };
    let facts = TargetIrLoweringFacts {
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
    };
    for (declaration, input, needs_variant) in [
        ("", "U64", false),
        ("type Choice = variant { Empty };", "U64", true),
        ("type Choice = variant { Empty };", "Choice", true),
    ] {
        let source = format!("package choices.example@1; {declaration} intent run(input: {input}) returns U64 profile p.read basis none budget <= p.small {{ return 0u64; }}");
        let core = compile_to_core(&parse_module(&source).unwrap(), &execution_context()).unwrap();
        let expected: Vec<String> = if needs_variant {
            vec!["core.variant/v1".to_owned()]
        } else {
            Vec::new()
        };
        assert_eq!(core.required_core_capabilities, expected);
        let report = lower_to_target_ir(&core, &facts);
        if needs_variant {
            assert_eq!(report.status, TargetLoweringStatus::Unsupported);
            assert!(report.artifact.is_none());
            assert_eq!(report.failures.len(), 1);
            assert_eq!(
                report.failures[0].kind,
                TargetLoweringFailureKind::UnsupportedCoreCapability
            );
        } else {
            assert!(report.failures.iter().all(
                |failure| failure.kind != TargetLoweringFailureKind::UnsupportedCoreCapability
            ));
        }
    }
}

#[test]
fn variant_function_storage_refuses_as_unsupported_without_retagging_overflow() {
    let declarations = "type Choice = variant { Empty }; type Envelope = { choice: Choice, }; type Alias = Choice;";
    for ty in ["Choice", "Envelope", "Alias", "List<Choice,max=1>"] {
        let source = format!("package choices.example@1; {declarations} fn identity(input: {ty}) -> {ty} {{ return input; }}");
        let errors =
            compile_to_core(&parse_module(&source).unwrap(), &CompilerContext::new()).unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{ty}: {errors:?}"
        );
    }
    refusal("fn identity(input: List<U64,max=18446744073709551615>) -> List<U64,max=18446744073709551615> { return input; }", CompilerErrorKind::InvalidBound);
}

#[test]
fn variant_intent_storage_and_result_boundaries_are_explicit() {
    for (helper, output, body) in [
        ("fn unused() -> U64 { return 0u64; }", "U64", "return 0u64;"),
        ("", "Choice", "return input;"),
    ] {
        let source = format!("package choices.example@1; type Choice = variant {{ Empty }}; {helper} intent run(input: Choice) returns {output} profile p.read basis none budget <= p.small {{ {body} }}");
        let errors =
            compile_to_core(&parse_module(&source).unwrap(), &execution_context()).unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape),
            "{errors:?}"
        );
    }
}

#[test]
fn staged_type_checking_rejects_duplicate_variant_labels_at_the_repeated_span() {
    let source = "package choices.example@1; type Choice = variant { Same(U64), Same(Bool) };";
    let parsed = parse_module(source).unwrap();
    let resolved = amblicit_syntax::resolve_module(&parsed, &CompilerContext::new()).unwrap();
    let errors = amblicit_syntax::type_check(&resolved).unwrap_err();
    let duplicate = errors
        .iter()
        .find(|error| error.kind == CompilerErrorKind::SurfaceValidation)
        .unwrap();
    assert_eq!(duplicate.span.start, source.rfind("Same").unwrap());
    assert_eq!(
        &source[duplicate.span.start..duplicate.span.end],
        "Same(Bool)"
    );
    let surface = amblicit_syntax::validate_surface(&parsed).unwrap_err();
    let duplicate = surface
        .iter()
        .find(|error| error.kind == amblicit_syntax::SemanticErrorKind::DuplicateName)
        .unwrap();
    assert_eq!(
        &source[duplicate.span.start..duplicate.span.end],
        "Same(Bool)"
    );
    compile("type Choice = variant { First(U64), Second(Bool) };");
}

#[test]
fn variant_payload_diagnostics_cover_each_invalid_case_without_a_partial_type() {
    let source =
        "package choices.example@1; type Choice = variant { First(Unknown), Second(AlsoUnknown) };";
    let parsed = parse_module(source).unwrap();
    for errors in [
        compile_to_core(&parsed, &CompilerContext::new()).unwrap_err(),
        amblicit_syntax::type_check(
            &amblicit_syntax::resolve_module(&parsed, &CompilerContext::new()).unwrap(),
        )
        .unwrap_err(),
    ] {
        let spans: std::collections::BTreeSet<_> = errors
            .iter()
            .filter(|error| error.kind == CompilerErrorKind::UnresolvedType)
            .map(|error| &source[error.span.start..error.span.end])
            .collect();
        assert_eq!(
            spans,
            ["First(Unknown)", "Second(AlsoUnknown)"]
                .into_iter()
                .collect()
        );
    }
    let duplicated =
        "package choices.example@1; type Choice = variant { Same(Unknown), Same(Bool) };";
    let parsed = parse_module(duplicated).unwrap();
    let errors = amblicit_syntax::type_check(
        &amblicit_syntax::resolve_module(&parsed, &CompilerContext::new()).unwrap(),
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedType));
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::SurfaceValidation
            && error.span.start == duplicated.rfind("Same").unwrap()));
}

fn bounded_variant_result(declarations: &str, accepted: bool) {
    let source = format!("package choices.example@1; {declarations}");
    let result = compile_to_core(&parse_module(&source).unwrap(), &CompilerContext::new());
    match (accepted, result) {
        (true, Ok(_)) => {}
        (false, Err(errors)) => assert!(
            errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::InvalidBound),
            "{errors:?}"
        ),
        (true, Err(errors)) => panic!("boundary should compile: {errors:?}"),
        (false, Ok(_)) => panic!("oversized variant graph unexpectedly compiled"),
    }
}

#[test]
fn variant_structural_depth_composes_across_short_dependency_paths() {
    let lists = |depth, leaf| format!("{}{leaf}{}", "List<".repeat(depth), ",max=1>".repeat(depth));
    for (outer_depth, accepted) in [(63, true), (64, false)] {
        // Each variant adds one edge: (1 + 63) + (1 + outer_depth).
        // Both individual references and the two-declaration path are shallow.
        bounded_variant_result(
            &format!(
                "type Child = variant {{ Data({}) }}; type Parent = variant {{ Data({}) }};",
                lists(63, "U8"),
                lists(outer_depth, "Child")
            ),
            accepted,
        );
    }
}

#[test]
fn variant_case_and_repeated_payload_expansion_boundaries_are_exact() {
    let cases = |count| {
        let mut output = String::new();
        for index in 0..count {
            write!(output, "Case{index},").unwrap();
        }
        output
    };
    for (count, accepted) in [(65_535, true), (65_536, false)] {
        // One declaration plus one unit per payloadless case.
        bounded_variant_result(
            &format!("type Choice = variant {{ {} }};", cases(count)),
            accepted,
        );
    }
    for (fields, accepted) in [(64, true), (65, false)] {
        // Large costs 1001; each reference repeats that cost. Aggregate costs
        // are 1001 + 1 + 64*1001 = 65066 and 1001 + 1 + 65*1001 = 66067.
        let mut field_source = String::new();
        for index in 0..fields {
            write!(field_source, "field{index}: Large,").unwrap();
        }
        bounded_variant_result(
            &format!(
                "type Large = variant {{ {} }}; type Envelope = {{ {field_source} }};",
                cases(1000)
            ),
            accepted,
        );
    }
}
