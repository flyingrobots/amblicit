//! Actual source imports compile without leaking module-local names or authority.
use edict_syntax::source_closure::{
    resolve_source_closure, source_module_digest, SourceClosureLimits,
};
use edict_syntax::source_linker::{link_source_closure, SourceLinkErrorKind};
use edict_syntax::{
    digest_core_module, lower_to_target_ir, CompilerContext, CompilerErrorKind, CoreBudget,
    CoreExpr, CoreModule, ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};

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

fn import(package: &str, source: &str, alias: &str) -> String {
    format!(
        "use module {package} digest \"{}\" as {alias};",
        source_module_digest(source).unwrap()
    )
}

fn root(imports: &str, declarations: &str, result: &str) -> String {
    format!("package app.example@1; {imports}\n{declarations}\ntype Input = {{ value: U64, }}; type Output = {{ value: U64, }};\nintent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{ return {{ value: {result} }}; }}")
}

fn compile(source: &str, dependencies: &[&str]) -> CoreModule {
    let closure =
        resolve_source_closure(source, dependencies, SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).expect("authored lexical imports link");
    assert!(linked
        .module()
        .imports
        .iter()
        .all(|import| import.kind != edict_syntax::ast::ImportKind::Module));
    linked.compile(&context()).expect("linked modules compile")
}

fn result_call(core: &CoreModule) -> &str {
    let CoreExpr::Record { fields } = &core.intents["evaluate"].body.result else {
        panic!("record result")
    };
    let CoreExpr::Call {
        callee,
        args,
        type_args,
    } = &fields["value"]
    else {
        panic!("authored helper call")
    };
    assert_eq!(args.len(), 1);
    assert!(type_args.is_empty());
    callee
}

#[test]
fn authenticated_module_type_and_argument_taking_helper_compile_and_lower() {
    let library = "package library.metrics@1; type Count = U64; fn retain(value: Count) -> Count { return value; }";
    let source = root(
        &import("library.metrics@1", library, "metrics"),
        "type Imported = metrics.Count;",
        "metrics.retain(input.value)",
    );
    let core = compile(&source, &[library]);
    let callee = result_call(&core);
    let name = callee.strip_prefix("app.example@1.").unwrap();
    let function = &core.functions[name];
    assert_eq!(function.params.len(), 1);
    assert!(matches!(&function.body.result, CoreExpr::Local { .. }));
    let target = lower_to_target_ir(
        &core,
        &TargetIrLoweringFacts {
            target_profile: ResourceRef {
                coordinate: "echo.dpo@1".into(),
                digest: Some(format!("sha256:{}", "1".repeat(64))),
            },
            target_ir_domain: "echo.span-ir/v1".into(),
            operation_profiles: vec!["continuum.profile.read-only/v1".into()],
            obstruction_coordinates: vec![],
            effect_lowerings: vec![],
            effect_signatures: vec![],
            pure_functions: vec![],
        },
    );
    assert_eq!(target.status, TargetLoweringStatus::Lowered, "{target:?}");
}

#[test]
fn transitive_module_calls_and_duplicate_local_names_stay_isolated() {
    let leaf = "package library.leaf@1; type Count = U64; fn retain(value: Count) -> Count { return value; }";
    let middle = format!("package library.middle@1; {} type Count = U64; fn retain(value: Count) -> Count {{ return base.retain(value); }}", import("library.leaf@1", leaf, "base"));
    let source = root(
        &import("library.middle@1", &middle, "metrics"),
        "fn retain(value: U64) -> U64 { return 99u64; }",
        "metrics.retain(input.value)",
    );
    let core = compile(&source, &[leaf, &middle]);
    assert_eq!(core.functions.len(), 3);
    let middle_name = result_call(&core).strip_prefix("app.example@1.").unwrap();
    let CoreExpr::Call { callee, .. } = &core.functions[middle_name].body.result else {
        panic!("transitive call")
    };
    let leaf_name = callee.strip_prefix("app.example@1.").unwrap();
    assert_ne!(middle_name, leaf_name);
    assert!(matches!(
        &core.functions[leaf_name].body.result,
        CoreExpr::Local { .. }
    ));
    let reordered = compile(&source, &[&middle, leaf]);
    assert_eq!(
        digest_core_module(&core).unwrap(),
        digest_core_module(&reordered).unwrap()
    );
}

#[test]
fn library_local_names_do_not_capture_or_conflict_with_application_declarations() {
    let library = "package library.metrics@1; fn retain(applicationOnly: U64) -> U64 { let local = applicationOnly; return local; }";
    let source = root(
        &import("library.metrics@1", library, "metrics"),
        "fn applicationOnly(value: U64) -> U64 { return 99u64; } type Local = Bool;",
        "metrics.retain(input.value)",
    );
    let core = compile(&source, &[library]);
    assert_eq!(core.functions.len(), 2);
    let library_name = result_call(&core).strip_prefix("app.example@1.").unwrap();
    assert_eq!(core.functions[library_name].body.bindings.len(), 1);
}

#[test]
fn undefined_library_functions_and_types_cannot_capture_application_authority() {
    for library in [
        "package library.metrics@1; fn retain(value: U64) -> U64 { return applicationOnly(value); }",
        "package library.metrics@1; fn retain(value: ApplicationOnly) -> U64 { return value; }",
    ] {
        let source = root(&import("library.metrics@1", library, "metrics"), "type ApplicationOnly = U64; fn applicationOnly(value: U64) -> U64 { return value; }", "metrics.retain(input.value)");
        let closure = resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
        let failure = link_source_closure(&closure).unwrap_err();
        assert_eq!(failure.kind, SourceLinkErrorKind::UnresolvedName);
        assert_eq!(failure.coordinate, "library.metrics@1");
        assert!(failure.span.end <= library.len());
    }
}

#[test]
fn imported_aliases_do_not_implicitly_reexport_transitive_names() {
    let leaf = "package library.leaf@1; fn retain(value: U64) -> U64 { return value; }";
    let middle = format!(
        "package library.middle@1; {}",
        import("library.leaf@1", leaf, "base")
    );
    let source = root(
        &import("library.middle@1", &middle, "metrics"),
        "",
        "metrics.base.retain(input.value)",
    );
    let closure =
        resolve_source_closure(&source, &[leaf, &middle], SourceClosureLimits::default()).unwrap();
    let failure = link_source_closure(&closure).unwrap_err();
    assert_eq!(failure.kind, SourceLinkErrorKind::UnknownExport);
    assert_eq!(failure.coordinate, "app.example@1");
}

#[test]
fn invalid_unused_library_definitions_report_their_original_source_location() {
    let library = "package library.metrics@1; fn unused(value: U64) -> Bool { return value; }";
    let source = root(
        &import("library.metrics@1", library, "metrics"),
        "",
        "input.value",
    );
    let closure =
        resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).unwrap();
    let failures = linked.compile(&context()).unwrap_err();
    let failure = failures
        .iter()
        .find(|failure| failure.error.kind == CompilerErrorKind::TypeMismatch)
        .expect("unused invalid definition cannot disappear");
    assert_eq!(failure.coordinate, "library.metrics@1");
    assert!(failure.error.span.end <= library.len());
}

#[test]
fn imported_intent_entries_are_refused_instead_of_silently_dropped() {
    let library = root("", "", "input.value").replace("app.example@1", "library.metrics@1");
    let source = root(
        &import("library.metrics@1", &library, "metrics"),
        "",
        "input.value",
    );
    let closure =
        resolve_source_closure(&source, &[&library], SourceClosureLimits::default()).unwrap();
    let failure = link_source_closure(&closure).unwrap_err();
    assert_eq!(failure.kind, SourceLinkErrorKind::ImportedIntent);
    assert_eq!(failure.coordinate, "library.metrics@1");
}
