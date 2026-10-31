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
