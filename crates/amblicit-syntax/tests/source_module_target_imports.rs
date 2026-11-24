//! Flattening a source-module closure keeps one import per target (#342).
//!
//! Oracle: Echo's identity check accepts exactly one `target` import per
//! target profile, so identical imports from several modules must merge, and
//! two pins of one target with different digests must be refused.
use amblicit_syntax::source_closure::{
    resolve_source_closure, source_module_digest, SourceClosureLimits,
};
use amblicit_syntax::source_linker::{link_source_closure, SourceLinkErrorKind};
use amblicit_syntax::{
    encode_core_module, CompilerContext, CoreBudget, CoreImportKind, CoreModule,
};

const PIN: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const OTHER: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 100_000,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        )
}

fn library(name: &str, pin: &str) -> String {
    format!(
        "package library.{name}@1; use target echo.dpo@1 digest \"{pin}\" as echo; fn retain(value: U64) -> U64 {{ return value; }}"
    )
}

fn root(libraries: &[(&str, &str)], pin: &str) -> String {
    let imports = libraries
        .iter()
        .map(|(name, source)| {
            let digest = source_module_digest(source).unwrap();
            format!("use module library.{name}@1 digest \"{digest}\" as {name};")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let first = libraries[0].0;
    format!(
        "package app.example@1; use target echo.dpo@1 digest \"{pin}\" as echo; {imports}
type Input = {{ value: U64, }}; type Output = {{ value: U64, }};
intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{
  let evaluated = {first}.retain(input.value); return {{ value: evaluated }};
}}"
    )
}

fn compile(root: &str, dependencies: &[&str]) -> CoreModule {
    let closure = resolve_source_closure(root, dependencies, SourceClosureLimits::default())
        .expect("closure resolves");
    link_source_closure(&closure)
        .expect("identical target imports link")
        .compile(&context())
        .expect("linked modules compile")
}

fn target_imports(core: &CoreModule) -> usize {
    core.imports
        .iter()
        .filter(|import| import.kind == CoreImportKind::Target)
        .count()
}

#[test]
fn identical_target_imports_merge_into_one() {
    let (alpha, beta) = (library("alpha", PIN), library("beta", PIN));
    let source = root(&[("alpha", &alpha), ("beta", &beta)], PIN);
    let core = compile(&source, &[&alpha, &beta]);
    assert_eq!(target_imports(&core), 1, "{:?}", core.imports);
}

#[test]
fn merged_core_bytes_do_not_depend_on_dependency_order() {
    let (alpha, beta) = (library("alpha", PIN), library("beta", PIN));
    let source = root(&[("alpha", &alpha), ("beta", &beta)], PIN);
    let forward = encode_core_module(&compile(&source, &[&alpha, &beta])).unwrap();
    let reverse = encode_core_module(&compile(&source, &[&beta, &alpha])).unwrap();
    assert_eq!(forward, reverse);
}

#[test]
fn different_digests_for_one_target_are_refused() {
    let alpha = library("alpha", OTHER);
    let source = root(&[("alpha", &alpha)], PIN);
    let closure = resolve_source_closure(&source, &[&alpha], SourceClosureLimits::default())
        .expect("closure resolves");
    let error = link_source_closure(&closure).expect_err("conflicting target pins refuse");
    assert_eq!(
        error.kind,
        SourceLinkErrorKind::ConflictingTargetImport,
        "{error:?}"
    );
}
