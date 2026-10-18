//! Byte literals preserve raw octets through the public source compiler.
use edict_syntax::{
    compile_to_core, digest_core_module, lex, lower_to_target_ir, parse_module, CompilerContext,
    CompilerErrorKind, CoreBudget, CoreExpr, CoreModule, CoreNode, CoreValue, ParseErrorKind,
    ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};

fn source(literal: &str, bound: u64) -> String {
    format!(
        "package literal.example@1;
        type Input = {{ unused: U64, }};
        type Output = {{ bytes: Bytes<max={bound}>, }};
        intent emit(input: Input) returns Output
          profile p.read basis none budget <= p.small {{
          let bytes = {literal};
          return {{ bytes }};
        }}"
    )
}

fn compile(literal: &str) -> CoreModule {
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 1024,
            },
        );
    compile_to_core(&parse_module(&source(literal, 64)).unwrap(), &context).unwrap()
}

#[test]
fn bytes_prefix_is_one_token_with_the_complete_source_span() {
    let tokens = lex(r#"b"a\r\0\xFF""#).unwrap();
    assert_eq!(
        tokens.len(),
        2,
        "one literal plus EOF, not identifier b plus a string"
    );
    assert_eq!(tokens[0].span.start, 0);
    assert_eq!(tokens[0].span.end, 12);
    assert_eq!(lex(r#"b "separate""#).unwrap().len(), 3);
}

#[test]
fn source_bytes_literals_lower_to_exact_raw_octets() {
    for (literal, expected) in [
        (r#"b"""#, vec![]),
        (r#"b"ASCII""#, b"ASCII".to_vec()),
        (r#"b"\r\n\t\0\\\"""#, vec![13, 10, 9, 0, 92, 34]),
        (r#"b"\x00\x7F\x80\xFF""#, vec![0, 127, 128, 255]),
        (r#"b"\xc3\xa9""#, vec![195, 169]),
    ] {
        let core = compile(literal);
        assert!(
            matches!(&core.intents["emit"].body.nodes[0],
            CoreNode::Let { value: CoreExpr::Const(CoreValue::Bytes(bytes)), .. }
                if bytes == &expected),
            "{literal}"
        );
        let report = lower_to_target_ir(
            &core,
            &TargetIrLoweringFacts {
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
            },
        );
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{:?}",
            report.failures
        );
        assert_eq!(
            report.artifact.unwrap().intents["emit"].pure_bindings[0].value,
            CoreExpr::Const(CoreValue::Bytes(expected))
        );
    }
}

#[test]
fn byte_spellings_share_a_semantic_digest_but_different_octets_do_not() {
    assert_eq!(
        digest_core_module(&compile(r#"b"A""#)).unwrap(),
        digest_core_module(&compile(r#"b"\x41""#)).unwrap()
    );
    assert_ne!(
        digest_core_module(&compile(r#"b"A""#)).unwrap(),
        digest_core_module(&compile(r#"b"B""#)).unwrap()
    );
}

#[test]
fn invalid_byte_spellings_fail_at_the_lexical_boundary() {
    for literal in [
        r#"b"é""#,
        r#"b"\x""#,
        r#"b"\x1""#,
        r#"b"\xGG""#,
        r#"b"\q""#,
        r#"b"\u{41}""#,
        "b\"unfinished",
    ] {
        let error = parse_module(&source(literal, 64)).unwrap_err();
        assert_eq!(error.kind, ParseErrorKind::Lex, "{literal}");
    }
    assert!(lex(r#""\xFF""#).is_err(), "strings must remain valid UTF-8");
    assert!(
        lex(r#""é\r\0\x41""#).is_ok(),
        "strings retain Unicode and ASCII escapes"
    );
}

#[test]
fn byte_literal_length_still_obeys_the_declared_output_bound() {
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 1024,
            },
        );
    let errors =
        compile_to_core(&parse_module(&source(r#"b"\xFFab""#, 2)).unwrap(), &context).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn malformed_hex_escape_spans_include_digits_but_not_the_closing_quote() {
    for (literal, start, end) in [
        (r#"b"\xGG""#, 2, 6),
        (r#"b"\x1""#, 2, 5),
        (r#"b"\x""#, 2, 4),
        ("b\"\\x1", 2, 5),
        (r#""\xG""#, 1, 4),
    ] {
        let error = lex(literal).unwrap_err();
        assert_eq!(
            (error.span.start, error.span.end),
            (start, end),
            "{literal}"
        );
    }
}
