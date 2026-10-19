//! Return budgets use declared canonical size bounds, independently of helpers.
use edict_syntax::{
    compile_to_core, encode_canonical_cbor, parse_module, CanonicalValue, CompilerContext,
    CompilerErrorKind, CompilerStage, CoreBudget, ResourceRef, TypeShapeFact,
};

fn context(output: u64) -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 4096,
                max_allocated_bytes: 1_048_576,
                max_output_bytes: output,
            },
        )
}

fn source(declarations: &str, output: &str) -> String {
    format!("package return_budget.example@1; {declarations} intent run(input: {output}) returns {output} profile p.read basis none budget <= p.small {{ return input; }}")
}

#[test]
fn return_budget_exact_fit_accepts_and_one_byte_less_rejects() {
    // Each size is an independent canonical-CBOR upper bound for the type.
    for (declarations, output, maximum) in [
        ("", "Bool", 1),
        ("", "U32", 5),
        ("", "I32", 5),
        ("", "U64", 9),
        ("", "I64", 9),
        ("", "Bytes<max=23>", 24),
        ("", "Bytes<max=24>", 26),
        ("", "Bytes<max=255>", 257),
        ("", "Bytes<max=256>", 259),
        ("", "Bytes<max=65535>", 65_538),
        ("", "Bytes<max=65536>", 65_541),
        ("", "Bytes<exact=24>", 26),
        ("", "String<max=5>", 21),
        ("", "String<max=6>", 26),
        ("", "String<max=64>", 259),
        ("", "List<Bool, max=23>", 24),
        ("", "List<Bool, max=24>", 26),
        ("", "List<Bytes<max=24>, max=2>", 53),
        (
            "type Result = { key: String<max=64>, descriptor: String<max=4096> };",
            "Result",
            16_662,
        ),
        (
            "type Inner = { value: Bytes<max=24> }; type Result = { items: List<Inner, max=2> };",
            "Result",
            74,
        ),
    ] {
        let parsed = parse_module(&source(declarations, output)).expect("budget fixture parses");
        compile_to_core(&parsed, &context(maximum)).expect("exact declared output maximum fits");
        let errors = compile_to_core(&parsed, &context(maximum - 1))
            .expect_err("an output maximum one byte above the budget must fail");
        assert_eq!(errors.len(), 1, "{output}: {errors:?}");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::InvalidBound,
            "{output}: {errors:?}"
        );
        assert_eq!(errors[0].stage, CompilerStage::TypeCheck);
    }
}

#[test]
fn return_budget_overflow_rejects_before_core() {
    for (declarations, output) in [
        ("", "String<max=18446744073709551615>"),
        ("", "Bytes<max=18446744073709551615>"),
        ("", "List<U64, max=18446744073709551615>"),
        // Payload multiplication fits U64, but the list header does not.
        ("", "List<U64, max=2049638230412172401>"),
        // Each field fits; the record's checked accumulation does not.
        (
            "type Result = { first: String<max=4611686018427387899>, second: U64 };",
            "Result",
        ),
    ] {
        let parsed = parse_module(&source(declarations, output)).expect("overflow source parses");
        let errors =
            compile_to_core(&parsed, &context(u64::MAX)).expect_err("size overflow rejects");
        assert_eq!(errors[0].kind, CompilerErrorKind::InvalidBound);
    }
}

#[test]
fn return_budget_refuses_opaque_requests_but_preserves_bounded_outputs() {
    for (declarations, output) in [
        ("", "List<ExternalActionRequest<U64>, max=0>"),
        (
            "type Result = { request: ExternalActionRequest<U64> };",
            "Result",
        ),
    ] {
        let parsed = parse_module(&source(declarations, output)).expect("request output parses");
        let errors = compile_to_core(&parsed, &context(4096)).expect_err("opaque output rejects");
        assert_eq!(errors[0].kind, CompilerErrorKind::UnsupportedSourceShape);
    }
    let request_only = parse_module(&source("", "ExternalActionRequest<U64>")).unwrap();
    compile_to_core(&request_only, &context(4096))
        .expect("request-only protocol output remains accepted");
    let parsed = parse_module("package return_budget.example@1; intent run(input: ExternalActionRequest<U64>) returns U64 profile p.read basis none budget <= p.small { return 0u64; }").unwrap();
    compile_to_core(&parsed, &context(9)).expect("opaque input does not poison bounded output");
}

#[test]
fn return_budget_preserves_imported_nominal_representation_bounds() {
    let source = "package return_budget.example@1; use lawpack bounds.example@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as bounds; intent run(input: bounds.Key) returns bounds.Key profile p.read basis none budget <= p.small { return input; }";
    let parsed = parse_module(source).unwrap();
    let fact = TypeShapeFact {
        lawpack: ResourceRef {
            coordinate: "bounds.example@1".to_owned(),
            digest: Some(
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_owned(),
            ),
        },
        coordinate: "bounds.example@1.Key".to_owned(),
        definition: "Nominal<Bytes<exact=24>>".to_owned(),
    };
    compile_to_core(&parsed, &context(26).with_type_shape(fact.clone())).unwrap();
    let errors = compile_to_core(&parsed, &context(25).with_type_shape(fact)).unwrap_err();
    assert_eq!(errors[0].kind, CompilerErrorKind::InvalidBound);
}

#[test]
fn return_budget_unicode_record_maximum_matches_canonical_encoder() {
    let output = CanonicalValue::Map(vec![
        (
            CanonicalValue::Text("key".to_owned()),
            CanonicalValue::Text("\u{1f600}".repeat(64)),
        ),
        (
            CanonicalValue::Text("descriptor".to_owned()),
            CanonicalValue::Text("\u{1f600}".repeat(4096)),
        ),
    ]);
    let actual = encode_canonical_cbor(&output).unwrap();
    assert_eq!(actual.len(), 16_662);
    let parsed = parse_module(&source(
        "type Result = { key: String<max=64>, descriptor: String<max=4096> };",
        "Result",
    ))
    .unwrap();
    compile_to_core(&parsed, &context(u64::try_from(actual.len()).unwrap())).unwrap();
    let errors =
        compile_to_core(&parsed, &context(u64::try_from(actual.len() - 1).unwrap())).unwrap_err();
    assert_eq!(errors[0].kind, CompilerErrorKind::InvalidBound);
}

#[test]
fn return_budget_supports_every_accepted_imported_integer_width() {
    let parsed = parse_module("package return_budget.example@1; use lawpack bounds.example@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as bounds; intent run(input: bounds.Small) returns bounds.Small profile p.read basis none budget <= p.small { return input; }").unwrap();
    for (width, maximum) in [
        ("I8", 2),
        ("U8", 2),
        ("I16", 3),
        ("U16", 3),
        ("I32", 5),
        ("U32", 5),
        ("I64", 9),
        ("U64", 9),
    ] {
        let fact = TypeShapeFact {
            lawpack: ResourceRef {
                coordinate: "bounds.example@1".to_owned(),
                digest: Some(
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
                ),
            },
            coordinate: "bounds.example@1.Small".to_owned(),
            definition: width.to_owned(),
        };
        compile_to_core(&parsed, &context(maximum).with_type_shape(fact.clone()))
            .expect("every accepted Core integer width has a canonical bound");
        let errors =
            compile_to_core(&parsed, &context(maximum - 1).with_type_shape(fact)).unwrap_err();
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::InvalidBound,
            "{width}: {errors:?}"
        );
    }
}
