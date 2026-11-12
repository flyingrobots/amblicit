//! Typed function failures, exercised through source parsing and compilation.
//!
//! Oracle: the "Typed Function Failures" section of
//! `docs/SPEC_edict-language-v1.md` (EDICT-LANG-FAIL-*). Expected coordinates,
//! Core forms and diagnostic kinds are written from that section, not read
//! back from the compiler.
use amblicit_syntax::ast::{Decl, Expr, Module, Stmt};
use amblicit_syntax::source_closure::{
    resolve_source_closure, source_module_digest, SourceClosureLimits,
};
use amblicit_syntax::source_linker::link_source_closure;
use amblicit_syntax::{
    compile_to_core, parse_module, CompilerContext, CompilerErrorKind, CoreBudget, CoreExpr,
    CoreModule, CoreNode, CorePredicate,
};

const TOO_SMALL: &str = "rope.example@1.ClosureTooSmall";
const CONFLICT: &str = "rope.example@1.ConflictingContent";

/// Two failures, a function that raises one, a function that propagates it
/// and raises the other, and an intent that maps both.
const BASE: &str = r#"package rope.example@1;
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
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let value = both(input.value, input.other) else {
    ClosureTooSmall => text.ClosureInsufficient,
    ConflictingContent => text.ContentIdentityMismatch({ observed: input.other }),
  };
  return { value };
}
"#;

fn context() -> CompilerContext {
    let budget = CoreBudget {
        max_steps: 100_000,
        max_allocated_bytes: u64::MAX,
        max_output_bytes: u64::MAX,
    };
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_operation_profile("text.replaceRange", "continuum.profile.read-only/v1")
        .with_budget("p.small", budget.clone())
        .with_budget("text.replaceRangeBudget", budget)
}

fn parse(source: &str) -> Module {
    parse_module(source).unwrap_or_else(|error| panic!("typed-failure syntax parses: {error:?}"))
}

fn compile(source: &str) -> CoreModule {
    compile_to_core(&parse(source), &context())
        .unwrap_or_else(|errors| panic!("typed-failure source compiles: {errors:#?}"))
}

/// Compile `source` and require a diagnostic of `kind` whose span starts at
/// the first occurrence of `anchor`.
fn refuse(source: &str, kind: CompilerErrorKind, anchor: &str) {
    let errors = compile_to_core(&parse(source), &context())
        .expect_err("the spec refuses this typed-failure program");
    let start = source
        .find(anchor)
        .unwrap_or_else(|| panic!("anchor `{anchor}` is in the source"));
    assert!(
        errors
            .iter()
            .any(|error| error.kind == kind && error.span.start == start),
        "expected {kind:?} at `{anchor}` ({start}), got {errors:#?}"
    );
}

fn function_failures<'a>(core: &'a CoreModule, name: &str) -> &'a [String] {
    &core.functions[name].failures
}

#[test]
fn failure_syntax_parses_into_declarations_clauses_and_expressions() {
    let module = parse(BASE);
    let failures: Vec<&str> = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Failure(failure) => Some(failure.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(failures, ["ClosureTooSmall", "ConflictingContent"]);
    let both = module
        .decls
        .iter()
        .find_map(|decl| match decl {
            Decl::Function(function) if function.name == "both" => Some(function),
            _ => None,
        })
        .expect("both is declared");
    assert_eq!(
        both.fails,
        [
            vec!["ConflictingContent".to_owned()],
            vec!["ClosureTooSmall".to_owned()]
        ]
    );
    let Stmt::Let { value, .. } = &both.body.stmts[0] else {
        panic!("first statement binds the expected value")
    };
    let Expr::Expect { call, .. } = value else {
        panic!("`expect checked(value)` is an expect expression, got {value:?}")
    };
    assert!(matches!(call.as_ref(), Expr::Call { .. }));
    let Stmt::Return {
        value: Expr::If { els, .. },
        ..
    } = &both.body.stmts[1]
    else {
        panic!("second statement returns a conditional")
    };
    assert!(
        matches!(els.as_ref(), Expr::Fail { failure, .. } if failure == &["ConflictingContent"])
    );
}

#[test]
fn failure_words_remain_ordinary_identifiers_elsewhere() {
    let source = r"package words.example@1;
type Input = { fail: U64, expect: U64, failure: U64, fails: U64, };
type Output = { value: U64, };
fn fails(failure: U64) -> U64 { return failure; }
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {
  let fail = input.fail;
  let expect = fails(input.expect);
  return { value: expect };
}
";
    let core = compile(source);
    assert!(core.functions["fails"].failures.is_empty());
    assert!(!core
        .required_core_capabilities
        .iter()
        .any(|capability| capability == "core.failure/v1"));
}

#[test]
fn failure_declarations_compile_to_sorted_module_coordinates() {
    let core = compile(BASE);
    assert_eq!(function_failures(&core, "checked"), [TOO_SMALL]);
    // Declared as `ConflictingContent, ClosureTooSmall`; Core sorts them.
    assert_eq!(function_failures(&core, "both"), [TOO_SMALL, CONFLICT]);
    assert!(core
        .required_core_capabilities
        .iter()
        .any(|capability| capability == "core.failure/v1"));
    let CoreExpr::If { else_value, .. } = &core.functions["checked"].body.result else {
        panic!("checked returns a conditional")
    };
    assert_eq!(
        else_value.as_ref(),
        &CoreExpr::Fail {
            failure: TOO_SMALL.to_owned()
        }
    );
}

#[test]
fn unknown_repeated_or_ambiguous_failure_names_refuse() {
    refuse(
        &BASE.replace("fails ClosureTooSmall {", "fails Missing {"),
        CompilerErrorKind::UnknownFailure,
        "Missing",
    );
    refuse(
        &BASE.replace(
            "else fail ClosureTooSmall;",
            "else fail Missing;",
        ),
        CompilerErrorKind::UnknownFailure,
        "fail Missing",
    );
    refuse(
        &BASE.replace(
            "fn checked(value: U64) -> U64 fails ClosureTooSmall {",
            "fn checked(value: U64) -> U64 fails ClosureTooSmall, ClosureTooSmall {",
        ),
        CompilerErrorKind::UnsupportedSourceShape,
        "fn checked",
    );
    refuse(
        &BASE.replace(
            "failure ConflictingContent;",
            "failure ConflictingContent;\nfailure ConflictingContent; // repeated",
        ),
        CompilerErrorKind::UnsupportedSourceShape,
        "failure ConflictingContent; // repeated",
    );
}

#[test]
fn declared_failure_set_is_exactly_what_the_body_produces() {
    let source = BASE.replace(
        "intent evaluate",
        r"type Choice = variant { Small(U64), Large(U64), };
fn pick(choice: Choice) -> U64 fails ClosureTooSmall {
  let bounded: U64 = if true then 1u64 else fail ClosureTooSmall;
  return match choice {
    Small(value) => value + bounded,
    Large(value) => fail ClosureTooSmall,
  };
}
intent evaluate",
    );
    let core = compile(&source);
    assert_eq!(function_failures(&core, "pick"), [TOO_SMALL]);
    let binding = &core.functions["pick"].body.bindings[0];
    let CoreExpr::If { else_value, .. } = &binding.value else {
        panic!("annotated binding keeps its conditional")
    };
    assert!(matches!(else_value.as_ref(), CoreExpr::Fail { .. }));
    let CoreExpr::Match { arms, .. } = &core.functions["pick"].body.result else {
        panic!("pick returns a match")
    };
    assert_eq!(
        arms[1].body,
        CoreExpr::Fail {
            failure: TOO_SMALL.to_owned()
        }
    );
}

#[test]
fn undeclared_and_unraised_failures_refuse() {
    // `fail` outside the declared set.
    refuse(
        &BASE.replace(
            "fn checked(value: U64) -> U64 fails ClosureTooSmall {",
            "fn checked(value: U64) -> U64 fails ConflictingContent {",
        ),
        CompilerErrorKind::UndeclaredFailure,
        "fail ClosureTooSmall",
    );
    // A declared failure that the body never produces.
    refuse(
        &BASE.replace(
            "fn checked(value: U64) -> U64 fails ClosureTooSmall {",
            "fn checked(value: U64) -> U64 fails ClosureTooSmall, ConflictingContent {",
        ),
        CompilerErrorKind::UnraisedFailure,
        "fn checked",
    );
    // `expect` widening the enclosing set.
    refuse(
        &BASE.replace(
            "fn both(value: U64, other: U64) -> U64 fails ConflictingContent, ClosureTooSmall {",
            "fn both(value: U64, other: U64) -> U64 fails ConflictingContent {",
        ),
        CompilerErrorKind::UndeclaredFailure,
        "expect checked(value)",
    );
    // `fail` with no expected type.
    refuse(
        &BASE.replace(
            "return if value < 10u64 then value else fail ClosureTooSmall;",
            "let lost = fail ClosureTooSmall;\n  return value;",
        ),
        CompilerErrorKind::TypeMismatch,
        "fail ClosureTooSmall",
    );
    // `fail` in an intent.
    refuse(
        &BASE.replace("return { value };", "return { value: fail ClosureTooSmall };"),
        CompilerErrorKind::UndeclaredFailure,
        "fail ClosureTooSmall }",
    );
}

#[test]
fn expect_propagates_callee_failures_through_nested_calls() {
    let source = BASE.replace(
        "intent evaluate",
        r"type Pair = { left: U64, right: U64, };
fn retain(value: U64) -> U64 { return value; }
fn pair(value: U64) -> Pair fails ClosureTooSmall {
  return { left: expect checked(value), right: retain(expect checked(value)) };
}
intent evaluate",
    );
    let core = compile(&source);
    assert_eq!(function_failures(&core, "pair"), [TOO_SMALL]);
    let CoreExpr::Record { fields } = &core.functions["pair"].body.result else {
        panic!("pair returns a record")
    };
    // `expect` leaves an ordinary call in Core.
    let checked = "rope.example@1.checked";
    assert!(matches!(&fields["left"], CoreExpr::Call { callee, .. } if callee == checked));
    let CoreExpr::Call { args, .. } = &fields["right"] else {
        panic!("right calls retain")
    };
    assert!(matches!(&args[0], CoreExpr::Call { callee, .. } if callee == checked));
}

#[test]
fn failing_calls_without_expect_or_mapping_refuse() {
    // A failing call in a function body without `expect`.
    refuse(
        &BASE.replace("let first = expect checked(value);", "let first = checked(value);"),
        CompilerErrorKind::UnhandledFailure,
        "checked(value)",
    );
    // A failing call inside an intent expression.
    refuse(
        &BASE.replace("return { value };", "return { value: checked(value) };"),
        CompilerErrorKind::UnhandledFailure,
        "checked(value) }",
    );
    // `expect` in an intent.
    refuse(
        &BASE.replace("return { value };", "return { value: expect checked(value) };"),
        CompilerErrorKind::UnhandledFailure,
        "expect checked(value) }",
    );
    // `expect` on a call that declares no failures.
    refuse(
        &BASE.replace(
            "intent evaluate",
            "fn retain(value: U64) -> U64 { return value; }\nfn wrap(value: U64) -> U64 fails ClosureTooSmall { let kept = expect retain(value); return expect checked(kept); }\nintent evaluate",
        ),
        CompilerErrorKind::UnsupportedSourceShape,
        "expect retain(value)",
    );
    // A failing call as an argument of a mapped call.
    refuse(
        &BASE.replace(
            "both(input.value, input.other) else",
            "both(checked(input.value), input.other) else",
        ),
        CompilerErrorKind::UnhandledFailure,
        "checked(input.value)",
    );
    // A failing call in a `require` predicate.
    refuse(
        &BASE.replace(
            "return { value };",
            "require checked(value) == value else text.Rejected;\n  return { value };",
        ),
        CompilerErrorKind::UnhandledFailure,
        "checked(value) ==",
    );
    // A failing `fold` step.
    refuse(
        &BASE.replace(
            "intent evaluate",
            "fn run(value: U64) -> U64 { return fold(value, checked, 2u64); }\nintent evaluate",
        ),
        CompilerErrorKind::UnhandledFailure,
        "fold(value, checked, 2u64)",
    );
}

#[test]
fn intent_maps_every_failure_to_a_terminal_obstruction() {
    let core = compile(BASE);
    let CoreNode::Attempt {
        binding,
        value,
        failure_map,
    } = &core.intents["evaluate"].body.nodes[0]
    else {
        panic!("the mapped let is an attempt node")
    };
    assert_eq!(binding.ty, "U64");
    assert!(matches!(value, CoreExpr::Call { callee, .. } if callee == "rope.example@1.both"));
    assert_eq!(
        failure_map.keys().map(String::as_str).collect::<Vec<_>>(),
        [TOO_SMALL, CONFLICT]
    );
    assert_eq!(failure_map[TOO_SMALL].kind, "text.ClosureInsufficient");
    assert!(failure_map[TOO_SMALL].payload.is_empty());
    assert_eq!(failure_map[CONFLICT].kind, "text.ContentIdentityMismatch");
    assert!(matches!(
        &failure_map[CONFLICT].payload["observed"],
        CoreExpr::Field { field, .. } if field == "other"
    ));
}

#[test]
fn one_failure_shorthand_maps_to_a_single_arm() {
    let source = BASE.replace(
        "return { value };",
        "let small = checked(value) else text.ClosureInsufficient;\n  return { value: small };",
    );
    let core = compile(&source);
    let CoreNode::Attempt { failure_map, .. } = &core.intents["evaluate"].body.nodes[1] else {
        panic!("the shorthand let is an attempt node")
    };
    assert_eq!(
        failure_map.keys().map(String::as_str).collect::<Vec<_>>(),
        [TOO_SMALL]
    );
    assert_eq!(failure_map[TOO_SMALL].kind, "text.ClosureInsufficient");
}

#[test]
fn incomplete_unknown_repeated_and_binding_failure_maps_refuse() {
    refuse(
        &BASE.replace(
            "    ConflictingContent => text.ContentIdentityMismatch({ observed: input.other }),\n",
            "",
        ),
        CompilerErrorKind::IncompleteFailureMapping,
        "let value",
    );
    refuse(
        &BASE.replace(
            "ConflictingContent => text.ContentIdentityMismatch",
            "Unrelated => text.ContentIdentityMismatch",
        ),
        CompilerErrorKind::UnknownFailure,
        "Unrelated",
    );
    refuse(
        &BASE.replace(
            "ConflictingContent => text.ContentIdentityMismatch",
            "ClosureTooSmall => text.Again,\n    ConflictingContent => text.ContentIdentityMismatch",
        ),
        CompilerErrorKind::DuplicateObstructionFailure,
        "ClosureTooSmall => text.Again",
    );
    refuse(
        &BASE.replace(
            "ClosureTooSmall => text.ClosureInsufficient",
            "ClosureTooSmall(lost) => text.ClosureInsufficient",
        ),
        CompilerErrorKind::UnsupportedSourceShape,
        "ClosureTooSmall(lost)",
    );
    refuse(
        &BASE.replace(
            "else {\n    ClosureTooSmall => text.ClosureInsufficient,\n    ConflictingContent => text.ContentIdentityMismatch({ observed: input.other }),\n  };",
            "else text.ClosureInsufficient;",
        ),
        CompilerErrorKind::IncompleteFailureMapping,
        "let value",
    );
    refuse(
        &BASE.replace(
            "return { value };",
            "let kept = retain(value) else { ClosureTooSmall => text.Dead };\n  return { value: kept };",
        )
        .replace(
            "intent evaluate",
            "fn retain(value: U64) -> U64 { return value; }\nintent evaluate",
        ),
        CompilerErrorKind::UnknownFailure,
        "ClosureTooSmall => text.Dead",
    );
}

#[test]
fn linked_modules_keep_original_failure_coordinates() {
    let library = "package library.rope@1;\nfailure ClosureTooSmall;\nfn checked(value: U64) -> U64 fails ClosureTooSmall { return if value < 10u64 then value else fail ClosureTooSmall; }";
    let digest = source_module_digest(library).expect("library digest");
    let root = format!(
        "package app.example@1;\nuse module library.rope@1 digest \"{digest}\" as rope;\nfailure Local;\nfn wrap(value: U64) -> U64 fails rope.ClosureTooSmall {{ return expect rope.checked(value); }}\ntype Input = {{ value: U64, }};\ntype Output = {{ value: U64, }};\nintent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{\n  let value = rope.checked(input.value) else {{ ClosureTooSmall => text.ClosureInsufficient }};\n  let wrapped = wrap(value) else text.ClosureInsufficient;\n  return {{ value: wrapped }};\n}}"
    );
    let closure = resolve_source_closure(&root, &[library], SourceClosureLimits::default())
        .expect("closure resolves");
    let linked = link_source_closure(&closure).expect("closure links");
    let core = linked
        .compile(&context())
        .unwrap_or_else(|errors| panic!("linked closure compiles: {errors:#?}"));
    let original = "library.rope@1.ClosureTooSmall";
    for node in &core.intents["evaluate"].body.nodes {
        let CoreNode::Attempt { failure_map, .. } = node else {
            panic!("both lets are attempts")
        };
        assert_eq!(
            failure_map.keys().map(String::as_str).collect::<Vec<_>>(),
            [original]
        );
    }
    for function in core.functions.values() {
        assert_eq!(function.failures, [original]);
    }
    // `failure Local;` is declared but no function uses it, which is legal.
    // Two failures with the same bare name in one set are ambiguous.
    let ambiguous = root.replace(
        "failure Local;",
        "failure ClosureTooSmall;\nfn both(value: U64) -> U64 fails ClosureTooSmall, rope.ClosureTooSmall { let kept = expect rope.checked(value); return if kept == 0u64 then fail ClosureTooSmall else kept; }",
    );
    let closure = resolve_source_closure(&ambiguous, &[library], SourceClosureLimits::default())
        .expect("closure resolves");
    let linked = link_source_closure(&closure).expect("closure links");
    let errors = linked
        .compile(&context())
        .expect_err("ambiguous bare failure names refuse");
    assert!(
        errors
            .iter()
            .any(|error| error.error.kind == CompilerErrorKind::UnsupportedSourceShape
                && error.coordinate == "app.example@1"
                && error.error.span.start == ambiguous.find("fn both").unwrap_or(usize::MAX)),
        "{errors:#?}"
    );
}

#[test]
fn reconciled_jedit_typed_failure_reproducer_compiles() {
    let source = include_str!("../../../fixtures/lang/functions/pure-typed-failure.amblicit");
    let core = compile(source);
    let coordinate = "jedit.iterative.pure_typed_failure@1.Underflow";
    assert_eq!(core.functions["distance"].failures, [coordinate]);
    let CoreExpr::If {
        predicate,
        else_value,
        ..
    } = &core.functions["distance"].body.result
    else {
        panic!("distance returns a guarded conditional")
    };
    assert!(matches!(predicate.as_ref(), CorePredicate::Compare { .. }));
    assert_eq!(
        else_value.as_ref(),
        &CoreExpr::Fail {
            failure: coordinate.to_owned()
        }
    );
    let CoreNode::Attempt { failure_map, .. } = &core.intents["evaluate"].body.nodes[0] else {
        panic!("the reproducer maps its failure")
    };
    assert_eq!(failure_map[coordinate].kind, "text.RangeInverted");
}
