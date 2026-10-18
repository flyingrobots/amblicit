//! Source aliases preserve their underlying semantic and authority identities.
use edict_syntax::{
    compile_to_core, digest_core_module, parse_module, CompilerContext, CompilerErrorKind,
    CoreBudget, CoreModule, CoreType, ResourceRef, TypeShapeFact,
};
use std::fmt::Write;

fn context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.small",
            CoreBudget {
                max_steps: 4096,
                max_allocated_bytes: 65536,
                max_output_bytes: 65536,
            },
        )
}
fn compile(source: &str) -> CoreModule {
    compile_to_core(&parse_module(source).unwrap(), &context())
        .expect("supported transparent source aliases")
}
fn program(declarations: &str, field: &str) -> String {
    format!("package alias.example@1; {declarations} type Input = {{ value: {field}, }}; type Output = {{ value: {field}, }}; intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{ return {{ value: input.value }}; }}")
}

#[test]
fn source_scalar_and_bounded_byte_aliases_are_semantically_transparent() {
    for (declarations, name, expanded) in [
        ("type Count = U64;", "Count", "U64"),
        ("type Flag = Bool;", "Flag", "Bool"),
        ("type Data = Bytes<exact=8>;", "Data", "Bytes<exact=8>"),
        ("type Data = Bytes<max=8>;", "Data", "Bytes<max=8>"),
        ("type Data = String<max=8>;", "Data", "String<max=8>"),
        (
            "type Counts = List<U64,max=4>;",
            "Counts",
            "List<U64,max=4>",
        ),
    ] {
        let alias = compile(&program(declarations, name));
        let direct = compile(&program("", expanded));
        assert_eq!(
            digest_core_module(&alias).unwrap(),
            digest_core_module(&direct).unwrap()
        );
        assert!(
            !alias.types.contains_key(name),
            "transparent alias is not a new nominal contract"
        );
    }
}

#[test]
fn forward_aliases_and_records_resolve_independently_of_declaration_order() {
    let first = compile("package alias.example@1; type Envelope = { value: Count, payload: Payload, }; type Count = Scalar; type Scalar = U64; type Payload = { enabled: Bool, };");
    let reordered = compile("package alias.example@1; type Scalar = U64; type Count = Scalar; type Payload = { enabled: Bool, }; type Envelope = { value: Count, payload: Payload, };");
    assert_eq!(
        digest_core_module(&first).unwrap(),
        digest_core_module(&reordered).unwrap()
    );
    let CoreType::Record { fields } = &first.types["Envelope"] else {
        panic!("record")
    };
    assert_eq!(fields["value"], "U64");
    assert_eq!(fields["payload"], "alias.example@1.Payload");
}

#[test]
fn aliases_preserve_an_imported_nominal_contract_without_laundering_it() {
    let pin = format!("sha256:{}", "1".repeat(64));
    let context = context().with_type_shape(TypeShapeFact {
        lawpack: ResourceRef {
            coordinate: "example.identities@1".into(),
            digest: Some(pin.clone()),
        },
        coordinate: "example.identities@1.Key".into(),
        definition: "Nominal<Bytes<exact=32>>".into(),
    });
    let source = format!("package alias.example@1; use lawpack example.identities@1 digest \"{pin}\" as identities; type Key = identities.Key; type Input = {{ value: Key, }}; type Output = {{ value: identities.Key, }}; intent evaluate(input: Input) returns Output profile p.read basis none budget <= p.small {{ return {{ value: input.value }}; }}");
    let core = compile_to_core(&parse_module(&source).unwrap(), &context).unwrap();
    assert!(!core.types.contains_key("Key"));
    assert!(
        matches!(&core.types["example.identities@1.Key"], CoreType::Nominal { contract, .. } if contract == "example.identities@1.Key")
    );
    let bad = source.replace("value: Key", "value: Bytes<exact=32>");
    let errors = compile_to_core(&parse_module(&bad).unwrap(), &context).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn cyclic_and_missing_source_aliases_have_structured_refusals() {
    for (source, kind) in [
        (
            "package alias.example@1; type First = Second; type Second = First;",
            CompilerErrorKind::InvalidCoreTypeIntegrity,
        ),
        (
            "package alias.example@1; type First = Missing;",
            CompilerErrorKind::UnresolvedType,
        ),
    ] {
        let errors = compile_to_core(&parse_module(source).unwrap(), &context()).unwrap_err();
        assert!(errors.iter().any(|error| error.kind == kind), "{errors:?}");
    }
}

#[test]
fn source_type_dependency_depth_and_expansion_work_refuse_before_unbounded_expansion() {
    let mut chain = String::from("package alias.example@1; type Layer0 = U64;");
    for index in 1..129 {
        write!(chain, " type Layer{index} = Layer{};", index - 1).unwrap();
    }
    let errors = compile_to_core(&parse_module(&chain).unwrap(), &context()).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
    let mut fanout = String::from("package alias.example@1; type Node0 = { value: U64, };");
    for index in 1..24 {
        write!(
            fanout,
            " type Node{index} = {{ left: Node{}, right: Node{}, }};",
            index - 1,
            index - 1
        )
        .unwrap();
    }
    let errors = compile_to_core(&parse_module(&fanout).unwrap(), &context()).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}

#[test]
fn source_type_depth_checks_cached_shared_paths_at_the_exact_boundary() {
    let mut chain = String::from("package alias.example@1; type Layer0 = U64;");
    for index in 1..127 {
        write!(chain, " type Layer{index} = Layer{};", index - 1).unwrap();
    }
    let exact = format!("{chain} type ZEnvelope = {{ shallow: Layer0, deep: Layer126, }};");
    compile(&exact);
    let too_deep = format!("{chain} type Layer127 = Layer126; type ZEnvelope = {{ shallow: Layer0, deep: Layer127, }};");
    let failures = compile_to_core(&parse_module(&too_deep).unwrap(), &context()).unwrap_err();
    assert!(failures
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}

#[test]
fn source_defined_expansion_work_accepts_exactly_65536_cells_and_refuses_one_more() {
    // Complete binary declaration shapes with 1..32767 cells sum to 65519.
    // A separate record with sixteen scalar fields contributes seventeen cells.
    let mut source = String::from("package alias.example@1; type Node0 = {};");
    for index in 1..15 {
        write!(
            source,
            " type Node{index} = {{ left: Node{}, right: Node{}, }};",
            index - 1,
            index - 1
        )
        .unwrap();
    }
    source.push_str(" type Padding = {");
    for index in 0..16 {
        write!(source, " field{index}: U64,").unwrap();
    }
    let exact = format!("{source} }};");
    compile(&exact);
    let over = format!("{source} extra: U64, }};");
    let failures = compile_to_core(&parse_module(&over).unwrap(), &context()).unwrap_err();
    assert!(failures
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}
