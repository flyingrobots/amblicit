//! Primary effect diagnostics use the exact authenticated lawpack closure.
use amblicit_syntax::{
    compile_to_core, decode_lawpack_adapter, decode_lawpack_bundle, parse_module,
    prepare_lawpack_compilation, CompilerError, CompilerErrorKind, CompilerStage, WriteClass,
};

const SOURCE: &str = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");
const MANIFEST: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor");
const EXPORTS: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor");
const ADAPTER: &[u8] = include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor");

fn compile(source: &str) -> Result<amblicit_syntax::CoreModule, Vec<CompilerError>> {
    let module = parse_module(source).expect("effect source parses");
    let bundle = decode_lawpack_bundle(MANIFEST, EXPORTS).unwrap();
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER).unwrap();
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter).unwrap();
    let context = preparation
        .compiler_context()
        .clone()
        .with_effect_write_class("other.secret", WriteClass::Create);
    compile_to_core(&module, &context)
}

#[test]
fn bare_imported_effect_reports_missing_failure_mapping() {
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(SOURCE.matches(mapping).count(), 1);
    let source = SOURCE.replace(mapping, "");
    let errors = compile(&source).unwrap_err();
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::EffectWithoutFailureMapping);
    assert_eq!(cause.stage, CompilerStage::TypeCheck);
    assert!(source[cause.span.start..cause.span.end].contains("hello.createGreeting"));
    assert!(cause.message.contains("alreadyExists"));
}

#[test]
fn unknown_effect_lists_its_exact_owner_exports() {
    for alias in ["hello", "cells"] {
        let source = SOURCE
            .replace("as hello;", &format!("as {alias};"))
            .replace("hello.", &format!("{alias}."))
            .replace(&format!("{alias}.echo@1"), "hello.echo@1");
        let requested = format!("{alias}.update(input)");
        let source = source.replace(&format!("{alias}.createGreeting(input)"), &requested);
        let errors = compile(&source).unwrap_err();
        let cause = &errors[0];
        assert_eq!(cause.kind, CompilerErrorKind::MissingContextFact);
        assert_eq!(cause.stage, CompilerStage::TypeCheck);
        assert!(source[cause.span.start..cause.span.end].contains(&requested));
        assert!(cause.message.contains("hello.echo@1"));
        assert!(cause.message.contains(&format!("{alias}.createGreeting")));
        assert!(!cause.message.contains("other.secret"));
    }
}

#[test]
fn mapped_effect_controls_remain_valid() {
    compile(SOURCE).expect("mapped effect compiles");
    let renamed = SOURCE
        .replace("as hello;", "as cells;")
        .replace("hello.", "cells.")
        .replace("cells.echo@1", "hello.echo@1");
    compile(&renamed).expect("exact owner remains valid through renamed alias");
}

#[test]
fn bare_unknown_call_remains_unresolved_function() {
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(SOURCE.matches(mapping).count(), 1);
    assert_eq!(SOURCE.matches("hello.createGreeting(input)").count(), 1);
    let source = SOURCE
        .replace(mapping, "")
        .replace("hello.createGreeting(input)", "hello.update(input)");
    let errors = compile(&source).expect_err("unknown bare call has no helper fact");
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::UnresolvedFunction);
    assert_eq!(cause.stage, CompilerStage::TypeCheck);
    assert!(source[cause.span.start..cause.span.end].contains("hello.update(input)"));
}

#[test]
fn legacy_context_without_signatures_does_not_claim_no_exports() {
    let source = r#"package a.b@1;
use lawpack hello.echo@1 digest "sha256:1111111111111111111111111111111111111111111111111111111111111111" as hello;
type Input = { id: String<max=16>, };
type Receipt = { id: String<max=16>, };
type Output = { id: String<max=16>, };
intent t(input: Input) returns Output
  profile p.effectful
  basis none
  budget <= p.tiny {
  let receipt: Receipt = hello.known(input.id)
    else { rejected(reason) => domain.WriteRejected };
  return { id: input.id };
}"#;
    let context = amblicit_syntax::CompilerContext::new()
        .with_operation_profile("p.effectful", "test.effectful@1")
        .with_operation_profile_write_classes("p.effectful", [WriteClass::Create])
        .with_budget(
            "p.tiny",
            amblicit_syntax::CoreBudget {
                max_steps: 100,
                max_allocated_bytes: 512,
                max_output_bytes: 512,
            },
        )
        .with_effect_write_class("hello.known", WriteClass::Create);
    let known = parse_module(source).expect("legacy effect source parses");
    compile_to_core(&known, &context)
        .expect("legacy write-class context still compiles its known effect");
    assert_eq!(source.matches("hello.known(input.id)").count(), 1);
    let unknown = source.replace("hello.known(input.id)", "hello.unknown(input.id)");
    let module = parse_module(&unknown).expect("unknown legacy effect parses");
    let errors =
        compile_to_core(&module, &context).expect_err("unknown effect has no write-class fact");
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::MissingContextFact);
    assert_eq!(cause.stage, CompilerStage::TypeCheck);
    assert!(unknown[cause.span.start..cause.span.end].contains("hello.unknown(input.id)"));
    assert!(cause
        .message
        .contains("missing authenticated effect export information"));
    assert!(!cause.message.contains("exports: none"));
}

#[test]
fn effect_guidance_refuses_pure_functions_and_intent_clauses() {
    let function = "fn illegal(input: hello.CreateGreetingInput) -> hello.GreetingReceipt {\n  return hello.createGreeting(input);\n}\n\n";
    assert_eq!(SOURCE.matches("intent createGreeting").count(), 1);
    let function_source = SOURCE.replace(
        "intent createGreeting",
        &format!("{function}intent createGreeting"),
    );
    assert_eq!(SOURCE.matches("basis input.basis").count(), 1);
    let basis_source = SOURCE.replace("basis input.basis", "basis hello.createGreeting(input)");
    for source in [function_source, basis_source] {
        let errors = compile(&source).expect_err("semantic effects cannot run in a pure context");
        let cause = &errors[0];
        assert_eq!(cause.kind, CompilerErrorKind::UnsupportedSourceShape);
        assert_eq!(cause.stage, CompilerStage::TypeCheck);
        assert!(source[cause.span.start..cause.span.end].contains("hello.createGreeting(input)"));
        assert!(cause.message.contains("unavailable in this pure context"));
        assert!(!cause.message.contains("with `else`"));
    }
}

#[test]
fn effect_arity_refuses_mapping_guidance() {
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(SOURCE.matches(mapping).count(), 1);
    for args in ["", "input, input"] {
        let call = format!("hello.createGreeting({args})");
        let mapped = SOURCE.replace("hello.createGreeting(input)", &call);
        for source in [mapped.clone(), mapped.replace(mapping, "")] {
            let errors = compile(&source).expect_err("effect arity is unsupported");
            let cause = &errors[0];
            assert_eq!(cause.kind, CompilerErrorKind::UnsupportedSourceShape);
            assert_eq!(cause.stage, CompilerStage::TypeCheck);
            assert!(source[cause.span.start..cause.span.end].contains(&call));
            assert!(cause.message.contains("exactly one effect argument"));
            assert!(!cause.message.contains("with `else`"));
        }
    }
}

#[test]
fn bare_return_effect_checks_profile_before_mapping_guidance() {
    let start = SOURCE
        .find("{\n  let receipt:")
        .expect("intent body begins");
    let prefix = &SOURCE[..start];
    for nested in [false, true] {
        let (output, value) = if nested {
            ("NestedReceipt", "{ receipt: hello.createGreeting(input), }")
        } else {
            ("hello.GreetingReceipt", "hello.createGreeting(input)")
        };
        let prefix = prefix.replace("returns GreetingCreated", &format!("returns {output}"));
        let prefix = if nested {
            prefix.replace("intent createGreeting", "type NestedReceipt = { receipt: hello.GreetingReceipt, };\n\nintent createGreeting")
        } else {
            prefix
        };
        let source = format!("{prefix}{{\n  return {value};\n}}\n");
        let module = parse_module(&source).expect("bare return source parses");
        let bundle = decode_lawpack_bundle(MANIFEST, EXPORTS).expect("load lawpack");
        let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER).expect("load adapter");
        let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter)
            .expect("prepare exact return effect");
        let allowed = compile_to_core(&module, preparation.compiler_context())
            .expect_err("allowed bare effect still lacks a map");
        assert_eq!(
            allowed[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
        let denied = preparation
            .compiler_context()
            .clone()
            .with_operation_profile_write_classes("hello.createGreeting", []);
        let errors = compile_to_core(&module, &denied).expect_err("profile forbids the effect");
        let cause = &errors[0];
        assert_eq!(cause.kind, CompilerErrorKind::ProfileEffectMismatch);
        assert_eq!(cause.stage, CompilerStage::TypeCheck);
        assert!(source[cause.span.start..cause.span.end].contains("hello.createGreeting(input)"));
        assert!(!cause.message.contains("with `else`"));
    }
}

#[test]
fn bare_effect_arguments_report_primary_input_errors() {
    let mapping = "\n    else { alreadyExists(existing) => hello.AlreadyExists }";
    assert_eq!(SOURCE.matches(mapping).count(), 1);
    for (argument, expected) in [
        ("true", CompilerErrorKind::TypeMismatch),
        ("missing", CompilerErrorKind::UnresolvedType),
        ("input.absent", CompilerErrorKind::UnknownField),
    ] {
        let call = format!("hello.createGreeting({argument})");
        let mapped = SOURCE.replace("hello.createGreeting(input)", &call);
        for source in [mapped.clone(), mapped.replace(mapping, "")] {
            let errors = compile(&source).expect_err("invalid effect input must reject");
            let cause = &errors[0];
            assert_eq!(cause.kind, expected, "{argument}: {errors:?}");
            assert_eq!(cause.stage, CompilerStage::TypeCheck);
            assert!(source[cause.span.start..cause.span.end].contains(argument));
            assert!(!cause.message.contains("with `else`"));
        }
    }
}

#[test]
fn bare_effect_output_checks_surrounding_expected_type() {
    let start = SOURCE
        .find("{\n  let receipt:")
        .expect("intent body begins");
    for nested in [false, true] {
        let prefix = if nested {
            SOURCE[..start]
                .replace(
                    "intent createGreeting",
                    "type NestedResult = { receipt: GreetingCreated, };\n\nintent createGreeting",
                )
                .replace("returns GreetingCreated", "returns NestedResult")
        } else {
            SOURCE[..start].to_owned()
        };
        let value = if nested {
            "{ receipt: hello.createGreeting(input), }"
        } else {
            "hello.createGreeting(input)"
        };
        let source = format!("{prefix}{{\n return {value};\n}}\n");
        let control = if nested {
            source.replace("receipt: GreetingCreated", "receipt: hello.GreetingReceipt")
        } else {
            source.replace("returns GreetingCreated", "returns hello.GreetingReceipt")
        };
        assert_eq!(
            compile(&control).expect_err("bare control needs a map")[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
        let errors = compile(&source).expect_err("effect output is incompatible");
        assert_eq!(errors[0].kind, CompilerErrorKind::TypeMismatch);
        assert!(source[errors[0].span.start..errors[0].span.end]
            .contains("hello.createGreeting(input)"));
        assert!(!errors[0].message.contains("with `else`"));
    }
}

#[test]
fn effect_predicates_report_expected_predicate_before_map_guidance() {
    for assertion in [
        "hello.createGreeting(input)",
        "if true then hello.createGreeting(input) else hello.createGreeting(input)",
    ] {
        let source = SOURCE.replace(
            "  let receipt:",
            &format!("  require {assertion} else hello.AlreadyExists;\n  let receipt:"),
        );
        let control = source.replace(
            &format!("require {assertion} else hello.AlreadyExists;"),
            "require true else hello.AlreadyExists;",
        );
        compile(&control).expect("Boolean predicate control compiles");
        let errors = compile(&source).expect_err("effect receipt is not a predicate");
        assert_eq!(errors[0].kind, CompilerErrorKind::ExpectedPredicate);
        assert!(source[errors[0].span.start..errors[0].span.end]
            .contains("hello.createGreeting(input)"));
        assert!(!errors[0].message.contains("with `else`"));
    }
}

#[test]
fn effect_comparisons_report_type_mismatch_before_map_guidance() {
    for comparison in [
        "hello.createGreeting(input) == true",
        "true == hello.createGreeting(input)",
    ] {
        let source = SOURCE.replace(
            "  let receipt:",
            &format!("  require {comparison} else hello.AlreadyExists;\n  let receipt:"),
        );
        let control = source.replace(
            &format!("require {comparison} else hello.AlreadyExists;"),
            "require true == true else hello.AlreadyExists;",
        );
        compile(&control).expect("Boolean comparison control compiles");
        let errors = compile(&source).expect_err("effect receipt cannot compare to Boolean");
        assert_eq!(errors[0].kind, CompilerErrorKind::TypeMismatch);
        assert!(source[errors[0].span.start..errors[0].span.end]
            .contains("hello.createGreeting(input)"));
        assert!(!errors[0].message.contains("with `else`"));
    }
}

#[test]
fn effect_integer_comparisons_report_incompatibility_before_map_guidance() {
    for comparison in [
        "hello.createGreeting(input) == 1",
        "1 == hello.createGreeting(input)",
        "hello.createGreeting(input) == -1",
        "-1 == hello.createGreeting(input)",
    ] {
        let source = SOURCE.replace(
            "  let receipt:",
            &format!("  require {comparison} else hello.AlreadyExists;\n  let receipt:"),
        );
        let control = source.replace(
            &format!("require {comparison} else hello.AlreadyExists;"),
            "require 1u64 == 1 else hello.AlreadyExists;",
        );
        compile(&control).expect("integer comparison control compiles");
        let errors = compile(&source).expect_err("record receipt cannot compare to integer");
        assert_eq!(errors[0].kind, CompilerErrorKind::TypeMismatch);
        assert!(!errors[0].message.contains("with `else`"));
    }
}

#[test]
fn effect_field_selection_validates_the_exported_record_before_map_guidance() {
    for (value, expected) in [
        (
            "hello.createGreeting(input).missing",
            CompilerErrorKind::UnknownField,
        ),
        (
            "hello.createGreeting(input).key.missing",
            CompilerErrorKind::TypeMismatch,
        ),
    ] {
        let source = SOURCE.replace("key: receipt.key", &format!("key: {value}"));
        let errors = compile(&source).expect_err("invalid effect result selection");
        assert_eq!(errors[0].kind, expected, "{value}: {errors:?}");
        assert_eq!(&source[errors[0].span.start..errors[0].span.end], value);
        assert!(!errors[0].message.contains("with `else`"));
    }
    let source = SOURCE.replace("key: receipt.key", "key: hello.createGreeting(input).key");
    assert_eq!(
        compile(&source).expect_err("valid projection still needs map")[0].kind,
        CompilerErrorKind::EffectWithoutFailureMapping
    );
}

#[test]
fn effect_field_selection_preserves_predicate_and_comparison_requirements() {
    for (predicate, expected) in [
        (
            "hello.createGreeting(input).key",
            CompilerErrorKind::ExpectedPredicate,
        ),
        (
            "hello.createGreeting(input).key == 1",
            CompilerErrorKind::TypeMismatch,
        ),
        (
            "1 == hello.createGreeting(input).key",
            CompilerErrorKind::TypeMismatch,
        ),
    ] {
        let source = SOURCE.replace(
            "  let receipt:",
            &format!("  require {predicate} else hello.AlreadyExists;\n  let receipt:"),
        );
        let errors = compile(&source).expect_err("projected string cannot be this predicate");
        assert_eq!(errors[0].kind, expected, "{predicate}: {errors:?}");
        assert!(!errors[0].message.contains("with `else`"));
    }
}

#[test]
fn effect_operator_families_reject_before_mapping_guidance() {
    compile(SOURCE).expect("mapped authenticated effect control");
    for expression in [
        "hello.createGreeting(input) + \"x\"",
        "\"x\" + hello.createGreeting(input)",
        "len(hello.createGreeting(input))",
        "slice(hello.createGreeting(input), 0u64, 0u64)",
        "len(hello.createGreeting(input).key)",
        "slice(hello.createGreeting(input).key, 0u64, 0u64)",
    ] {
        let source = SOURCE.replace(
            "  let receipt:",
            &format!("  let invalid = {expression};\n  let receipt:"),
        );
        let errors = compile(&source)
            .expect_err("record/string effect output cannot satisfy operator family");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::TypeMismatch,
            "{expression}: {errors:?}"
        );
        assert!(!errors[0].message.contains("with `else`"));
    }
    let source = SOURCE.replace(
        "  let receipt:",
        "  let validFamily = hello.createGreeting(input).key + \"x\";\n  let receipt:",
    );
    assert_eq!(
        compile(&source).expect_err("valid string family still needs map")[0].kind,
        CompilerErrorKind::EffectWithoutFailureMapping
    );
}

#[test]
fn effect_subtraction_rejects_incompatible_outputs_before_mapping() {
    let control = SOURCE.replace(
        "  let receipt:",
        "  let difference = 1u64 - 0u64;\n  let receipt:",
    );
    compile(&control).expect("proven unsigned subtraction control");
    for expression in [
        "hello.createGreeting(input) - 0u64",
        "0u64 - hello.createGreeting(input)",
        "hello.createGreeting(input).key - 0",
        "0 - hello.createGreeting(input).key",
    ] {
        let source = SOURCE.replace(
            "  let receipt:",
            &format!("  let difference = {expression};\n  let receipt:"),
        );
        let errors = compile(&source).expect_err("non-unsigned effect cannot subtract");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::TypeMismatch,
            "{expression}: {errors:?}"
        );
        assert!(!errors[0].message.contains("with `else`"));
    }
}

#[test]
fn effect_concatenation_requires_matching_operand_families() {
    let source = SOURCE.replace("intent createGreeting(input: hello.CreateGreetingInput)", "type WrappedInput = { request: hello.CreateGreetingInput, bytes: Bytes<max=1>, };\nintent createGreeting(input: WrappedInput)")
        .replace("basis input.basis", "basis input.request.basis")
        .replace("hello.createGreeting(input)", "hello.createGreeting(input.request)")
        .replace("message: input.message", "message: input.request.message");
    compile(&source).expect("wrapped authenticated effect input control");
    for control in [
        "input.bytes + input.bytes",
        "input.request.key + input.request.key",
    ] {
        compile(&source.replace(
            "  let receipt:",
            &format!("  let joined = {control};\n  let receipt:"),
        ))
        .expect("matching family control");
    }
    for expression in [
        "hello.createGreeting(input.request).key + input.bytes",
        "input.bytes + hello.createGreeting(input.request).key",
    ] {
        let invalid = source.replace(
            "  let receipt:",
            &format!("  let joined = {expression};\n  let receipt:"),
        );
        let errors = compile(&invalid).expect_err("mixed families cannot concatenate");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::TypeMismatch,
            "{expression}: {errors:?}"
        );
    }
}

#[test]
fn nested_effect_operator_context_preserves_primary_type_error() {
    let plain = SOURCE.replace(
        "  let receipt:",
        "  let bad = len(input.key + \"x\");\n  let receipt:",
    );
    assert_eq!(
        compile(&plain).expect_err("string concatenation is not bytes")[0].kind,
        CompilerErrorKind::TypeMismatch
    );
    for expression in [
        "len(hello.createGreeting(input).key + \"x\")",
        "len(\"x\" + hello.createGreeting(input).key)",
        "len((hello.createGreeting(input).key + \"x\") + \"y\")",
        "slice(hello.createGreeting(input).key + \"x\", 0u64, 0u64)",
    ] {
        let source = SOURCE.replace(
            "  let receipt:",
            &format!("  let bad = {expression};\n  let receipt:"),
        );
        assert_eq!(
            compile(&source).expect_err("effect string concatenation is not bytes")[0].kind,
            CompilerErrorKind::TypeMismatch,
            "{expression}"
        );
    }
}

#[test]
fn utf8_effect_operand_keeps_primary_byte_type_error() {
    let valid = SOURCE.replace(
        "  let receipt:",
        "  let valid = isUtf8(b\"ok\");\n  let receipt:",
    );
    compile(&valid).expect("pure byte UTF-8 control compiles");
    let invalid = valid.replace("isUtf8(b\"ok\")", "isUtf8(hello.createGreeting(input))");
    let errors = compile(&invalid).expect_err("record effect output is not Bytes");
    let cause = &errors[0];
    assert_eq!(cause.kind, CompilerErrorKind::TypeMismatch);
    assert!(invalid[cause.span.start..cause.span.end].contains("hello.createGreeting(input)"));
    assert!(!cause.message.contains("with `else`"));
}

#[test]
fn conditional_effect_results_keep_primary_branch_type_errors() {
    let insert = |expression: &str| {
        SOURCE.replace(
            "  return {",
            &format!("  let chosen = {expression};\n  return {{"),
        )
    };
    compile(&insert("if true then receipt.key else input.key"))
        .expect("mapped compatible branch control");
    for expression in [
        "if true then hello.createGreeting(input).key else true",
        "if true then true else hello.createGreeting(input).key",
        "if true then hello.createGreeting(input).key else hello.createGreeting(input)",
        "if true then hello.createGreeting(input) else hello.createGreeting(input).key",
    ] {
        let errors = compile(&insert(expression)).expect_err("incompatible conditional");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::TypeMismatch,
            "{expression}: {errors:?}"
        );
        assert!(!errors[0].message.contains("with `else`"));
    }
    for expression in [
        "if true then hello.createGreeting(input).key else input.key",
        "if true then input.key else hello.createGreeting(input).key",
        "if true then hello.createGreeting(input).key else hello.createGreeting(input).key",
    ] {
        assert_eq!(
            compile(&insert(expression)).expect_err("compatible effects still need maps")[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
    }
}

#[test]
fn annotated_conditional_effect_results_keep_primary_branch_type_errors() {
    let insert = |expression: &str| {
        SOURCE.replace(
            "  return {",
            &format!("  let chosen: String<max=64> = {expression};\n  return {{"),
        )
    };
    compile(&insert("if true then receipt.key else input.key"))
        .expect("mapped compatible branch control");
    for expression in [
        "if true then hello.createGreeting(input).key else true",
        "if true then true else hello.createGreeting(input).key",
        "if true then hello.createGreeting(input).key else hello.createGreeting(input)",
        "if true then hello.createGreeting(input) else hello.createGreeting(input).key",
    ] {
        let errors = compile(&insert(expression)).expect_err("incompatible conditional");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::TypeMismatch,
            "{expression}: {errors:?}"
        );
        assert!(!errors[0].message.contains("with `else`"));
    }
    for expression in [
        "if true then hello.createGreeting(input).key else input.key",
        "if true then input.key else hello.createGreeting(input).key",
        "if true then hello.createGreeting(input).key else hello.createGreeting(input).key",
    ] {
        assert_eq!(
            compile(&insert(expression)).expect_err("compatible effects still need maps")[0].kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
    }
}

#[test]
fn concatenated_effect_results_keep_complete_output_bounds() {
    let insert = |bound: u64, expression: &str| {
        SOURCE.replace(
            "  return {",
            &format!("  let chosen: String<max={bound}> = {expression};\n  return {{"),
        )
    };
    compile(&insert(65, "receipt.key + \"x\"")).expect("mapped exact-bound concatenation control");
    for (bound, expression) in [
        (1, "hello.createGreeting(input).key + \"x\""),
        (64, "hello.createGreeting(input).key + \"x\""),
        (64, "\"x\" + hello.createGreeting(input).key"),
        (65, "(hello.createGreeting(input).key + \"x\") + \"y\""),
        (65, "\"y\" + (\"x\" + hello.createGreeting(input).key)"),
        (
            127,
            "hello.createGreeting(input).key + hello.createGreeting(input).key",
        ),
    ] {
        let errors =
            compile(&insert(bound, expression)).expect_err("complete output exceeds bound");
        assert_eq!(
            errors[0].kind,
            CompilerErrorKind::TypeMismatch,
            "{expression}: {errors:?}"
        );
        assert!(!errors[0].message.contains("with `else`"));
    }
    for (bound, expression) in [
        (65, "hello.createGreeting(input).key + \"x\""),
        (66, "(hello.createGreeting(input).key + \"x\") + \"y\""),
    ] {
        assert_eq!(
            compile(&insert(bound, expression)).expect_err("bounded effects still need maps")[0]
                .kind,
            CompilerErrorKind::EffectWithoutFailureMapping
        );
    }
}

#[test]
fn nested_effect_conditional_diagnostics_finish_within_deadline() {
    let mut expression = "input.key".to_owned();
    for _ in 0..20 {
        expression = format!("if true then hello.createGreeting(input).key else ({expression})");
    }
    let source = SOURCE.replace(
        "  return {",
        &format!("  let chosen = {expression};\n  return {{"),
    );
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || send.send(compile(&source)).unwrap());
    let result = receive
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("linear-size invalid source must not cause exponential diagnostic work");
    worker.join().unwrap();
    let errors = result.expect_err("bare effects still require maps");
    assert!(errors
        .iter()
        .all(|error| error.kind == CompilerErrorKind::EffectWithoutFailureMapping));
}
