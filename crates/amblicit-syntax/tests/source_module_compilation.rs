//! Actual source imports compile without leaking module-local names or authority.
use amblicit_syntax::source_closure::{
    resolve_source_closure, source_module_digest, SourceClosureLimits,
};
use amblicit_syntax::source_linker::{link_source_closure, SourceLinkErrorKind};
use amblicit_syntax::{
    digest_core_module, lower_to_target_ir, CompilerContext, CompilerErrorKind, CoreBudget,
    CoreExpr, CoreModule, CoreNode, CoreType, PureFunctionFact, ResourceRef, TargetIrLoweringFacts,
    TargetLoweringStatus, TypeShapeFact,
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
    format!("package app.example@1; {imports}\n{declarations}\ntype Input = {{ value: U64, }}; type Output = {{ value: U64, }};\nintent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{ let evaluated = {result}; return {{ value: evaluated }}; }}")
}

fn compile(source: &str, dependencies: &[&str]) -> CoreModule {
    let closure =
        resolve_source_closure(source, dependencies, SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).expect("authored lexical imports link");
    assert!(linked
        .module()
        .imports
        .iter()
        .all(|import| import.kind != amblicit_syntax::ast::ImportKind::Module));
    linked.compile(&context()).expect("linked modules compile")
}

fn result_call(core: &CoreModule) -> &str {
    let CoreExpr::Record { fields } = &core.intents["evaluate"].body.result else {
        panic!("record result")
    };
    let CoreNode::Let {
        binding,
        value:
            CoreExpr::Call {
                callee,
                args,
                type_args,
            },
    } = &core.intents["evaluate"].body.nodes[0]
    else {
        panic!("authored helper binding")
    };
    assert_eq!(
        &fields["value"],
        &CoreExpr::Local {
            reference: binding.clone()
        }
    );
    assert_eq!(args.len(), 1);
    assert_eq!(type_args.as_slice(), &[] as &[String]);
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
    assert_eq!(
        &library[failure.error.span.start..failure.error.span.end],
        "return value;"
    );
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

#[test]
fn module_hygiene_cannot_launder_reserved_source_type_names() {
    for (coordinate, library, declarations) in [
        (
            "library.metrics@1",
            "package library.metrics@1; type Bool = { value: U64, };",
            "",
        ),
        (
            "app.example@1",
            "package library.metrics@1;",
            "type U64 = { value: Bool, };",
        ),
    ] {
        let source = root(
            &import("library.metrics@1", library, "metrics"),
            declarations,
            "input.value",
        );
        let closure =
            resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
        let failure = link_source_closure(&closure).unwrap_err();
        assert_eq!(failure.kind, SourceLinkErrorKind::ReservedTypeIdentity);
        assert_eq!(failure.coordinate, coordinate);
    }
}

#[test]
fn module_hygiene_does_not_hide_original_source_function_authority_collisions() {
    let pin = format!("sha256:{}", "1".repeat(64));
    let library = format!("package library.metrics@1; use lawpack library.metrics@1 digest \"{pin}\" as authority; fn retain(value: U64) -> U64 {{ return value; }}");
    let source = root(
        &import("library.metrics@1", &library, "metrics"),
        "",
        "metrics.retain(input.value)",
    );
    let closure =
        resolve_source_closure(&source, &[&library], SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).unwrap();
    let context = context().with_pure_function(
        "authority.retain",
        PureFunctionFact {
            lawpack: ResourceRef {
                coordinate: "library.metrics@1".into(),
                digest: Some(pin),
            },
            coordinate: "library.metrics@1.retain".into(),
            type_parameters: vec![],
            parameter_types: vec!["U64".into()],
            return_type: "U64".into(),
            cost_template: "library.metrics@1.small".into(),
        },
    );
    let failures = linked.compile(&context).unwrap_err();
    assert!(failures
        .iter()
        .any(|failure| failure.coordinate == "library.metrics@1"
            && failure.error.kind == CompilerErrorKind::UnresolvedFunction));
}

#[test]
fn exported_record_type_identity_is_owned_by_its_source_module_across_applications() {
    let library = "package library.metrics@1; type Fact = { value: U64, }; fn retain(value: Fact) -> Fact { return value; }";
    let source = format!("package app.first@1; {} type Input = {{ value: metrics.Fact, }}; type Output = {{ value: metrics.Fact, }}; intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{ let evaluated = metrics.retain(input.value); return {{ value: evaluated }}; }}", import("library.metrics@1", library, "metrics"));
    let first = compile(&source, &[library]);
    let second = compile(&source.replace("app.first@1", "app.second@1"), &[library]);
    for core in [&first, &second] {
        assert!(core.types.contains_key("library.metrics@1.Fact"));
        let CoreType::Record { fields } = &core.types["Input"] else {
            panic!("application input record")
        };
        assert_eq!(fields["value"], "library.metrics@1.Fact");
        assert_eq!(
            core.functions.values().next().unwrap().return_type,
            "library.metrics@1.Fact"
        );
    }
    assert_eq!(
        first.types["library.metrics@1.Fact"],
        second.types["library.metrics@1.Fact"]
    );
}

#[test]
fn source_module_record_definitions_cannot_redeclare_imported_nominal_authority() {
    let pin = format!("sha256:{}", "1".repeat(64));
    let library = format!("package library.metrics@1; use lawpack library.metrics@1 digest \"{pin}\" as authority; type Fact = {{ value: U64, }};");
    let source = format!("package app.example@1; {} type Input = {{ value: metrics.Fact, }}; type Output = {{ value: metrics.Fact, }}; intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{ let evaluated = input.value; return {{ value: evaluated }}; }}", import("library.metrics@1", &library, "metrics"));
    let closure =
        resolve_source_closure(&source, &[&library], SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).unwrap();
    let context = context().with_type_shape(TypeShapeFact {
        lawpack: ResourceRef {
            coordinate: "library.metrics@1".into(),
            digest: Some(pin),
        },
        coordinate: "library.metrics@1.Fact".into(),
        definition: "Nominal<Bytes<exact=32>>".into(),
    });
    let failures = linked.compile(&context).unwrap_err();
    assert!(failures
        .iter()
        .any(|failure| failure.coordinate == "library.metrics@1"
            && failure.error.kind == CompilerErrorKind::InvalidCoreTypeIntegrity));
}

#[test]
fn source_identifiers_matching_old_compiler_names_remain_ordinary_local_bindings() {
    let library = "package library.metrics@1; fn retain(value: U64) -> U64 { return value; }";
    let first = compile(
        &root(
            &import("library.metrics@1", library, "metrics"),
            "",
            "metrics.retain(input.value)",
        ),
        &[library],
    );
    let old_symbol = result_call(&first).strip_prefix("app.example@1.").unwrap();
    let changed = format!(
        "package library.metrics@1; fn retain({old_symbol}: U64) -> U64 {{ return {old_symbol}; }}"
    );
    let second = compile(
        &root(
            &import("library.metrics@1", &changed, "metrics"),
            "",
            "metrics.retain(input.value)",
        ),
        &[&changed],
    );
    let function_name = result_call(&second).strip_prefix("app.example@1.").unwrap();
    let function = &second.functions[function_name];
    assert_eq!(
        function.body.result,
        CoreExpr::Local {
            reference: function.params[0].clone()
        }
    );
}

#[test]
fn module_alias_and_function_names_do_not_rewrite_runtime_field_labels() {
    let library = "package library.metrics@1; type Inner = { retain: U64, }; type Envelope = { metrics: Inner, }; fn take(value: Envelope) -> U64 { return value.metrics.retain; }";
    let source = format!("package app.example@1; {} type Input = {{ value: metrics.Envelope, }}; type Output = {{ value: U64, }}; intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{ let evaluated = metrics.take(input.value); return {{ value: evaluated }}; }}", import("library.metrics@1", library, "metrics"));
    let core = compile(&source, &[library]);
    let function_name = result_call(&core).strip_prefix("app.example@1.").unwrap();
    let CoreExpr::Field { base, field } = &core.functions[function_name].body.result else {
        panic!("nested field")
    };
    assert_eq!(field, "retain");
    let CoreExpr::Field { base, field } = base.as_ref() else {
        panic!("inner field")
    };
    assert_eq!(field, "metrics");
    assert!(matches!(base.as_ref(), CoreExpr::Local { .. }));
    let CoreType::Record { fields } = &core.types["library.metrics@1.Envelope"] else {
        panic!("envelope")
    };
    assert_eq!(fields["metrics"], "library.metrics@1.Inner");
}

#[test]
fn linking_is_repeatable_and_preserves_the_authenticated_source_artifact() {
    let library = "package library.metrics@1; fn retain(value: U64) -> U64 { return value; }";
    let source = root(
        &import("library.metrics@1", library, "metrics"),
        "",
        "metrics.retain(input.value)",
    );
    let closure =
        resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
    let before = closure.canonical_bytes().to_vec();
    let linked = link_source_closure(&closure).unwrap();
    let first = linked.compile(&context()).unwrap();
    let second = linked.compile(&context()).unwrap();
    assert_eq!(
        digest_core_module(&first).unwrap(),
        digest_core_module(&second).unwrap()
    );
    assert_eq!(closure.canonical_bytes(), before);
    assert_eq!(linked.source_closure().digest(), closure.digest());
}

#[test]
fn excessively_deep_linker_member_paths_and_type_constructors_refuse_structurally() {
    let library = "package library.metrics@1;";
    let path = format!("input.value{}", ".field".repeat(128));
    let deep_type = format!(
        "type Deep = {}U64{};",
        "List<".repeat(140),
        ",max=1>".repeat(140)
    );
    for source in [
        root(&import("library.metrics@1", library, "metrics"), "", &path),
        root(
            &import("library.metrics@1", library, "metrics"),
            &deep_type,
            "input.value",
        ),
    ] {
        let closure =
            resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
        let failure = link_source_closure(&closure).unwrap_err();
        assert_eq!(failure.kind, SourceLinkErrorKind::DepthExceeded);
        assert_eq!(failure.coordinate, "app.example@1");
    }
}

#[test]
fn linker_preserves_the_compilers_exact_type_constructor_height_boundary() {
    for depth in [128, 129] {
        let source = format!(
            "package app.example@1; type Deep = {}U64{};",
            "List<".repeat(depth),
            ",max=1>".repeat(depth)
        );
        let closure = resolve_source_closure(&source, &[], SourceClosureLimits::default()).unwrap();
        let linked = link_source_closure(&closure);
        if depth == 128 {
            linked
                .expect("height 128 must remain linkable")
                .compile(&context())
                .unwrap();
        } else {
            assert_eq!(linked.unwrap_err().kind, SourceLinkErrorKind::DepthExceeded);
        }
    }
}

#[test]
fn linker_preserves_builtin_request_constructors_and_rewrites_their_type_arguments() {
    let library = "package library.metrics@1; type Count = U64;";
    let source = format!(
        "package app.example@1; {} type Pending = ExternalActionRequest<List<metrics.Count,max=1>>;",
        import("library.metrics@1", library, "metrics")
    );
    compile(&source, &[library]);
}

#[test]
fn linked_root_preserves_capability_request_operations() {
    let library = "package library.metrics@1;";
    let source =
        include_str!("../../../fixtures/lang/external-actions/workspace-snapshot.amblicit")
            .replacen(
                "package examples.workspace_observer@1;",
                &format!(
                    "package examples.workspace_observer@1; {}",
                    import("library.metrics@1", library, "metrics")
                ),
                1,
            );
    let closure =
        resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
    let linked = link_source_closure(&closure).unwrap();
    let context = context()
        .with_operation_profile("workspace.read", "continuum.profile.read-only/v1")
        .with_budget(
            "workspace.tiny",
            CoreBudget {
                max_steps: 512,
                max_allocated_bytes: 256 * 1024,
                max_output_bytes: 128 * 1024,
            },
        );
    let core = linked.compile(&context).unwrap();
    let CoreNode::ExternalActionRequest { operation, .. } = &core.intents["observe"].body.nodes[0]
    else {
        panic!("authored capability request survives linking")
    };
    assert_eq!(operation.coordinate, "workspace.snapshot.observe@1");
    assert_eq!(operation.digest, Some(format!("sha256:{}", "a".repeat(64))));
}

#[test]
fn library_core_paths_require_their_own_import_instead_of_capturing_the_root() {
    let pin = format!("sha256:{}", "1".repeat(64));
    let lawpack_import = format!("use lawpack example.bounds@1 digest \"{pin}\" as core;");
    let library =
        "package library.metrics@1; fn retain(value: U64) -> U64 { return core.retain(value); }";
    let source = root(
        &format!(
            "{} {lawpack_import}",
            import("library.metrics@1", library, "metrics")
        ),
        "",
        "metrics.retain(input.value)",
    );
    let closure =
        resolve_source_closure(&source, &[library], SourceClosureLimits::default()).unwrap();
    let failure = link_source_closure(&closure).unwrap_err();
    assert_eq!(failure.kind, SourceLinkErrorKind::UnresolvedName);
    assert_eq!(failure.coordinate, "library.metrics@1");
    let declared = library.replace("fn retain", &format!("{lawpack_import} fn retain"));
    let control = root(
        &import("library.metrics@1", &declared, "metrics"),
        "",
        "metrics.retain(input.value)",
    );
    let closure =
        resolve_source_closure(&control, &[&declared], SourceClosureLimits::default()).unwrap();
    link_source_closure(&closure).expect("an owning module import permits the namespace");
}

#[test]
fn linker_preserves_utf8_prelude_with_an_imported_byte_alias() {
    let library = "package library.bytes@1; type Blob=Bytes<max=32>;";
    let source=format!("package app.example@1; {} type Input={{bytes:bytes.Blob,}}; intent evaluate(input:Input) returns Bool profile p.read basis none budget <= p.small {{let valid = isUtf8(input.bytes); return valid;}}",import("library.bytes@1",library,"bytes"));
    let core = compile(&source, &[library]);
    assert!(core.intents.contains_key("evaluate"));
}

#[test]
fn linker_preserves_count_prelude_with_an_imported_byte_alias() {
    let library = "package library.bytes@1; type Blob=Bytes<max=32>;";
    let source=format!("package app.example@1; {} type Input={{bytes:bytes.Blob,}}; intent evaluate(input:Input) returns U64 profile p.read basis none budget <= p.small {{let lines = count(input.bytes, b\"\\n\"); return lines;}}",import("library.bytes@1",library,"bytes"));
    let core = compile(&source, &[library]);
    assert!(core.intents.contains_key("evaluate"));
}

#[test]
fn count_over_an_imported_byte_alias_keeps_its_bound_in_source_and_target() {
    // The imported alias keeps its named coordinate, so both the compiler and
    // Target must resolve it to its byte maximum to prove `count + 1`.
    let library = "package library.bytes@1; type Blob=Bytes<max=32>;";
    let source=format!("package app.example@1; {} type Input={{bytes:bytes.Blob,}}; intent evaluate(input:Input) returns U64 profile p.read basis none budget <= p.small {{let lines = count(input.bytes, b\"\\n\") + 1u64; return lines;}}",import("library.bytes@1",library,"bytes"));
    let core = compile(&source, &[library]);
    let target = lower_to_target_ir(
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
    assert_eq!(target.status, TargetLoweringStatus::Lowered, "{target:?}");
}

#[test]
fn linked_unsigned_byte_helpers_preserve_builtin_types_and_literal_width() {
    for body in ["return value;", "return 255u8;"] {
        let library = format!(
            "package library.bytes@1; type Byte = U8; fn retain(value: Byte) -> Byte {{ {body} }}"
        );
        let source = root(
            &import("library.bytes@1", &library, "bytes"),
            "",
            "bytes.retain(input.value)",
        )
        .replace("U64", "U8");
        let core = compile(&source, &[&library]);
        let CoreType::Record { fields } = &core.types["Input"] else {
            panic!("input record");
        };
        assert_eq!(fields["value"], "U8");
        let callee = result_call(&core);
        let function = &core.functions[callee.strip_prefix("app.example@1.").unwrap()];
        assert_eq!(function.return_type, "U8");
        if body == "return 255u8;" {
            assert_eq!(
                function.body.result,
                CoreExpr::Const(amblicit_syntax::CoreValue::Int {
                    width: "U8".into(),
                    value: "255".into(),
                })
            );
        } else {
            assert!(matches!(function.body.result, CoreExpr::Local { .. }));
        }
    }
}

#[test]
fn linker_preserves_at_prelude_inside_an_imported_source_helper() {
    let library = "package library.bytes@1; fn lookup(bytes: Bytes<max=32>, index: U64) -> U8 { return if index < len(bytes) then at(bytes, index) else 0u8; }";
    let source = format!("package app.example@1; {} type Input={{bytes:Bytes<max=32>, index:U64,}}; intent evaluate(input:Input) returns U8 profile p.read basis none budget <= p.small {{let value = bytes.lookup(input.bytes,input.index); return value;}}", import("library.bytes@1",library,"bytes"));
    let core = compile(&source, &[library]);
    assert_eq!(core.functions.len(), 1);
    let target = lower_to_target_ir(
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
    assert_eq!(target.status, TargetLoweringStatus::Lowered, "{target:?}");
}

#[test]
fn linked_folds_resolve_local_and_imported_steps_with_hygienic_identity() {
    let library = "package library.steps@1; fn retain(value: U64) -> U64 { return 1u64; } fn run(value: U64) -> U64 { return fold(value, retain, 3u64); }";
    let local = "fn retain(value: U64) -> U64 { return 2u64; }";
    let imports = import("library.steps@1", library, "steps");
    for (source, dependencies, expected) in [
        (
            root("", local, "fold(input.value, retain, 3u64)"),
            vec![],
            "2",
        ),
        (
            root(&imports, local, "fold(input.value, steps.retain, 3u64)"),
            vec![library],
            "1",
        ),
        (
            root(&imports, local, "steps.run(input.value)"),
            vec![library],
            "1",
        ),
    ] {
        let core = compile(&source, &dependencies);
        let CoreNode::Let { value, .. } = &core.intents["evaluate"].body.nodes[0] else {
            panic!("fold result binding")
        };
        let expression = match value {
            CoreExpr::Call { callee, .. } => {
                &core.functions[callee.strip_prefix("app.example@1.").unwrap()]
                    .body
                    .result
            }
            value => value,
        };
        let CoreExpr::Fold { step, bound, .. } = expression else {
            panic!("explicit fold")
        };
        assert_eq!(*bound, 3);
        let function = &core.functions[step.strip_prefix("app.example@1.").unwrap()];
        assert_eq!(
            function.body.result,
            CoreExpr::Const(amblicit_syntax::CoreValue::Int {
                width: "U64".into(),
                value: expected.into()
            })
        );
        assert!(core
            .required_core_capabilities
            .iter()
            .any(|capability| capability == "core.fold/v1"));
        let facts = TargetIrLoweringFacts {
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
        };
        let report = lower_to_target_ir(&core, &facts);
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
        assert_eq!(
            digest_core_module(&core).unwrap(),
            digest_core_module(&compile(&source, &dependencies)).unwrap()
        );
    }
}

#[test]
fn linked_fold_keeps_callback_and_dynamic_bound_refusals() {
    for (expression, kind) in [
        (
            "fold(input.value, input.value, 3u64)",
            CompilerErrorKind::UnsupportedSourceShape,
        ),
        (
            "fold(input.value, retain, input.value)",
            CompilerErrorKind::InvalidBound,
        ),
    ] {
        let source = root(
            "",
            "fn retain(value: U64) -> U64 { return value; }",
            expression,
        );
        let closure = resolve_source_closure(&source, &[], SourceClosureLimits::default()).unwrap();
        let linked = link_source_closure(&closure).expect("linking preserves typed fold refusal");
        let errors = linked
            .compile(&context())
            .expect_err("invalid fold refuses after linking");
        assert!(
            errors.iter().any(|failure| failure.error.kind == kind),
            "{errors:?}"
        );
    }
}

#[test]
fn linked_lists_preserve_builtins_and_nominal_elements() {
    let library = "package library.worklist@1; type Item = variant { Empty, Value(U64) };
      fn step(xs: List<Item,max=2>) -> List<Item,max=2> { return if listLen(xs) < 2u64 then listAppend(xs,Item::Value(1u64)) else xs; }
      fn run(value: U64) -> U64 { let empty: List<Item,max=2> = []; let result = fold(empty,step,3u64); let joined = listConcat(result,empty); return if value < listLen(joined) then match listAt(joined,value) { Empty => 0u64, Value(found) => found } else 0u64; }";
    let source = root(
        &import("library.worklist@1", library, "worklist"),
        "",
        "worklist.run(input.value)",
    );
    let core = compile(&source, &[library]);
    let facts = TargetIrLoweringFacts {
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
    };
    let report = lower_to_target_ir(&core, &facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert_eq!(
        digest_core_module(&core).unwrap(),
        digest_core_module(&compile(&source, &[library])).unwrap()
    );
}
