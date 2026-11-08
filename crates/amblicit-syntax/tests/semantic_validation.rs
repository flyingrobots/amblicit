//! Semantic validation tests for source-AST checks that do not require Core IR.

use amblicit_syntax::{parse_module, validate_module, validate_surface, SemanticErrorKind};

const BOUNDED_HELLO: &str = include_str!("../../../fixtures/lang/bounds/bounded-hello.amblicit");
const CONDITIONAL_BLOB: &str =
    include_str!("../../../fixtures/lang/effects/conditional-blob.amblicit");
const READ_GREETING: &str = include_str!("../../../fixtures/lang/effects/read-greeting.amblicit");

fn semantic_kinds(src: &str) -> Vec<SemanticErrorKind> {
    let module = parse_module(src).expect("source parses");
    let mut kinds = validate_surface(&module)
        .expect_err("source must fail semantic validation")
        .into_iter()
        .map(|err| err.kind)
        .collect::<Vec<_>>();
    kinds.sort_by_key(|kind| format!("{kind:?}"));
    kinds
}

#[test]
fn phase1_fixtures_validate_semantically() {
    for src in [BOUNDED_HELLO, CONDITIONAL_BLOB, READ_GREETING] {
        let module = parse_module(src).expect("fixture parses");
        validate_surface(&module).expect("fixture is semantically valid");
    }
}

#[test]
fn validate_module_remains_surface_stage_compatibility_alias() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    validate_surface(&module).expect("surface validation accepts fixture");
    validate_module(&module).expect("compatibility alias accepts fixture");
}

#[test]
fn surface_validation_defers_import_and_name_resolution() {
    let module = parse_module(
        "package a.b@1;\n\
         intent t(input: missing.Input) returns missing.Output\n\
           profile missing.readOnly\n\
           basis input.id\n\
           budget <= missing.budget {\n\
           let projected: missing.Box = unknown.helper<missing.Unchecked>(input);\n\
           return { projected };\n\
         }",
    )
    .expect("source parses");

    validate_surface(&module).expect("surface validation does not resolve imports or names");
}

#[test]
fn surface_validation_defers_contextual_typing_and_loop_bound_proof() {
    let module = parse_module(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           let value: shape.U64 = 1i32;\n\
           for item in input.items bounded shape.maxItems {\n\
             let current = item;\n\
           }\n\
           return { value };\n\
         }",
    )
    .expect("source parses");

    validate_surface(&module).expect("surface validation does not type-check or prove loop bounds");
}

#[test]
fn surface_validation_defers_obstruction_exhaustiveness() {
    let module = parse_module(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out\n\
           profile shape.readWrite\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           echo.write(input) else {\n\
             unavailable => shape.Unavailable({ input }),\n\
           };\n\
           return { input };\n\
         }",
    )
    .expect("source parses");

    validate_surface(&module)
        .expect("surface validation does not prove obstruction exhaustiveness");
}

#[test]
fn unbounded_runtime_scalars_are_rejected_recursively() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         type T = { name: String, blob: Option<Bytes>, items: List<String, max=3>, };\n",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
        ]
    );
}

#[test]
fn unbounded_runtime_scalars_are_rejected_in_declaration_type_surfaces() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         type T = { entries: Map<String, Bytes, max=3>, };\n\
         type V = variant { Raw(Bytes), Label(String) };\n",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
        ]
    );
}

#[test]
fn unbounded_runtime_scalars_are_rejected_in_intent_and_expression_surfaces() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         intent t(input: String) returns Bytes\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           let value: String = echo.make<CapabilityRef<Bytes>, shape.Box<String>>(input);\n\
           return value;\n\
         }",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
            SemanticErrorKind::UnboundedScalar,
        ]
    );
}

#[test]
fn intent_required_clauses_are_validated() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out {\n\
           return { input };\n\
         }",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::MissingBasis,
            SemanticErrorKind::MissingBudget,
            SemanticErrorKind::MissingOperationMode,
        ]
    );
}

#[test]
fn duplicate_implements_and_footprint_clauses_are_rejected() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out\n\
           profile shape.readOnly\n\
           implements shape.reader\n\
           implements shape.writer\n\
           basis none\n\
           footprint <= shape.small\n\
           footprint <= shape.large\n\
           budget <= shape.tinyBudget {\n\
           return { input };\n\
         }",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::DuplicateIntentClause,
            SemanticErrorKind::DuplicateIntentClause,
        ]
    );
}

#[test]
fn profile_or_implements_satisfies_operation_mode() {
    for clause in ["profile shape.readOnly", "implements shape.reader"] {
        let src = format!(
            "package a.b@1;\n\
             intent t(input: shape.In) returns shape.Out\n\
               {clause}\n\
               basis none\n\
               budget <= shape.tinyBudget {{\n\
               return {{ input }};\n\
             }}"
        );
        let module = parse_module(&src).expect("source parses");
        validate_surface(&module).expect("operation mode is present");
    }
}

#[test]
fn duplicate_singleton_intent_clauses_are_rejected() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out\n\
           profile shape.readOnly\n\
           profile shape.readWrite\n\
           basis none\n\
           basis input.id\n\
           budget <= shape.tinyBudget\n\
           budget <= shape.largeBudget {\n\
           return { input };\n\
         }",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::DuplicateIntentClause,
            SemanticErrorKind::DuplicateIntentClause,
            SemanticErrorKind::DuplicateIntentClause,
        ]
    );
}

#[test]
fn module_namespace_collisions_are_rejected() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         use shape \"schemas/a.graphql\" as Input;\n\
         type Input = { id: String<max=8>, };\n\
         enum Status { Ready }\n\
         intent Status(input: shape.In) returns shape.Out\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           return { input };\n\
         }",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::DuplicateName,
            SemanticErrorKind::DuplicateName,
        ]
    );
}

#[test]
fn local_binders_cannot_shadow_visible_names() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         use shape \"schemas/a.graphql\" as shape;\n\
         type Input = { id: String<max=8>, };\n\
         intent t(shape: Input) returns Input\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           let shape = shape;\n\
           return { shape };\n\
         }",
    );
    assert_eq!(
        kinds,
        vec![
            SemanticErrorKind::ShadowedName,
            SemanticErrorKind::ShadowedName,
        ]
    );
}

#[test]
fn branch_and_loop_binders_are_scoped() {
    let module = parse_module(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           if input.ready {\n\
             let branch = input;\n\
           } else {\n\
             let branch = input;\n\
           }\n\
           for branch in input.items bounded shape.maxItems {\n\
             let loopValue = branch;\n\
           }\n\
           let branch = input;\n\
           return { branch };\n\
         }",
    )
    .expect("source parses");
    validate_surface(&module).expect("block-local names do not leak");
}

#[test]
fn branch_yield_binders_are_scoped() {
    let module = parse_module(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           let result = if input.ready {\n\
             let temporary = input;\n\
             yield temporary;\n\
           } else {\n\
             let temporary = input;\n\
             yield temporary;\n\
           };\n\
           let temporary = result;\n\
           return { temporary };\n\
         }",
    )
    .expect("source parses");
    validate_surface(&module).expect("branch-yield locals do not leak");
}

#[test]
fn expression_binders_cannot_shadow_visible_names() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         type Input = { id: String<max=8>, };\n\
         intent t(input: Input) returns Input\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           let result = match input { Some(input) => input, None => input };\n\
           return { result };\n\
         }",
    );
    assert_eq!(kinds, vec![SemanticErrorKind::ShadowedName]);
}

#[test]
fn clause_expression_binders_see_parameters() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         type Input = { id: String<max=8>, };\n\
         intent t(input: Input) returns Input\n\
           profile shape.readOnly\n\
           basis none\n\
           budget <= shape.tinyBudget\n\
           where match input { Some(input) => true, None => true } {\n\
           return { input };\n\
         }",
    );
    assert_eq!(kinds, vec![SemanticErrorKind::ShadowedName]);
}

#[test]
fn obstruction_map_binders_cannot_shadow_visible_names() {
    let kinds = semantic_kinds(
        "package a.b@1;\n\
         intent t(input: shape.In) returns shape.Out\n\
           profile shape.readWrite\n\
           basis none\n\
           budget <= shape.tinyBudget {\n\
           echo.write(input) else {\n\
             mismatch(input) => shape.Bad({ observed: input }),\n\
           };\n\
           return { input };\n\
         }",
    );
    assert_eq!(kinds, vec![SemanticErrorKind::ShadowedName]);
}

#[test]
fn unsigned_byte_prelude_name_cannot_be_shadowed() {
    for source in [
        "package bytes.shadow@1; fn U8(value: U8) -> U8 { return value; }",
        "package bytes.shadow@1; fn retain(U8: U8) -> U8 { return U8; }",
        "package bytes.shadow@1; fn retain(value: U8) -> U8 { let U8 = value; return U8; }",
    ] {
        assert_eq!(
            semantic_kinds(source),
            vec![SemanticErrorKind::ShadowedName]
        );
    }
}
