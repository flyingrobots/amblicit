//! Strict record-field failure order is semantic and must survive Core hashing.
//!
//! Oracle: Typed Function Failures in the language spec keeps evaluation
//! strict and left to right. Reversing two fields with distinct failures
//! changes which failure escapes; reordering total fields does not.
use amblicit_syntax::{
    compile_to_core, encode_core_module, lower_to_target_ir, parse_module,
    validate_core_module_type_integrity, CompilerContext, CoreBudget, CoreExpr, CoreModule,
    CoreTypeIntegrityFailureKind, ResourceRef, TargetIrLoweringFacts, TargetLoweringStatus,
};

fn source(result: &str, body: &str) -> String {
    format!(
        "package record.order@1;\nfailure First;\nfailure Second;\n\
         type Pair = {{ a: U64, z: U64, }};\ntype Result = {result};\ntype Input = {{}};\n\
         fn first() -> U64 fails First {{ return fail First; }}\n\
         fn second() -> U64 fails Second {{ return fail Second; }}\n\
         fn retain(pair: Pair) -> Pair {{ return pair; }}\n\
         fn compose() -> Result fails First, Second {{ {body} }}\n\
         intent evaluate(input: Input) returns Result profile p.read basis none budget <= p.large {{\n\
           let value = compose() else {{ First => text.First, Second => text.Second }};\n\
           return value;\n}}"
    )
}

fn compiled(source: &str) -> CoreModule {
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.large",
            CoreBudget {
                max_steps: 10_000_000,
                max_allocated_bytes: u64::MAX,
                max_output_bytes: u64::MAX,
            },
        );
    let module = parse_module(source).expect("record-order source parses");
    let core = compile_to_core(&module, &context)
        .unwrap_or_else(|errors| panic!("record-order source compiles: {errors:#?}"));
    validate_core_module_type_integrity(&core).expect("record-order Core is valid");
    core
}

fn native_accepts(core: &CoreModule) {
    for domain in ["echo.span-ir/v2", "echo.span-ir/v3"] {
        let facts = TargetIrLoweringFacts {
            target_profile: ResourceRef {
                coordinate: "echo.dpo@1".into(),
                digest: Some(format!("sha256:{}", "1".repeat(64))),
            },
            target_ir_domain: domain.into(),
            operation_profiles: vec!["continuum.profile.read-only/v1".into()],
            obstruction_coordinates: Vec::new(),
            effect_lowerings: Vec::new(),
            effect_signatures: Vec::new(),
            pure_functions: Vec::new(),
        };
        let report = lower_to_target_ir(core, &facts);
        assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:#?}");
        assert!(report.artifact.is_some(), "{domain}: expected artifact");
    }
}

#[test]
fn record_failure_order_is_hash_significant_in_every_expression_position() {
    for (result, body) in [
        ("Pair", "return { FIELDS };"),
        ("Pair", "let pair: Pair = { FIELDS }; return pair;"),
        ("Pair", "return retain({ FIELDS });"),
        (
            "Pair",
            "return if true then { FIELDS } else { a: 0u64, z: 0u64 };",
        ),
        ("List<Pair,max=1>", "return [{ FIELDS }];"),
        ("{ inner: Pair }", "return { inner: { FIELDS } };"),
    ] {
        for (first, second) in [
            (
                "z: fail First, a: fail Second",
                "a: fail Second, z: fail First",
            ),
            (
                "z: expect first(), a: expect second()",
                "a: expect second(), z: expect first()",
            ),
        ] {
            let first_core = compiled(&source(result, &body.replace("FIELDS", first)));
            let second_core = compiled(&source(result, &body.replace("FIELDS", second)));
            native_accepts(&first_core);
            native_accepts(&second_core);
            assert!(
                encode_core_module(&first_core).expect("first-order Core encodes")
                    != encode_core_module(&second_core).expect("second-order Core encodes"),
                "{body}: First escapes from `{first}`, Second escapes from `{second}`"
            );
        }
    }
}

#[test]
fn total_record_field_order_stays_canonical_even_in_a_partial_function() {
    let body = "return if true then { FIELDS } else if true then fail First else fail Second;";
    let first = compiled(&source("Pair", &body.replace("FIELDS", "z: 1u64, a: 2u64")));
    let second = compiled(&source("Pair", &body.replace("FIELDS", "a: 2u64, z: 1u64")));
    native_accepts(&first);
    native_accepts(&second);
    assert_eq!(
        encode_core_module(&first).expect("first total record encodes"),
        encode_core_module(&second).expect("second total record encodes"),
        "total record fields retain their canonical representation"
    );
}

fn partial_pair() -> CoreModule {
    compiled(&source("Pair", "return { z: fail First, a: fail Second };"))
}

fn invalid_definition(core: &CoreModule, path: &str) {
    let Err(failure) = validate_core_module_type_integrity(core) else {
        panic!("invalid record evaluation authority was accepted")
    };
    assert_eq!(
        failure.kind(),
        CoreTypeIntegrityFailureKind::InvalidDefinition
    );
    assert_eq!(failure.path(), path);
}

#[test]
fn fallible_record_order_and_capability_are_independently_required() {
    let original = partial_pair();
    let CoreExpr::OrderedRecord { fields } = &original.functions["compose"].body.result else {
        panic!("fallible fields require an ordered record")
    };
    assert_eq!(
        fields
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["z", "a"]
    );

    let mut unordered = original.clone();
    unordered
        .functions
        .get_mut("compose")
        .expect("compose")
        .body
        .result = CoreExpr::Record {
        fields: fields.iter().cloned().collect(),
    };
    invalid_definition(&unordered, "functions.compose.body");

    let mut incapable = original;
    incapable
        .required_core_capabilities
        .retain(|capability| capability != "core.failure/v1");
    invalid_definition(&incapable, "requiredCoreCapabilities");
}

#[test]
fn ordered_record_names_are_nonempty_and_unique() {
    for name in ["z", ""] {
        let mut forged = partial_pair();
        let CoreExpr::OrderedRecord { fields } = &mut forged
            .functions
            .get_mut("compose")
            .expect("compose")
            .body
            .result
        else {
            panic!("ordered record")
        };
        fields[1].0 = name.to_owned();
        invalid_definition(
            &forged,
            &format!("functions.compose.body.result.orderedRecord.{name}"),
        );
    }
}

#[test]
fn total_ordered_records_are_noncanonical_even_when_other_functions_fail() {
    let mut forged = compiled(&source(
        "Pair",
        "return if true then { z: 0u64, a: 0u64 } else if true then fail First else fail Second;",
    ));
    native_accepts(&forged);
    let CoreExpr::If { then_value, .. } = &mut forged
        .functions
        .get_mut("compose")
        .expect("compose")
        .body
        .result
    else {
        panic!("conditional retains valid failure authority")
    };
    let CoreExpr::Record { fields } = then_value.as_ref() else {
        panic!("total branch has an ordinary record")
    };
    **then_value = CoreExpr::OrderedRecord {
        fields: fields
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
    };
    invalid_definition(&forged, "functions.compose.body");
}

#[test]
fn ordered_record_fields_still_obey_the_enclosing_failure_set() {
    let mut forged = partial_pair();
    let CoreExpr::OrderedRecord { fields } = &mut forged
        .functions
        .get_mut("compose")
        .expect("compose")
        .body
        .result
    else {
        panic!("ordered record")
    };
    fields[0].1 = CoreExpr::Fail {
        failure: "record.order@1.Missing".into(),
    };
    invalid_definition(&forged, "functions.compose.failures");
}
