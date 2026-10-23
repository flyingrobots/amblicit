//! Authenticated source-module imports keep dependency authority explicit.
use amblicit_syntax::{
    compile_to_core, parse_module, CompilerContext, CompilerErrorKind, ParseErrorKind,
};
use amblicit_syntax::{highlight_source, HighlightRole};

const PIN: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

#[test]
fn source_module_import_highlights_the_contextual_keyword() {
    let source =
        format!("package app.example@1; use module example.metrics@2 digest \"{PIN}\" as metrics;");
    let tokens = highlight_source(&source).unwrap();
    let module = tokens
        .iter()
        .find(|token| token.lexeme(&source) == "module")
        .unwrap();
    assert_eq!(module.role, HighlightRole::Keyword);
}

#[test]
fn source_module_import_preserves_exact_package_digest_and_alias() {
    let source =
        format!("package app.example@1; use module example.metrics@2 digest \"{PIN}\" as metrics;");
    let module = parse_module(&source).expect("digest-bound source module syntax");
    let import = &module.imports[0];
    let package = import.package.as_ref().expect("module coordinate");
    assert_eq!(package.path, ["example", "metrics"]);
    assert_eq!(package.version, "2");
    assert_eq!(import.digest.as_deref(), Some(PIN));
    assert_eq!(import.alias, "metrics");
    assert_eq!(&source[import.span.start..import.span.end], &source[23..]);
}

#[test]
fn source_module_import_requires_a_digest_clause() {
    let source = "package app.example@1; use module example.metrics@2 as metrics;";
    let failure = parse_module(source).unwrap_err();
    assert_eq!(failure.kind, ParseErrorKind::ExpectedKeyword);
    assert_eq!(&source[failure.span.start..failure.span.end], "as");
}

#[test]
fn unresolved_source_import_cannot_silently_compile_even_when_unused() {
    let source =
        format!("package app.example@1; use module example.metrics@2 digest \"{PIN}\" as metrics;");
    let module = parse_module(&source).expect("module syntax");
    let failures = compile_to_core(&module, &CompilerContext::new()).unwrap_err();
    assert!(failures
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));
    assert!(failures.iter().all(|error| error.span.end <= source.len()));
}
