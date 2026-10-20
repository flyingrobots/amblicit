//! Compiler-spine tests for the first executable source-to-Core path.
//!
//! These tests assert public stage boundaries and structured values. They do
//! not inspect stdout, stderr, diagnostic prose, canonical bytes, or hashes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use edict_syntax::{
    compile_to_core, digest_core_module, load_compiler_context_from_authority_fact_files,
    lower_core, parse_module, resolve_module, type_check, BoundFact, CompilerContext,
    CompilerErrorKind, CompilerStage, CoreBound, CoreBudget, CoreExpr, CoreNode,
    CoreObstructionReason, CorePredicate, CoreRequireFailureArm, CoreType, PureFunctionFact,
    PureHelperCostFact, ResolvedModule, ResourceRef, TypeShapeFact, WriteClass,
};

const BOUNDED_HELLO: &str = include_str!("../../../fixtures/lang/bounds/bounded-hello.edict");
static TEMP_CASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const EFFECTFUL_REPLACE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      return { id: input.id };\n\
    }";
const EFFECTFUL_BRANCH_YIELD: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let id = if true {\n\
        let receipt: Receipt = target.replace(input.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
        yield input.id;\n\
      } else {\n\
        yield \"fallback\";\n\
      };\n\
      return { id };\n\
    }";
const PURE_HELPER_BRANCH_YIELD: &str = "package a.b@1;\n\
    use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
    type Input = { value: U64, };\n\
    type Output = { value: U64, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      let _discarded = if true {\n\
        yield helpers.bump(input.value);\n\
      } else {\n\
        yield input.value;\n\
      };\n\
      return { value: input.value };\n\
    }";
const DUPLICATE_OBSTRUCTION_FAILURE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else {\n\
          rejected(reason) => domain.WriteRejected,\n\
          rejected(other) => domain.WriteRejectedAgain,\n\
        };\n\
      return { id: input.id };\n\
    }";
const ORDERED_OBSTRUCTION_FAILURES: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else {\n\
          rejected(reason) => domain.WriteRejected,\n\
          timeout(wait) => domain.WriteTimedOut,\n\
        };\n\
      return { id: input.id };\n\
    }";
const REVERSED_OBSTRUCTION_FAILURES: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else {\n\
          timeout(wait) => domain.WriteTimedOut,\n\
          rejected(reason) => domain.WriteRejected,\n\
        };\n\
      return { id: input.id };\n\
    }";
const TERMINAL_REQUIRE_OBSTRUCTION: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      require true else jim.EditObstruction.StaleBase;\n\
      return { id: input.id };\n\
    }";
const TERMINAL_REQUIRE_WITH_REASON_PAYLOAD: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      require true else jim.EditObstruction.StaleBase({ reason: input.id });\n\
      return { id: input.id };\n\
    }";
const CONTINUE_OBSTRUCTED_REQUIRE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      require true else continue obstructed {\n\
        reason: jim.EditObstruction.StaleBase,\n\
        provided: input.id,\n\
      };\n\
      return { id: input.id };\n\
    }";
const PURE_CONDITIONAL: &str = "package a.b@1;\n\
    type Input = { choose: U32, left: String<max=16>, right: String<max=16>, };\n\
    type Output = { value: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      let value = if input.choose == 0u32 then input.left else input.right;\n\
      return { value };\n\
    }";
const PURE_HELPER_CALL: &str = "package a.b@1;\n\
    use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
    type Input = { value: U64, };\n\
    type Output = { value: U64, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      let value: U64 = helpers.bump(input.value);\n\
      return { value };\n\
    }";
const INVALID_IMPORTED_STRING_HELPER: &str = "package a.b@1;\n\
    use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
    type Input = { value: U64, };\n\
    type Output = { value: String<max=32>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      let value = helpers.invalidString(input.value);\n\
      return { value };\n\
    }";
const BOUNDED_LIST_LOOP: &str = "package a.b@1;\n\
    type Input = { items: List<U64, max=4>, };\n\
    type Output = { value: U64, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      for item in input.items bounded 4 {\n\
        require item <= 10u64 else example.ItemTooLarge;\n\
      }\n\
      return { value: 0u64 };\n\
    }";
const EXPORTED_LIST_LOOP: &str = "package a.b@1;\n\
    use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
    type Input = { values: List<String<max=32>, max=4>, };\n\
    type Output = { value: U64, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      let values = helpers.identityList(input.values);\n\
      for value in values bounded 4 {\n\
        require value == value else example.InvalidValue;\n\
      }\n\
      return { value: 0u64 };\n\
    }";
const STATEMENT_CONDITIONAL: &str = "package a.b@1;\n\
    type Input = { choose: U32, left: U64, right: U64, };\n\
    type Output = { value: U64, };\n\
    intent t(input: Input) returns Output\n\
      profile p.read\n\
      basis none\n\
      budget <= p.tiny {\n\
      if input.choose == 0u32 {\n\
        let selected: U64 = input.left;\n\
        require selected <= 10u64 else example.LeftTooLarge;\n\
      } else {\n\
        require input.right <= 10u64 else example.RightTooLarge;\n\
      }\n\
      return { value: input.left };\n\
    }";

fn hello_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("hello.readOnly", "continuum.profile.read-only/v1")
        .with_budget(
            "hello.tinyBudget",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 4096,
            },
        )
}

fn pure_context() -> CompilerContext {
    pure_context_without_helper_cost().with_pure_helper_cost(
        "helpers.tiny",
        example_helper_cost(CoreBudget {
            max_steps: 1,
            max_allocated_bytes: 8,
            max_output_bytes: 8,
        }),
    )
}

fn pure_context_without_helper_cost() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 8,
                max_allocated_bytes: 1024,
                max_output_bytes: 256,
            },
        )
}

fn example_bounds_lawpack() -> ResourceRef {
    ResourceRef {
        coordinate: "example.bounds@1".to_owned(),
        digest: Some(
            "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        ),
    }
}

fn example_helper_cost(budget: CoreBudget) -> PureHelperCostFact {
    PureHelperCostFact {
        lawpack: example_bounds_lawpack(),
        coordinate: "example.bounds@1.tiny".to_owned(),
        budget,
    }
}

fn pure_helper_context(parameter_type: &str) -> CompilerContext {
    pure_context().with_pure_function(
        "helpers.bump",
        PureFunctionFact {
            lawpack: example_bounds_lawpack(),
            coordinate: "example.bounds@1.bump".to_owned(),
            type_parameters: Vec::new(),
            parameter_types: vec![parameter_type.to_owned()],
            return_type: "U64".to_owned(),
            cost_template: "example.bounds@1.tiny".to_owned(),
        },
    )
}

fn coordinate_bound_context() -> CompilerContext {
    pure_context().with_bound(
        "bounds.maxItems",
        BoundFact {
            lawpack: example_bounds_lawpack(),
            coordinate: "example.bounds@1.maxItems".to_owned(),
            value: 4,
        },
    )
}

fn coordinate_bounded_loop_source() -> String {
    BOUNDED_LIST_LOOP
        .replace(
            "type Input",
            "use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as bounds;\n    type Input",
        )
        .replace("bounded 4", "bounded bounds.maxItems")
}

#[test]
fn bounded_hello_compiles_to_initial_core() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");

    assert_eq!(core.api_version, "edict.core/v1");
    assert_eq!(core.coordinate, "examples.hello@1");
    assert_eq!(core.imports.len(), 1);
    assert_eq!(core.imports[0].resource.coordinate, "hello.optics@1");
    assert_eq!(
        core.imports[0].resource.digest.as_deref(),
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000000")
    );

    let input = core.types.get("HelloInput").expect("HelloInput type");
    assert_eq!(
        input,
        &CoreType::Record {
            fields: [(
                "name".to_owned(),
                "String<max=256,canonical=raw-utf8>".to_owned(),
            )]
            .into(),
        }
    );

    assert!(!core.types.contains_key("HelloInput.name"));

    let intent = core.intents.get("sayHello").expect("sayHello intent");
    assert_eq!(
        intent.required_operation_profile,
        "continuum.profile.read-only/v1"
    );
    assert_eq!(intent.core_evaluation_budget.max_steps, 64);
    assert_eq!(intent.input_constraints.len(), 1);

    assert!(matches!(
        &intent.input_constraints[0].predicate,
        CorePredicate::Compare { .. }
    ));
    assert_eq!(intent.body.locals.len(), 2);
    assert_eq!(intent.body.locals[0].id, "arg.0");
    assert_eq!(intent.body.locals[0].alpha_name, "$arg0");
    assert_eq!(intent.body.locals[1].id, "local.0");
    assert_eq!(intent.body.locals[1].alpha_name, "$local0");
    assert_eq!(intent.body.nodes.len(), 1);
    assert!(matches!(intent.body.nodes[0], CoreNode::Let { .. }));
    assert!(matches!(intent.body.result, CoreExpr::Record { .. }));
}

#[test]
fn compiler_spine_exposes_distinct_stage_boundaries() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let resolved = resolve_module(&module, &hello_context()).expect("resolve stage");
    assert_eq!(resolved.coordinate, "examples.hello@1");
    assert_eq!(
        resolved.intents[0].profile,
        "continuum.profile.read-only/v1"
    );

    let typed = type_check(&resolved).expect("type-check stage");
    assert_eq!(
        typed.intents[0].input_binding.ty,
        "examples.hello@1.HelloInput"
    );

    let core = lower_core(&typed).expect("lower Core stage");
    assert!(core.intents.contains_key("sayHello"));
}

#[test]
fn pure_conditional_expression_lowers_to_core() {
    let module = parse_module(PURE_CONDITIONAL).expect("conditional source parses");
    let core = compile_to_core(&module, &pure_context()).expect("conditional lowers to Core");
    let intent = core.intents.get("t").expect("lowered intent");
    let CoreNode::Let { value, .. } = &intent.body.nodes[0] else {
        panic!("conditional binding is a Core let");
    };

    assert!(matches!(
        value,
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } if matches!(predicate.as_ref(), CorePredicate::Compare { .. })
            && matches!(then_value.as_ref(), CoreExpr::Field { field, .. } if field == "left")
            && matches!(else_value.as_ref(), CoreExpr::Field { field, .. } if field == "right")
    ));
}

#[test]
fn pure_conditional_expression_rejects_incompatible_branches() {
    let source = PURE_CONDITIONAL.replace(
        "then input.left else input.right",
        "then input.left else input.choose",
    );
    let module = parse_module(&source).expect("incompatible conditional parses");
    let errors = compile_to_core(&module, &pure_context())
        .expect_err("incompatible conditional branches reject");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn pure_conditional_bare_integer_inherits_width_from_either_branch() {
    for expression in [
        "if true then 0 else input.value",
        "if true then input.value else 0",
    ] {
        let source = format!(
            "package a.b@1;\n\
             type Input = {{ value: U64, }};\n\
             type Output = {{ value: U64, }};\n\
             intent t(input: Input) returns Output\n\
               profile p.read\n\
               basis none\n\
               budget <= p.tiny {{\n\
               let value = {expression};\n\
               return {{ value }};\n\
             }}"
        );
        let module = parse_module(&source).expect("conditional source parses");
        compile_to_core(&module, &pure_context())
            .expect("either typed branch constrains the bare integer");
    }
}

#[test]
fn pure_conditional_branch_mutation_moves_core_digest() {
    let original = parse_module(PURE_CONDITIONAL).expect("conditional source parses");
    let swapped_source = PURE_CONDITIONAL.replace(
        "then input.left else input.right",
        "then input.right else input.left",
    );
    let swapped = parse_module(&swapped_source).expect("swapped conditional parses");
    let original_core =
        compile_to_core(&original, &pure_context()).expect("original conditional compiles");
    let swapped_core =
        compile_to_core(&swapped, &pure_context()).expect("swapped conditional compiles");

    assert_ne!(
        digest_core_module(&original_core).expect("digest original Core"),
        digest_core_module(&swapped_core).expect("digest swapped Core")
    );
}

#[test]
fn digest_bound_pure_helper_call_lowers_to_core() {
    let module = parse_module(PURE_HELPER_CALL).expect("pure-helper source parses");
    let core =
        compile_to_core(&module, &pure_helper_context("U64")).expect("pure-helper call lowers");
    let intent = core.intents.get("t").expect("lowered intent");
    let CoreNode::Let { value, .. } = &intent.body.nodes[0] else {
        panic!("helper binding is a Core let");
    };

    assert!(matches!(
        value,
        CoreExpr::Call { callee, type_args, args }
            if callee == "example.bounds@1.bump" && type_args.is_empty() && args.len() == 1
    ));
}

#[test]
fn exact_bytes_and_imported_nominal_aliases_preserve_core_identity() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as text;\n\
        type Input = { bufferId: text.BufferId, basisHeadId: text.HeadId, raw: Bytes<exact=32>, };\n\
        type Output = { bufferId: text.BufferId, basisHeadId: text.HeadId, raw: Bytes<exact=32>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis input.basisHeadId\n\
          budget <= p.tiny {\n\
          return { bufferId: input.bufferId, basisHeadId: input.basisHeadId, raw: input.raw };\n\
        }";
    let lawpack = example_bounds_lawpack();
    let context = pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.BufferId".to_owned(),
            definition: "Nominal<Bytes<exact=32>>".to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack,
            coordinate: "example.bounds@1.HeadId".to_owned(),
            definition: "Nominal<Bytes<exact=32>>".to_owned(),
        });
    let module = parse_module(source).expect("exact-byte source parses");
    let exact_core = compile_to_core(&module, &context).expect("exact-byte source compiles");

    for parent in ["Input", "Output"] {
        let Some(CoreType::Record { fields }) = exact_core.types.get(parent) else {
            panic!("{parent} must remain a named record");
        };
        assert_eq!(
            fields.get("raw").map(String::as_str),
            Some("Bytes<exact=32>")
        );
        assert_eq!(
            fields.get("bufferId").map(String::as_str),
            Some("example.bounds@1.BufferId")
        );
        assert_eq!(
            fields.get("basisHeadId").map(String::as_str),
            Some("example.bounds@1.HeadId")
        );
        assert!(exact_core
            .types
            .keys()
            .all(|coordinate| !coordinate.starts_with(&format!("{parent}."))));
    }
    for contract in ["example.bounds@1.BufferId", "example.bounds@1.HeadId"] {
        assert_eq!(
            exact_core.types.get(contract),
            Some(&CoreType::Nominal {
                contract: contract.to_owned(),
                representation: "Bytes<exact=32>".to_owned(),
            }),
            "{contract} must retain its exact nominal storage ABI",
        );
    }

    let max_only = parse_module(&source.replace("Bytes<exact=32>", "Bytes<max=32>"))
        .expect("max-only control parses");
    let max_only_core = compile_to_core(&max_only, &context).expect("max-only control compiles");
    assert_ne!(
        digest_core_module(&exact_core).expect("exact Core digests"),
        digest_core_module(&max_only_core).expect("max-only Core digests"),
    );
}

#[test]
fn imported_nominal_byte_interval_preserves_both_bounds() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as text;\n\
        type Input = { slice: text.BoundedSlice, };\n\
        type Output = { slice: text.BoundedSlice, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          return { slice: input.slice };\n\
        }";
    let lawpack = example_bounds_lawpack();
    let context = pure_context().with_type_shape(TypeShapeFact {
        lawpack,
        coordinate: "example.bounds@1.BoundedSlice".to_owned(),
        definition: "Nominal<Bytes<min=4,max=8>>".to_owned(),
    });
    let module = parse_module(source).expect("bounded imported byte source parses");
    let core = compile_to_core(&module, &context)
        .expect("bounded imported byte interval compiles to Core");

    for parent in ["Input", "Output"] {
        let Some(CoreType::Record { fields }) = core.types.get(parent) else {
            panic!("{parent} must remain a named record");
        };
        assert_eq!(
            fields.get("slice").map(String::as_str),
            Some("example.bounds@1.BoundedSlice")
        );
        assert!(!core.types.contains_key(&format!("{parent}.slice")));
    }
    let coordinate = "example.bounds@1.BoundedSlice";
    {
        assert_eq!(
            core.types.get(coordinate),
            Some(&CoreType::Nominal {
                contract: "example.bounds@1.BoundedSlice".to_owned(),
                representation: "Bytes<min=4,max=8>".to_owned(),
            }),
            "{coordinate} must retain the imported byte interval",
        );
    }
}

#[test]
fn imported_nominal_identifiers_reject_cross_assignment() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as text;\n\
        type Input = { bufferId: text.BufferId, basisHeadId: text.HeadId, };\n\
        type Output = { bufferId: text.BufferId, basisHeadId: text.HeadId, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis input.basisHeadId\n\
          budget <= p.tiny {\n\
          return { bufferId: input.basisHeadId, basisHeadId: input.bufferId };\n\
        }";
    let lawpack = example_bounds_lawpack();
    let context = pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.BufferId".to_owned(),
            definition: "Nominal<Bytes<exact=32>>".to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack,
            coordinate: "example.bounds@1.HeadId".to_owned(),
            definition: "Nominal<Bytes<exact=32>>".to_owned(),
        });
    let module = parse_module(source).expect("nominal identifier source parses");
    let failures = compile_to_core(&module, &context)
        .expect_err("distinct imported nominal identifiers must not cross-assign");

    assert!(failures
        .iter()
        .any(|failure| failure.kind == CompilerErrorKind::TypeMismatch));
    assert!(failures
        .iter()
        .all(|failure| failure.kind != CompilerErrorKind::UnresolvedType));
}

#[test]
fn malformed_imported_string_policy_rejects_as_unresolved_type() {
    let lawpack = ResourceRef {
        coordinate: "example.bounds@1".to_owned(),
        digest: Some(
            "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        ),
    };
    let context = pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.InvalidString".to_owned(),
            definition: "String<max=32,canonical=raw-utf8,canonical=unexpected>".to_owned(),
        })
        .with_pure_function(
            "helpers.invalidString",
            PureFunctionFact {
                lawpack,
                coordinate: "example.bounds@1.invalidString".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "example.bounds@1.InvalidString".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        );
    let module = parse_module(INVALID_IMPORTED_STRING_HELPER)
        .expect("malformed imported type source parses");

    let errors = compile_to_core(&module, &context)
        .expect_err("malformed imported string policy rejects before Core");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedType));
}

#[test]
fn nested_imported_type_fact_with_foreign_coordinate_rejects_before_core() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
        type Input = { values: List<String<max=32>, max=4>, };\n\
        type Output = { values: List<String<max=32>, max=4>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let values = helpers.identityList(input.values);\n\
          return { values };\n\
        }";
    let lawpack = example_bounds_lawpack();
    let context = pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.Key".to_owned(),
            definition: "String<max=32,canonical=raw-utf8>".to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.KeyList".to_owned(),
            definition: "List<example.bounds@1.Key,max=4>".to_owned(),
        })
        .with_pure_function(
            "helpers.identityList",
            PureFunctionFact {
                lawpack: lawpack.clone(),
                coordinate: "example.bounds@1.identityList".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["example.bounds@1.KeyList".to_owned()],
                return_type: "example.bounds@1.KeyList".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        );
    let module = parse_module(source).expect("nested imported type source parses");
    let mut resolved = resolve_module(&module, &context).expect("source resolves");
    resolved.type_shapes.insert(
        "example.bounds@1.Key".to_owned(),
        TypeShapeFact {
            lawpack,
            coordinate: "other.package@1.Key".to_owned(),
            definition: "String<max=32,canonical=raw-utf8>".to_owned(),
        },
    );

    let errors =
        type_check(&resolved).expect_err("foreign nested type coordinate rejects before Core");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedType));
}

#[test]
fn missing_pure_helper_rejects_before_core() {
    let module = parse_module(PURE_HELPER_CALL).expect("pure-helper source parses");
    let errors = compile_to_core(&module, &pure_context()).expect_err("missing helper rejects");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedFunction));
}

#[test]
fn pure_helper_fact_without_exact_owning_import_rejects_before_core() {
    let source = PURE_HELPER_CALL.replace(
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "sha256:2222222222222222222222222222222222222222222222222222222222222222",
    );
    let module = parse_module(&source).expect("pure-helper source parses");
    let errors = compile_to_core(&module, &pure_helper_context("U64"))
        .expect_err("unowned helper fact rejects");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedFunction));
}

#[test]
fn pure_helper_fact_with_foreign_coordinate_rejects_before_core() {
    let module = parse_module(PURE_HELPER_CALL).expect("pure-helper source parses");
    let context = pure_helper_context("U64").with_pure_function(
        "helpers.bump",
        PureFunctionFact {
            lawpack: example_bounds_lawpack(),
            coordinate: "other.package@1.bump".to_owned(),
            type_parameters: Vec::new(),
            parameter_types: vec!["U64".to_owned()],
            return_type: "U64".to_owned(),
            cost_template: "example.bounds@1.tiny".to_owned(),
        },
    );

    let errors = compile_to_core(&module, &context)
        .expect_err("foreign helper coordinate rejects despite exact lawpack resource");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedFunction));
}

#[test]
fn pure_helper_source_qualifier_must_match_the_owning_import() {
    for (source_coordinate, canonical_coordinate) in [
        ("wrong.bump", "example.bounds@1.bump"),
        ("helpers.other", "example.bounds@1.bump"),
    ] {
        let source = PURE_HELPER_CALL.replace("helpers.bump", source_coordinate);
        let module = parse_module(&source).expect("misqualified helper source parses");
        let context = pure_context().with_pure_function(
            source_coordinate,
            PureFunctionFact {
                lawpack: example_bounds_lawpack(),
                coordinate: canonical_coordinate.to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "U64".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        );

        let errors = compile_to_core(&module, &context)
            .expect_err("wrong helper alias or suffix rejects before Core");

        assert!(errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::UnresolvedFunction));
    }
}

#[test]
fn compatible_helper_conditionals_are_branch_order_independent() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
        type Input = { value: U64, };\n\
        type Output = { value: String<max=8>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let value = if true then helpers.short(input.value) else helpers.long(input.value);\n\
          return { value };\n\
        }";
    let context = bounded_string_helper_context();
    let mirrored = source
        .replace("helpers.short(input.value)", "helpers.swap(input.value)")
        .replace("helpers.long(input.value)", "helpers.short(input.value)")
        .replace("helpers.swap(input.value)", "helpers.long(input.value)");
    assert_ne!(mirrored, source, "mirrored helper order changes source");

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("bounded helper conditional parses");
        compile_to_core(&module, &context)
            .expect("compatible helper conditional compiles in either branch order");
    }
}

#[test]
fn non_nested_byte_intervals_join_independently_of_branch_order() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
        type Input = { value: U64, };\n\
        type Output = { value: Bytes<max=5>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let value = if true then helpers.low(input.value) else helpers.high(input.value);\n\
          return { value };\n\
        }";
    let mirrored = source
        .replace("helpers.low(input.value)", "helpers.swap(input.value)")
        .replace("helpers.high(input.value)", "helpers.low(input.value)")
        .replace("helpers.swap(input.value)", "helpers.high(input.value)");
    assert_ne!(mirrored, source, "mirrored helper order changes source");
    let context = ranged_byte_helper_context();

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("ranged byte conditional parses");
        let core = compile_to_core(&module, &context)
            .expect("non-nested byte intervals join in either branch order");
        let CoreNode::Let { binding, .. } = &core.intents.get("t").expect("intent t").body.nodes[0]
        else {
            panic!("conditional lowers as one binding");
        };
        assert_eq!(binding.ty, "Bytes<min=2,max=5>");
    }
}

fn bounded_string_helper_context() -> CompilerContext {
    let lawpack = example_bounds_lawpack();
    pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.Short".to_owned(),
            definition: "String<max=4,canonical=raw-utf8>".to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.Long".to_owned(),
            definition: "String<max=8,canonical=raw-utf8>".to_owned(),
        })
        .with_pure_function(
            "helpers.short",
            PureFunctionFact {
                lawpack: lawpack.clone(),
                coordinate: "example.bounds@1.short".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "example.bounds@1.Short".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        )
        .with_pure_function(
            "helpers.long",
            PureFunctionFact {
                lawpack,
                coordinate: "example.bounds@1.long".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "example.bounds@1.Long".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        )
        .with_pure_helper_cost(
            "helpers.tiny",
            example_helper_cost(CoreBudget {
                max_steps: 5,
                max_allocated_bytes: 8,
                max_output_bytes: 8,
            }),
        )
}

fn ranged_byte_helper_context() -> CompilerContext {
    let lawpack = example_bounds_lawpack();
    pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.LowBytes".to_owned(),
            definition: "Bytes<min=2,max=4>".to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.HighBytes".to_owned(),
            definition: "Bytes<min=3,max=5>".to_owned(),
        })
        .with_pure_function(
            "helpers.low",
            PureFunctionFact {
                lawpack: lawpack.clone(),
                coordinate: "example.bounds@1.low".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "example.bounds@1.LowBytes".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        )
        .with_pure_function(
            "helpers.high",
            PureFunctionFact {
                lawpack,
                coordinate: "example.bounds@1.high".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "example.bounds@1.HighBytes".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        )
        .with_pure_helper_cost(
            "helpers.tiny",
            example_helper_cost(CoreBudget {
                max_steps: 5,
                max_allocated_bytes: 8,
                max_output_bytes: 8,
            }),
        )
}

#[test]
fn pure_helper_costs_are_charged_at_call_sites_and_inside_loops() {
    let direct = parse_module(PURE_HELPER_CALL).expect("direct helper source parses");
    let direct_context = pure_helper_context("U64").with_pure_helper_cost(
        "helpers.tiny",
        example_helper_cost(CoreBudget {
            max_steps: 9,
            max_allocated_bytes: 8,
            max_output_bytes: 8,
        }),
    );
    let direct_errors = compile_to_core(&direct, &direct_context)
        .expect_err("helper cost above the operation step budget rejects");
    assert!(direct_errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));

    for helper_cost in [
        CoreBudget {
            max_steps: 1,
            max_allocated_bytes: 1_025,
            max_output_bytes: 8,
        },
        CoreBudget {
            max_steps: 1,
            max_allocated_bytes: 8,
            max_output_bytes: 257,
        },
    ] {
        let context = pure_helper_context("U64")
            .with_pure_helper_cost("helpers.tiny", example_helper_cost(helper_cost));
        let errors = compile_to_core(&direct, &context)
            .expect_err("every helper cost dimension is budgeted");
        assert!(errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound));
    }

    let missing_cost_context = pure_context_without_helper_cost().with_pure_function(
        "helpers.bump",
        PureFunctionFact {
            lawpack: example_bounds_lawpack(),
            coordinate: "example.bounds@1.bump".to_owned(),
            type_parameters: Vec::new(),
            parameter_types: vec!["U64".to_owned()],
            return_type: "U64".to_owned(),
            cost_template: "example.bounds@1.tiny".to_owned(),
        },
    );
    let missing_cost_errors = compile_to_core(&direct, &missing_cost_context)
        .expect_err("unresolved helper cost template rejects");
    assert!(missing_cost_errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));

    let control = parse_module(EXPORTED_LIST_LOOP).expect("loop control source parses");
    compile_to_core(&control, &loop_helper_context())
        .expect("outside-loop identity helper plus structural loop work fits the budget");

    let loop_source = EXPORTED_LIST_LOOP.replace(
        "require value == value else example.InvalidValue;",
        "let bumped: U64 = helpers.bump(0u64);\n        require bumped == bumped else example.InvalidValue;",
    );
    let loop_module = parse_module(&loop_source).expect("loop helper source parses");
    let loop_errors = compile_to_core(&loop_module, &loop_helper_context())
        .expect_err("structural loop work and helper costs share one step budget");
    assert!(loop_errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));
}

#[test]
fn mutually_exclusive_helper_and_loop_work_share_branch_correlation() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
        type Input = { choose: U32, values: List<U64, max=4>, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          if input.choose == 0u32 {\n\
            let bumped: U64 = helpers.bump(0u64);\n\
            require bumped == bumped else example.InvalidValue;\n\
          } else {\n\
            for value in input.values bounded 4 {\n\
              require value == value else example.InvalidValue;\n\
            }\n\
          }\n\
          return { value: 0u64 };\n\
        }";
    let module = parse_module(source).expect("branch-correlated cost source parses");
    let context = pure_helper_context("U64").with_pure_helper_cost(
        "helpers.tiny",
        example_helper_cost(CoreBudget {
            max_steps: 5,
            max_allocated_bytes: 8,
            max_output_bytes: 8,
        }),
    );

    compile_to_core(&module, &context)
        .expect("mutually exclusive helper and loop work is charged per path");
}

#[test]
fn imported_type_alias_depth_rejects_with_a_structured_error() {
    let module = parse_module(PURE_HELPER_CALL).expect("pure helper source parses");
    let lawpack = example_bounds_lawpack();
    for count in [128, 129] {
        let mut context = pure_context()
            .with_pure_function(
                "helpers.bump",
                PureFunctionFact {
                    lawpack: lawpack.clone(),
                    coordinate: "example.bounds@1.bump".to_owned(),
                    type_parameters: Vec::new(),
                    parameter_types: vec!["U64".to_owned()],
                    return_type: "example.bounds@1.Alias0".to_owned(),
                    cost_template: "example.bounds@1.tiny".to_owned(),
                },
            )
            .with_pure_helper_cost(
                "helpers.tiny",
                example_helper_cost(CoreBudget {
                    max_steps: 1,
                    max_allocated_bytes: 8,
                    max_output_bytes: 8,
                }),
            );
        for index in 0..count {
            context = context.with_type_shape(TypeShapeFact {
                lawpack: lawpack.clone(),
                coordinate: format!("example.bounds@1.Alias{index}"),
                definition: if index + 1 == count {
                    "U64".to_owned()
                } else {
                    format!("example.bounds@1.Alias{}", index + 1)
                },
            });
        }

        if count == 128 {
            compile_to_core(&module, &context)
                .expect("imported alias chain at the depth boundary compiles");
        } else {
            let errors = compile_to_core(&module, &context).expect_err(
                "over-depth imported alias chain must reject without recursion failure",
            );
            assert!(errors
                .iter()
                .any(|error| error.kind == CompilerErrorKind::UnresolvedType));
        }
    }
}

#[test]
fn unresolved_loop_bound_fails_closed_during_helper_cost_accounting() {
    let source = EXPORTED_LIST_LOOP
        .replace("bounded 4", "bounded helpers.maxItems")
        .replace(
            "require value == value else example.InvalidValue;",
            "let bumped: U64 = helpers.bump(0u64);\n        require bumped == bumped else example.InvalidValue;",
        );
    let module = parse_module(&source).expect("unresolved helper-loop bound source parses");

    let errors = compile_to_core(&module, &loop_helper_context())
        .expect_err("helper cost accounting must reject an unresolved loop bound itself");

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].stage, CompilerStage::TypeCheck);
    assert_eq!(errors[0].kind, CompilerErrorKind::MissingContextFact);
    assert_eq!(
        errors[0].message,
        "bounded-loop helper cost requires a resolvable loop bound"
    );
}

#[test]
fn pure_helper_cost_requires_exact_lawpack_ownership() {
    let module = parse_module(PURE_HELPER_CALL).expect("direct helper source parses");
    let context = pure_context_without_helper_cost()
        .with_budget(
            "helpers.tiny",
            CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 8,
                max_output_bytes: 8,
            },
        )
        .with_pure_function(
            "helpers.bump",
            PureFunctionFact {
                lawpack: example_bounds_lawpack(),
                coordinate: "example.bounds@1.bump".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "U64".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        );
    let errors = compile_to_core(&module, &context)
        .expect_err("an unowned raw budget cannot authorize an imported helper cost");
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));

    let mut foreign_lawpack = example_bounds_lawpack();
    foreign_lawpack.digest =
        Some("sha256:2222222222222222222222222222222222222222222222222222222222222222".to_owned());
    let foreign_context = pure_helper_context("U64").with_pure_helper_cost(
        "helpers.tiny",
        PureHelperCostFact {
            lawpack: foreign_lawpack,
            coordinate: "example.bounds@1.tiny".to_owned(),
            budget: CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 8,
                max_output_bytes: 8,
            },
        },
    );
    let foreign_errors = compile_to_core(&module, &foreign_context)
        .expect_err("another lawpack digest cannot supply the helper cost value");
    assert!(foreign_errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));
}

fn loop_helper_context() -> CompilerContext {
    let lawpack = example_bounds_lawpack();
    pure_context()
        .with_pure_helper_cost(
            "helpers.tiny",
            example_helper_cost(CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 8,
                max_output_bytes: 8,
            }),
        )
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.Key".to_owned(),
            definition: "String<max=32,canonical=raw-utf8>".to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.KeyList".to_owned(),
            definition: "List<example.bounds@1.Key,max=4>".to_owned(),
        })
        .with_pure_function(
            "helpers.identityList",
            PureFunctionFact {
                lawpack: lawpack.clone(),
                coordinate: "example.bounds@1.identityList".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["example.bounds@1.KeyList".to_owned()],
                return_type: "example.bounds@1.KeyList".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        )
        .with_pure_function(
            "helpers.bump",
            PureFunctionFact {
                lawpack,
                coordinate: "example.bounds@1.bump".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["U64".to_owned()],
                return_type: "U64".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        )
}

#[test]
fn pure_helper_argument_type_mismatch_rejects_before_core() {
    let module = parse_module(PURE_HELPER_CALL).expect("pure-helper source parses");
    let errors = compile_to_core(&module, &pure_helper_context("Bool"))
        .expect_err("helper argument mismatch rejects");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn bounded_list_loop_lowers_to_core() {
    let module = parse_module(BOUNDED_LIST_LOOP).expect("bounded loop parses");
    let core = compile_to_core(&module, &pure_context()).expect("bounded loop lowers");
    let intent = core.intents.get("t").expect("lowered intent");
    let CoreNode::For {
        binder,
        bound,
        body,
        ..
    } = &intent.body.nodes[0]
    else {
        panic!("first node is a Core for");
    };

    assert_eq!(binder.ty, "U64");
    assert_eq!(bound, &CoreBound::Literal(4));
    assert!(matches!(body.nodes.as_slice(), [CoreNode::Require { .. }]));
    assert_eq!(body.result, CoreExpr::Const(edict_syntax::CoreValue::Null));
}

#[test]
fn exported_list_loop_preserves_the_declared_item_coordinate() {
    let lawpack = example_bounds_lawpack();
    let context = pure_context()
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.Key".to_owned(),
            definition: "String<max=32,canonical=raw-utf8>".to_owned(),
        })
        .with_type_shape(TypeShapeFact {
            lawpack: lawpack.clone(),
            coordinate: "example.bounds@1.KeyList".to_owned(),
            definition: "List<example.bounds@1.Key,max=4>".to_owned(),
        })
        .with_pure_function(
            "helpers.identityList",
            PureFunctionFact {
                lawpack,
                coordinate: "example.bounds@1.identityList".to_owned(),
                type_parameters: Vec::new(),
                parameter_types: vec!["example.bounds@1.KeyList".to_owned()],
                return_type: "example.bounds@1.KeyList".to_owned(),
                cost_template: "example.bounds@1.tiny".to_owned(),
            },
        );
    let module = parse_module(EXPORTED_LIST_LOOP).expect("exported-list loop parses");
    let core = compile_to_core(&module, &context).expect("exported-list loop lowers");
    let intent = core.intents.get("t").expect("lowered intent");
    let CoreNode::For { binder, .. } = &intent.body.nodes[1] else {
        panic!("second node is a Core for");
    };

    assert_eq!(binder.ty, "example.bounds@1.Key");
}

#[test]
fn bounded_list_loop_rejects_unsound_or_over_budget_bounds() {
    for source in [
        BOUNDED_LIST_LOOP.replace("bounded 4", "bounded 3"),
        BOUNDED_LIST_LOOP.replace("bounded 4", "bounded 9"),
    ] {
        let module = parse_module(&source).expect("invalid bounded loop still parses");
        let errors =
            compile_to_core(&module, &pure_context()).expect_err("invalid loop bound rejects");
        assert!(errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound));
    }
}

#[test]
fn bounded_list_loops_reject_cumulative_and_nested_over_budget_work() {
    let sequential = "package a.b@1;\n\
        type Input = { items: List<U64, max=8>, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          for left in input.items bounded 8 { require left <= 10u64 else example.TooLarge; }\n\
          for right in input.items bounded 8 { require right <= 10u64 else example.TooLarge; }\n\
          return { value: 0u64 };\n\
        }";
    let nested = "package a.b@1;\n\
        type Input = { batches: List<List<U64, max=4>, max=4>, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          for batch in input.batches bounded 4 {\n\
            for item in batch bounded 4 { require item <= 10u64 else example.TooLarge; }\n\
          }\n\
          return { value: 0u64 };\n\
        }";

    for source in [sequential, nested] {
        let module = parse_module(source).expect("over-budget loop source parses");
        let errors =
            compile_to_core(&module, &pure_context()).expect_err("cumulative loop work rejects");
        assert!(errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound));
    }
}

#[test]
fn bounded_list_loop_bound_mutation_moves_core_digest() {
    let original = parse_module(BOUNDED_LIST_LOOP).expect("bounded loop parses");
    let wider_source = BOUNDED_LIST_LOOP.replace("bounded 4", "bounded 5");
    let wider = parse_module(&wider_source).expect("wider safe bound parses");
    let original_core = compile_to_core(&original, &pure_context()).expect("original compiles");
    let wider_core = compile_to_core(&wider, &pure_context()).expect("wider compiles");

    assert_ne!(
        digest_core_module(&original_core).expect("digest original loop"),
        digest_core_module(&wider_core).expect("digest wider loop")
    );
}

#[test]
fn digest_bound_coordinate_loop_cap_lowers_to_core() {
    let source = coordinate_bounded_loop_source();
    let module = parse_module(&source).expect("coordinate-bounded loop parses");
    let core = compile_to_core(&module, &coordinate_bound_context())
        .expect("coordinate-bounded loop lowers");
    let intent = core.intents.get("t").expect("lowered intent");
    let CoreNode::For { bound, .. } = &intent.body.nodes[0] else {
        panic!("first node is a Core for");
    };

    assert_eq!(
        bound,
        &CoreBound::Coordinate("example.bounds@1.maxItems".to_owned())
    );
}

#[test]
fn missing_coordinate_loop_cap_rejects_before_core() {
    let source = coordinate_bounded_loop_source();
    let module = parse_module(&source).expect("coordinate-bounded loop parses");
    let errors = compile_to_core(&module, &pure_context()).expect_err("missing bound fact rejects");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));
}

#[test]
fn coordinate_bound_fact_without_exact_owning_import_rejects_before_core() {
    let source = BOUNDED_LIST_LOOP.replace("bounded 4", "bounded bounds.maxItems");
    let module = parse_module(&source).expect("unowned coordinate-bound source parses");

    let errors = compile_to_core(&module, &coordinate_bound_context())
        .expect_err("bound fact without its exact lawpack import rejects");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));
}

#[test]
fn coordinate_bound_fact_with_foreign_coordinate_rejects_before_core() {
    let source = coordinate_bounded_loop_source();
    let module = parse_module(&source).expect("coordinate-bounded loop source parses");
    let context = coordinate_bound_context().with_bound(
        "bounds.maxItems",
        BoundFact {
            lawpack: example_bounds_lawpack(),
            coordinate: "other.package@1.maxItems".to_owned(),
            value: 4,
        },
    );

    let errors = compile_to_core(&module, &context)
        .expect_err("foreign bound coordinate rejects despite exact lawpack resource");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));
}

#[test]
fn coordinate_bound_source_qualifier_must_match_the_owning_import() {
    let source = coordinate_bounded_loop_source().replace("bounds.maxItems", "wrong.maxItems");
    let module = parse_module(&source).expect("misqualified coordinate-bound source parses");
    let context = pure_context().with_bound(
        "wrong.maxItems",
        BoundFact {
            lawpack: example_bounds_lawpack(),
            coordinate: "example.bounds@1.maxItems".to_owned(),
            value: 4,
        },
    );

    let errors = compile_to_core(&module, &context)
        .expect_err("wrong coordinate-bound alias rejects before Core");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::MissingContextFact));
}

#[test]
fn coordinate_loop_cap_rejects_unsound_or_over_budget_fact_values() {
    let source = coordinate_bounded_loop_source();
    let module = parse_module(&source).expect("coordinate-bounded loop parses");
    for value in [3, 9] {
        let context = pure_context().with_bound(
            "bounds.maxItems",
            BoundFact {
                lawpack: example_bounds_lawpack(),
                coordinate: "example.bounds@1.maxItems".to_owned(),
                value,
            },
        );
        let errors = compile_to_core(&module, &context).expect_err("invalid bound fact rejects");
        assert!(errors
            .iter()
            .any(|error| error.kind == CompilerErrorKind::InvalidBound));
    }
}

#[test]
fn statement_conditional_lowers_to_isolated_core_branches() {
    let module = parse_module(STATEMENT_CONDITIONAL).expect("statement conditional parses");
    let core = compile_to_core(&module, &pure_context()).expect("statement conditional lowers");
    let intent = core.intents.get("t").expect("lowered intent");
    let CoreNode::Branch {
        binding,
        predicate,
        then_block,
        else_block,
    } = &intent.body.nodes[0]
    else {
        panic!("first node is a Core branch");
    };

    assert!(binding.is_none());
    assert!(matches!(predicate, CorePredicate::Compare { .. }));
    assert!(matches!(
        then_block.nodes.as_slice(),
        [CoreNode::Let { .. }, CoreNode::Require { .. }]
    ));
    assert!(matches!(
        else_block.nodes.as_slice(),
        [CoreNode::Require { .. }]
    ));
}

#[test]
fn statement_conditional_does_not_leak_locals() {
    let source =
        STATEMENT_CONDITIONAL.replace("return { value: input.left }", "return { value: selected }");
    let module = parse_module(&source).expect("leaking conditional source parses");
    let errors = compile_to_core(&module, &pure_context())
        .expect_err("branch-local binding must not escape");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedType));
}

#[test]
fn statement_conditional_rejects_branch_return() {
    let source = STATEMENT_CONDITIONAL.replace(
        "let selected: U64 = input.left;",
        "return { value: input.left };",
    );
    let module = parse_module(&source).expect("branch return source parses");
    let errors = compile_to_core(&module, &pure_context()).expect_err("branch return must reject");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnsupportedSourceShape));
}

#[test]
fn missing_context_facts_reject_in_resolve_stage() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let errors =
        resolve_module(&module, &CompilerContext::new()).expect_err("missing context facts reject");

    assert!(errors.iter().all(|err| err.stage == CompilerStage::Resolve));
    assert_eq!(
        errors
            .iter()
            .map(|err| err.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![
            CompilerErrorKind::MissingContextFact,
            CompilerErrorKind::MissingContextFact,
        ]
    );
}

#[test]
fn unresolved_local_types_reject_in_type_check_stage() {
    let module = parse_module(
        "package a.b@1;\n\
         intent t(input: MissingInput) returns MissingOutput\n\
           profile p.read\n\
           basis none\n\
           budget <= p.tiny {\n\
           return { input };\n\
         }",
    )
    .expect("source parses");
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 1,
                max_output_bytes: 1,
            },
        );

    let resolved = resolve_module(&module, &context).expect("resolve accepts source coordinates");
    let errors = type_check(&resolved).expect_err("unknown types reject in type-check");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|err| err.kind == CompilerErrorKind::UnresolvedType));
}

#[test]
fn unresolved_record_field_types_reject_in_type_check_stage() {
    let module = parse_module(
        "package a.b@1;\n\
         type Box = { value: MissingValue, };\n\
         intent t(input: Box) returns Box\n\
           profile p.read\n\
           basis none\n\
           budget <= p.tiny {\n\
           return { input };\n\
         }",
    )
    .expect("source parses");
    let context = CompilerContext::new()
        .with_operation_profile("p.read", "continuum.profile.read-only/v1")
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 1,
                max_output_bytes: 1,
            },
        );

    let resolved = resolve_module(&module, &context).expect("resolve accepts source coordinates");
    let errors = type_check(&resolved).expect_err("unknown field type rejects in type-check");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|err| err.kind == CompilerErrorKind::UnresolvedType));
}

#[test]
fn record_return_shape_mismatch_rejects_in_type_check_stage() {
    let source = BOUNDED_HELLO.replace("return { message };", "return { wrong: message };");
    let module = parse_module(&source).expect("source parses");
    let resolved = resolve_module(&module, &hello_context()).expect("resolve stage");
    let errors = type_check(&resolved).expect_err("return shape rejects");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|err| err.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn missing_return_rejects_in_type_check_stage() {
    let source = BOUNDED_HELLO.replace("  return { message };\n", "");
    let module = parse_module(&source).expect("source parses");
    let resolved = resolve_module(&module, &hello_context()).expect("resolve stage");
    let errors = type_check(&resolved).expect_err("missing return rejects");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|err| err.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn read_only_profile_rejects_write_effect_body() {
    let module = parse_module(
        "package a.b@1;\n\
         type Input = { id: String<max=16>, };\n\
         type Output = { id: String<max=16>, };\n\
         intent t(input: Input) returns Output\n\
           profile p.readOnly\n\
           basis none\n\
           budget <= p.tiny {\n\
           target.replace(input.id) else domain.WriteRejected;\n\
           return { id: input.id };\n\
         }",
    )
    .expect("source parses");
    let context = CompilerContext::new()
        .with_operation_profile("p.readOnly", "continuum.profile.read-only/v1")
        .with_operation_profile_write_classes("p.readOnly", [WriteClass::Read])
        .with_effect_write_class("target.replace", WriteClass::Replace)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 1,
                max_output_bytes: 1,
            },
        );

    let errors = compile_to_core(&module, &context)
        .expect_err("write effect rejects under read-only profile");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|err| err.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::ProfileEffectMismatch]
    );
}

#[test]
fn read_only_profile_rejects_write_effect_let_without_else() {
    let module = parse_module(
        "package a.b@1;\n\
         type Input = { id: String<max=16>, };\n\
         type Output = { id: String<max=16>, };\n\
         intent t(input: Input) returns Output\n\
           profile p.readOnly\n\
           basis none\n\
           budget <= p.tiny {\n\
           let _receipt = target.replace(input.id);\n\
           return { id: input.id };\n\
         }",
    )
    .expect("source parses");
    let context = CompilerContext::new()
        .with_operation_profile("p.readOnly", "continuum.profile.read-only/v1")
        .with_operation_profile_write_classes("p.readOnly", [WriteClass::Read])
        .with_effect_write_class("target.replace", WriteClass::Replace)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 1,
                max_allocated_bytes: 1,
                max_output_bytes: 1,
            },
        );

    let errors = compile_to_core(&module, &context)
        .expect_err("write effect rejects under read-only profile");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|err| err.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::ProfileEffectMismatch]
    );
}

#[test]
fn effectful_write_intent_lowers_to_typed_core_from_file_backed_facts() {
    let dir = temp_case_dir("effectful-write");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        effectful_target_profile_facts(),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", effectful_lawpack_facts());
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    let module = parse_module(EFFECTFUL_REPLACE).expect("effectful source parses");
    let core = compile_to_core(&module, &context).expect("effectful source compiles to Core");
    let intent = core.intents.get("t").expect("compiled effectful intent");

    assert_eq!(
        intent.required_operation_profile,
        "continuum.profile.write/v1"
    );
    assert_eq!(intent.body.nodes.len(), 1);

    let CoreNode::Effect {
        binding,
        effect,
        input,
        obstruction_map,
    } = &intent.body.nodes[0]
    else {
        panic!("effectful source lowers to a semantic effect node");
    };

    assert_eq!(binding.id, "local.0");
    assert_eq!(binding.alpha_name, "$local0");
    assert_eq!(binding.ty, "a.b@1.Receipt");
    assert_eq!(effect, "target.replace");

    let CoreExpr::Field { base, field } = input else {
        panic!("effect input preserves the source argument expression");
    };
    assert_eq!(field, "id");
    assert!(matches!(base.as_ref(), CoreExpr::Local { reference } if reference.id == "arg.0"));

    let arm = obstruction_map
        .get("rejected")
        .expect("failure arm is keyed by low-level failure coordinate");
    assert_eq!(arm.binder.id, "obstruction.0");
    assert_eq!(arm.binder.alpha_name, "$obstruction0");
    assert_eq!(arm.binder.ty, "Unit");
    assert!(matches!(
        &arm.value,
        CoreExpr::Call { callee, args, .. } if callee == "domain.WriteRejected" && args.is_empty()
    ));
}

#[test]
fn effectful_branch_yield_lowers_to_bound_core_branch() {
    let dir = temp_case_dir("effectful-branch-yield");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        effectful_target_profile_facts(),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", effectful_lawpack_facts());
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    let module = parse_module(EFFECTFUL_BRANCH_YIELD).expect("effectful branch-yield parses");
    let core = compile_to_core(&module, &context).expect("effectful branch-yield lowers");
    let intent = core.intents.get("t").expect("compiled intent");
    let CoreNode::Branch {
        binding: Some(binding),
        then_block,
        else_block,
        ..
    } = &intent.body.nodes[0]
    else {
        panic!("branch-yield let lowers to one bound Core branch");
    };

    assert_eq!(
        binding.ty, "String<max=16,canonical=raw-utf8>",
        "a join across distinct branch identities is structural"
    );
    assert!(matches!(
        then_block.nodes.as_slice(),
        [CoreNode::Effect { .. }]
    ));
    assert_eq!(else_block.nodes, [] as [edict_syntax::CoreNode; 0]);
    assert!(matches!(then_block.result, CoreExpr::Field { .. }));
    assert!(matches!(
        else_block.result,
        CoreExpr::Const(edict_syntax::CoreValue::String(ref value)) if value == "fallback"
    ));
    assert!(matches!(
        intent.body.result,
        CoreExpr::Record { ref fields }
            if matches!(&fields["id"], CoreExpr::Local { reference } if reference == binding)
    ));
}

#[test]
fn effectful_branch_yield_rejects_incompatible_results() {
    let source = EFFECTFUL_BRANCH_YIELD.replace("yield \"fallback\"", "yield true");
    let module = parse_module(&source).expect("incompatible branch-yield parses");
    let errors = compile_to_core(&module, &effectful_context())
        .expect_err("incompatible branch results reject");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn branch_yield_rejects_incompatible_anonymous_record_shapes() {
    let source = "package a.b@1;\n\
        type Input = { value: U64, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let selected = if true {\n\
            yield { a: 1u64 };\n\
          } else {\n\
            yield { b: 2u64 };\n\
          };\n\
          return { value: selected.a };\n\
        }";
    let module = parse_module(source).expect("anonymous-record branch-yield source parses");

    let errors = compile_to_core(&module, &pure_context())
        .expect_err("incompatible anonymous-record results reject before Core");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::TypeMismatch));
}

#[test]
fn branch_yield_inside_loop_preserves_cumulative_budget() {
    let source = "package a.b@1;\n\
        type Input = { batches: List<List<U64, max=4>, max=4>, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          for batch in input.batches bounded 4 {\n\
            let retained = if true {\n\
              for item in batch bounded 4 {\n\
                require item <= 10u64 else example.TooLarge;\n\
              }\n\
              yield batch;\n\
            } else {\n\
              yield batch;\n\
            };\n\
          }\n\
          return { value: 0u64 };\n\
        }";
    let module = parse_module(source).expect("nested branch-yield loop source parses");

    let errors = compile_to_core(&module, &pure_context())
        .expect_err("branch-local loop work keeps the enclosing step factor");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::InvalidBound));

    let control = source.replace(
        "for item in batch bounded 4 {\n\
require item <= 10u64 else example.TooLarge;\n\
}\n",
        "",
    );
    assert_ne!(control, source, "calibrated loop removal changes source");
    let control_module = parse_module(&control).expect("branch-yield budget control source parses");
    compile_to_core(&control_module, &pure_context())
        .expect("the same source without branch-local loop work stays in budget");
}

#[test]
fn branch_yield_does_not_leak_locals() {
    let source = "package a.b@1;\n\
        type Input = { value: U64, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let selected = if true {\n\
            let branch_local = input.value;\n\
            yield branch_local;\n\
          } else {\n\
            yield input.value;\n\
          };\n\
          return { value: branch_local };\n\
        }";
    let module = parse_module(source).expect("branch-yield leak source parses");
    let errors = compile_to_core(&module, &pure_context())
        .expect_err("branch-yield-local binding must not escape");

    assert!(errors
        .iter()
        .any(|error| error.kind == CompilerErrorKind::UnresolvedType));
}

#[test]
fn branch_yield_bare_integer_inherits_width_from_either_branch() {
    let source = "package a.b@1;\n\
        type Input = { value: U64, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let value = if true { yield 0; } else { yield input.value; };\n\
          return { value };\n\
        }";
    let mirrored = source
        .replace("yield 0;", "yield __typed;")
        .replace("yield input.value;", "yield 0;")
        .replace("yield __typed;", "yield input.value;");

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("fixed-width branch-yield source parses");
        let core = compile_to_core(&module, &pure_context())
            .expect("either branch supplies the bare integer width");
        let intent = core.intents.get("t").expect("compiled intent");
        let CoreNode::Branch {
            binding: Some(binding),
            ..
        } = &intent.body.nodes[0]
        else {
            panic!("branch-yield lowers with one binding");
        };

        assert_eq!(binding.ty, "U64");
    }
}

#[test]
fn branch_yield_integer_inference_preserves_source_order_local_identities() {
    let bare = "package a.b@1;\n\
        type Input = { value: U64, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let value = if true {\n\
            let then_local = input.value;\n\
            yield 0;\n\
          } else {\n\
            let else_local = input.value;\n\
            yield else_local;\n\
          };\n\
          return { value };\n\
        }";
    let typed = bare.replace("yield 0;", "yield 0u64;");
    assert_ne!(typed, bare, "typed integer mutation changes source");

    let bare_module = parse_module(bare).expect("bare-integer branch-yield source parses");
    let typed_module = parse_module(&typed).expect("typed-integer branch-yield source parses");
    let bare_core =
        compile_to_core(&bare_module, &pure_context()).expect("bare-integer branch-yield compiles");
    let typed_core = compile_to_core(&typed_module, &pure_context())
        .expect("typed-integer branch-yield compiles");

    assert_eq!(
        digest_core_module(&bare_core).expect("bare-integer Core digests"),
        digest_core_module(&typed_core).expect("typed-integer Core digests")
    );
}

#[test]
fn deeply_nested_branch_yield_integer_inference_compiles() {
    let mut nested_else = "yield input.value;".to_owned();
    for depth in (0..24).rev() {
        nested_else = format!(
            "let level_{depth} = if true {{ yield 0; }} else {{\n{nested_else}\n}};\n\
             yield level_{depth};"
        );
    }
    let source = format!(
        "package a.b@1;\n\
         type Input = {{ value: U64, }};\n\
         type Output = {{ value: U64, }};\n\
         intent t(input: Input) returns Output\n\
           profile p.read\n\
           basis none\n\
           budget <= p.tiny {{\n\
           let selected = if true {{ yield 0; }} else {{\n{nested_else}\n}};\n\
           return {{ value: selected }};\n\
         }}"
    );
    let module = parse_module(&source).expect("nested branch-yield source parses");

    compile_to_core(&module, &pure_context()).expect("deeply nested bare-integer source compiles");
}

#[test]
fn branch_yield_inference_cache_distinguishes_equal_spans() {
    fn resolved_for(width: &str, suffix: &str, intent_name: &str) -> ResolvedModule {
        let source = format!(
            "package a.b@1;\n\
             type Input{suffix} = {{ value: {width}, }};\n\
             type Output{suffix} = {{ value: {width}, }};\n\
             intent {intent_name}(input: Input{suffix}) returns Output{suffix}\n\
               profile p.read\n\
               basis none\n\
               budget <= p.tiny {{\n\
               let value = if true {{ yield 0; }} else {{ yield input.value; }};\n\
               return {{ value }};\n\
             }}"
        );
        let module = parse_module(&source).expect("equal-span branch-yield source parses");
        resolve_module(&module, &pure_context()).expect("equal-span source resolves")
    }

    let mut resolved = resolved_for("U64", "64", "first");
    let second = resolved_for("U32", "32", "other");
    let first_span = match &resolved.intents[0].source.body.stmts[0] {
        edict_syntax::ast::Stmt::Let {
            value: edict_syntax::ast::Expr::IfYield { else_block, .. },
            ..
        } => else_block.span,
        _ => panic!("first intent starts with branch-yield let"),
    };
    let second_span = match &second.intents[0].source.body.stmts[0] {
        edict_syntax::ast::Stmt::Let {
            value: edict_syntax::ast::Expr::IfYield { else_block, .. },
            ..
        } => else_block.span,
        _ => panic!("second intent starts with branch-yield let"),
    };
    assert_eq!(first_span, second_span, "fixture reuses diagnostic spans");

    resolved.types.extend(second.types);
    resolved.intents.extend(second.intents);

    type_check(&resolved).expect("distinct equal-span blocks retain their own inferred widths");
}

#[test]
fn branch_yield_record_bare_integer_inherits_field_width_from_either_branch() {
    let source = "package a.b@1;\n\
        type Input = { value: U64, };\n\
        type Output = { value: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let selected = if true { yield { value: 0 }; } else { yield { value: input.value }; };\n\
          return selected;\n\
        }";
    let mirrored = source
        .replace("yield { value: 0 };", "yield { value: __typed };")
        .replace("yield { value: input.value };", "yield { value: 0 };")
        .replace("yield { value: __typed };", "yield { value: input.value };");
    assert_ne!(mirrored, source, "mirrored aggregate order changes source");

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("aggregate branch-yield source parses");
        compile_to_core(&module, &pure_context())
            .expect("either aggregate branch supplies the bare integer field width");
    }
}

#[test]
fn branch_yield_complementary_record_integers_join_structurally() {
    let source = "package a.b@1;\n\
        type Input = { a: U64, b: U64, };\n\
        type Output = { a: U64, b: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let selected = if true { let then_b = input.b; yield { a: 0, b: then_b }; } else { let else_a = input.a; yield { a: else_a, b: 0 }; };\n\
          return selected;\n\
        }";
    let mirrored = source
        .replace(
            "let then_b = input.b; yield { a: 0, b: then_b };",
            "let then_b = input.a; yield { a: then_b, b: 0 };",
        )
        .replace(
            "let else_a = input.a; yield { a: else_a, b: 0 };",
            "let else_a = input.b; yield { a: 0, b: else_a };",
        );
    assert_ne!(
        mirrored, source,
        "mirrored complementary records change source"
    );

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("complementary record source parses");
        compile_to_core(&module, &pure_context())
            .expect("complementary record fields jointly determine integer widths");
    }
}

#[test]
fn deeply_nested_complementary_record_inference_compiles() {
    let mut nested = String::new();
    for depth in (0..20).rev() {
        nested = format!(
            "let level_{depth} = if true {{\n{nested}\n\
             yield {{ a: 0, b: input.b }};\n\
             }} else {{\n\
             yield {{ a: input.a, b: 0 }};\n\
             }};"
        );
    }
    let source = format!(
        "package a.b@1;\n\
         type Input = {{ a: U64, b: U64, }};\n\
         type Output = {{ a: U64, b: U64, }};\n\
         intent t(input: Input) returns Output\n\
           profile p.read\n\
           basis none\n\
           budget <= p.tiny {{\n\
           {nested}\n\
           return level_0;\n\
         }}"
    );
    let module = parse_module(&source).expect("nested complementary source parses");

    compile_to_core(&module, &pure_context())
        .expect("deeply nested complementary record source compiles");
}

#[test]
fn branch_yield_bounded_strings_choose_the_wider_type_in_either_order() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as helpers;\n\
        type Input = { value: U64, };\n\
        type Output = { value: String<max=8>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let value = if true { yield helpers.short(input.value); } else { yield helpers.long(input.value); };\n\
          return { value };\n\
        }";
    let mirrored = source
        .replace("yield helpers.short(input.value);", "yield __wide;")
        .replace(
            "yield helpers.long(input.value);",
            "yield helpers.short(input.value);",
        )
        .replace("yield __wide;", "yield helpers.long(input.value);");
    assert_ne!(mirrored, source, "mirrored branch order changes source");
    let context = bounded_string_helper_context();

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("bounded-string branch-yield source parses");
        let core = compile_to_core(&module, &context)
            .expect("branch-yield chooses the wider compatible string in either order");
        let intent = core.intents.get("t").expect("compiled intent");
        let CoreNode::Branch {
            binding: Some(binding),
            ..
        } = &intent.body.nodes[0]
        else {
            panic!("branch-yield lowers with one binding");
        };

        assert_eq!(
            binding.ty, "String<max=8,canonical=raw-utf8>",
            "a join across distinct aliases has one branch-order-independent identity"
        );
    }
}

#[test]
fn branch_yield_records_join_compatible_fields_independently() {
    let source = "package a.b@1;\n\
        type Input = { value: U64, };\n\
        type Output = { a: String<max=2>, b: String<max=2>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let selected = if true { yield { a: \"x\", b: \"yy\" }; } else { yield { a: \"xx\", b: \"y\" }; };\n\
          return selected;\n\
        }";
    let mirrored = source
        .replace("yield { a: \"x\", b: \"yy\" };", "yield __other__;")
        .replace(
            "yield { a: \"xx\", b: \"y\" };",
            "yield { a: \"x\", b: \"yy\" };",
        )
        .replace("yield __other__;", "yield { a: \"xx\", b: \"y\" };");
    assert_ne!(mirrored, source, "mirrored record branches change source");

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("fieldwise record join source parses");
        compile_to_core(&module, &pure_context())
            .expect("opposing bounded record fields join independently");
    }
}

#[test]
fn branch_yield_lists_join_item_and_length_bounds_independently() {
    let source = "package a.b@1;\n\
        type Input = { choose: U32, left: List<String<max=8>, max=16>, right: List<String<max=16>, max=8>, };\n\
        type Output = { value: List<String<max=16>, max=16>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          let value = if input.choose == 0u32 { yield input.left; } else { yield input.right; };\n\
          return { value };\n\
        }";
    let mirrored = source
        .replace("yield input.left;", "yield __other__;")
        .replace("yield input.right;", "yield input.left;")
        .replace("yield __other__;", "yield input.right;");
    assert_ne!(mirrored, source, "mirrored list branches change source");

    for source in [source.to_owned(), mirrored] {
        let module = parse_module(&source).expect("fieldwise list join source parses");
        let context = pure_context().with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 8,
                max_allocated_bytes: 1024,
                max_output_bytes: 1064,
            },
        );
        compile_to_core(&module, &context)
            .expect("opposing list item and length bounds join independently");
    }
}

#[test]
fn branch_yield_rejects_pure_helper_that_is_a_disallowed_write_effect() {
    let module =
        parse_module(PURE_HELPER_BRANCH_YIELD).expect("pure-helper branch-yield source parses");
    let context = pure_helper_context("U64")
        .with_operation_profile_write_classes("p.read", [WriteClass::Read])
        .with_effect_write_class("helpers.bump", WriteClass::Replace);

    let errors = compile_to_core(&module, &context)
        .expect_err("write effect disguised as a pure helper rejects before Core");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|error| error.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::ProfileEffectMismatch]
    );
}

#[test]
fn pure_helper_coordinate_with_effect_authority_rejects_even_when_profile_allows_write() {
    let module =
        parse_module(PURE_HELPER_BRANCH_YIELD).expect("pure-helper branch-yield source parses");
    let context = pure_helper_context("U64")
        .with_operation_profile_write_classes("p.read", [WriteClass::Read, WriteClass::Replace])
        .with_effect_write_class("helpers.bump", WriteClass::Replace);

    let errors = compile_to_core(&module, &context)
        .expect_err("effect-authorized coordinate cannot lower as a pure helper");

    assert!(errors
        .iter()
        .all(|error| error.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|error| error.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::UnsupportedSourceShape]
    );
}

#[test]
fn effectful_branch_yield_mutation_moves_core_digest() {
    let original = compile_to_core(
        &parse_module(EFFECTFUL_BRANCH_YIELD).expect("original parses"),
        &effectful_context(),
    )
    .expect("original lowers");
    let swapped_source = EFFECTFUL_BRANCH_YIELD
        .replace("yield input.id", "yield __placeholder")
        .replace("yield \"fallback\"", "yield input.id")
        .replace("yield __placeholder", "yield \"fallback\"");
    let swapped = compile_to_core(
        &parse_module(&swapped_source).expect("swapped parses"),
        &effectful_context(),
    )
    .expect("swapped lowers");

    assert_ne!(
        digest_core_module(&original).expect("digest original branch-yield"),
        digest_core_module(&swapped).expect("digest swapped branch-yield")
    );

    let mut unbound = original.clone();
    let CoreNode::Branch { binding, .. } = &mut unbound
        .intents
        .get_mut("t")
        .expect("unbound intent")
        .body
        .nodes[0]
    else {
        panic!("first node remains a branch");
    };
    *binding = None;
    assert_ne!(
        digest_core_module(&original).expect("digest bound branch-yield"),
        digest_core_module(&unbound).expect("digest unbound branch")
    );
}

#[test]
fn duplicate_obstruction_failures_reject_before_core_lowering() {
    let dir = temp_case_dir("duplicate-obstruction-failure");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        effectful_target_profile_facts(),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", effectful_lawpack_facts());
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    let module =
        parse_module(DUPLICATE_OBSTRUCTION_FAILURE).expect("duplicate obstruction source parses");

    let errors =
        compile_to_core(&module, &context).expect_err("duplicate obstruction failure keys reject");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|err| err.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::DuplicateObstructionFailure]
    );
}

#[test]
fn chained_effect_calls_reject_before_core_lowering() {
    let source = EFFECTFUL_REPLACE.replace(
        "target.replace(input.id)",
        "target.replace(input.id)(input.id)",
    );
    let dir = temp_case_dir("chained-effect-call");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        effectful_target_profile_facts(),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", effectful_lawpack_facts());
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    let module = parse_module(&source).expect("chained effect-call source parses");

    let errors = compile_to_core(&module, &context).expect_err("chained effect call rejects");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|err| err.kind == CompilerErrorKind::UnsupportedSourceShape));
}

#[test]
fn typed_effect_calls_reject_before_core_lowering() {
    let source = EFFECTFUL_REPLACE.replace(
        "target.replace(input.id)",
        "target.replace<Receipt>(input.id)",
    );
    let dir = temp_case_dir("typed-effect-call");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        effectful_target_profile_facts(),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", effectful_lawpack_facts());
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    let module = parse_module(&source).expect("typed effect-call source parses");

    let errors = compile_to_core(&module, &context).expect_err("typed effect call rejects");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert!(errors
        .iter()
        .any(|err| err.kind == CompilerErrorKind::UnsupportedSourceShape));
}

#[test]
fn obstruction_binder_ids_are_stable_by_failure_key() {
    let ordered = compile_effectful_source(ORDERED_OBSTRUCTION_FAILURES);
    let reversed = compile_effectful_source(REVERSED_OBSTRUCTION_FAILURES);

    assert_eq!(ordered, reversed);
}

#[test]
fn terminal_require_obstruction_lowers_to_core_failure_arm() {
    let core = compile_pure_source(TERMINAL_REQUIRE_OBSTRUCTION);
    let CoreNode::Require { predicate, arm } = only_require_node(&core) else {
        panic!("terminal require lowers to Core require node");
    };

    assert_eq!(predicate, &CorePredicate::True);
    let CoreRequireFailureArm::Terminal { reason } = arm else {
        panic!("terminal require obstruction remains terminal in Core");
    };
    assert_reason(reason, "jim.EditObstruction.StaleBase", []);
}

#[test]
fn terminal_require_preserves_reason_payload_field() {
    let core = compile_pure_source(TERMINAL_REQUIRE_WITH_REASON_PAYLOAD);
    let CoreNode::Require { arm, .. } = only_require_node(&core) else {
        panic!("terminal require lowers to Core require node");
    };
    let CoreRequireFailureArm::Terminal { reason } = arm else {
        panic!("terminal require obstruction remains terminal in Core");
    };

    assert_reason(reason, "jim.EditObstruction.StaleBase", ["reason"]);
    assert!(matches!(
        &reason.payload["reason"],
        CoreExpr::Field { field, .. } if field == "id"
    ));
}

#[test]
fn continue_obstructed_require_lowers_to_core_failure_arm() {
    let core = compile_pure_source(CONTINUE_OBSTRUCTED_REQUIRE);
    let CoreNode::Require { predicate, arm } = only_require_node(&core) else {
        panic!("continue obstructed require lowers to Core require node");
    };

    assert_eq!(predicate, &CorePredicate::True);
    let CoreRequireFailureArm::ContinueObstructed { reason } = arm else {
        panic!("continue obstructed source remains preserved in Core");
    };
    assert_reason(reason, "jim.EditObstruction.StaleBase", ["provided"]);
    assert!(matches!(
        &reason.payload["provided"],
        CoreExpr::Field { field, .. } if field == "id"
    ));
}

#[test]
fn assert_reason_matches_payload_key_sets_without_order_sensitivity() {
    let source = replace_required(
        CONTINUE_OBSTRUCTED_REQUIRE,
        "provided: input.id,",
        "provided: input.id,\nechoed: input.id,",
    );
    let core = compile_pure_source(&source);
    let CoreNode::Require { arm, .. } = only_require_node(&core) else {
        panic!("continue obstructed require lowers to Core require node");
    };
    let CoreRequireFailureArm::ContinueObstructed { reason } = arm else {
        panic!("continue obstructed source remains preserved in Core");
    };

    assert_reason(
        reason,
        "jim.EditObstruction.StaleBase",
        ["provided", "echoed"],
    );
}

#[test]
fn terminal_and_continue_obstructed_require_arms_are_core_distinct() {
    let terminal = compile_pure_source(TERMINAL_REQUIRE_OBSTRUCTION);
    let continued = compile_pure_source(&replace_required(
        CONTINUE_OBSTRUCTED_REQUIRE,
        "provided: input.id,\n",
        "",
    ));

    assert_ne!(terminal, continued);
    assert_ne!(
        digest_core_module(&terminal).expect("terminal Core digests"),
        digest_core_module(&continued).expect("continued Core digests")
    );
}

#[test]
fn obstruction_reason_mutations_move_core_digest() {
    let baseline = compile_pure_source(CONTINUE_OBSTRUCTED_REQUIRE);
    let changed_reason = compile_pure_source(&replace_required(
        CONTINUE_OBSTRUCTED_REQUIRE,
        "jim.EditObstruction.StaleBase",
        "jim.EditObstruction.Other",
    ));
    let changed_payload = compile_pure_source(&replace_required(
        CONTINUE_OBSTRUCTED_REQUIRE,
        "provided: input.id",
        "provided: \"changed\"",
    ));
    let reordered_payload = compile_pure_source(&replace_required(
        CONTINUE_OBSTRUCTED_REQUIRE,
        "reason: jim.EditObstruction.StaleBase,\nprovided: input.id,\n",
        "provided: input.id,\nreason: jim.EditObstruction.StaleBase,\n",
    ));
    let reformatted = compile_pure_source(&replace_required(
        CONTINUE_OBSTRUCTED_REQUIRE,
        "require true else continue obstructed",
        "require    true\n        else continue obstructed",
    ));

    let baseline_digest = digest_core_module(&baseline).expect("baseline Core digests");
    assert_ne!(
        baseline_digest,
        digest_core_module(&changed_reason).expect("reason mutation digests")
    );
    assert_ne!(
        baseline_digest,
        digest_core_module(&changed_payload).expect("payload mutation digests")
    );
    assert_eq!(
        baseline_digest,
        digest_core_module(&reordered_payload).expect("payload ordering digests")
    );
    assert_eq!(
        baseline_digest,
        digest_core_module(&reformatted).expect("formatting variant digests")
    );
}

#[test]
fn duplicate_obstruction_reason_payload_fields_reject_before_core_digest() {
    let source = replace_required(
        CONTINUE_OBSTRUCTED_REQUIRE,
        "provided: input.id,",
        "provided: input.id,\n        provided: \"duplicate\",",
    );
    let module = parse_module(&source).expect("duplicate payload source parses");
    let errors =
        compile_to_core(&module, &pure_context()).expect_err("duplicate payload keys reject");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|err| err.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::DuplicateObstructionPayloadField]
    );
}

#[test]
fn source_type_declarations_require_core_named_identities() {
    for name in [
        "Unit",
        "Bool",
        "I8",
        "I16",
        "I32",
        "I64",
        "U8",
        "U16",
        "U32",
        "U64",
        "String",
        "Bytes",
        "Record",
        "Option",
        "List",
        "Map",
        "CapabilityRef",
        "ExternalActionRequest",
        "Nominal",
        "Variant",
    ] {
        let source = format!("package reserved.identity@1;\ntype {name} = {{ value: U64, }};\n");
        let module = parse_module(&source).expect("reserved declaration source parses");
        let edict_syntax::ast::Decl::Type(declaration) = &module.decls[0] else {
            panic!("fixture starts with a type declaration");
        };
        let failures = compile_to_core(&module, &CompilerContext::new())
            .expect_err("reserved Core identity must reject during source compilation");

        let [failure] = failures.as_slice() else {
            panic!("{name} must reject with one structured compiler failure: {failures:?}");
        };
        assert_eq!(failure.stage, CompilerStage::TypeCheck, "{name}");
        assert_eq!(
            failure.kind,
            CompilerErrorKind::ReservedTypeIdentity,
            "{name}"
        );
        assert_eq!(failure.span, declaration.span, "{name}");
    }

    let ordinary = parse_module("package reserved.identity@1;\ntype Widget = { value: U64, };\n")
        .expect("ordinary declaration source parses");
    compile_to_core(&ordinary, &CompilerContext::new())
        .expect("ordinary named declaration compiles");
}

#[test]
fn public_lower_core_rejects_an_invalid_typed_type_table() {
    let module = parse_module(BOUNDED_HELLO).expect("bounded source parses");
    let resolved = resolve_module(&module, &hello_context()).expect("bounded source resolves");
    let mut typed = type_check(&resolved).expect("bounded source type-checks");
    typed.types.insert("Bool".to_owned(), CoreType::Bool);

    let failures = lower_core(&typed).expect_err("invalid typed Core table must reject");
    let [failure] = failures.as_slice() else {
        panic!("lower_core must return one structured failure: {failures:?}");
    };
    assert_eq!(failure.stage, CompilerStage::LowerCore);
    assert_eq!(failure.kind, CompilerErrorKind::InvalidCoreTypeIntegrity);
}

#[test]
fn compiler_emits_authored_and_authenticated_named_types_without_field_scratch_entries() {
    let source = "package a.b@1;\n\
        use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as bounds;\n\
        type Nested = { leaf: U64, };\n\
        type Input = { scalar: U64, text: String<max=8>, nested: Nested, values: List<String<max=4>, max=2>, foreign: bounds.BufferId, };\n\
        type Output = { scalar: U64, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          return { scalar: input.scalar };\n\
        }";
    let lawpack = example_bounds_lawpack();
    let context = pure_context().with_type_shape(TypeShapeFact {
        lawpack,
        coordinate: "example.bounds@1.BufferId".to_owned(),
        definition: "Nominal<Bytes<exact=32>>".to_owned(),
    });
    let module = parse_module(source).expect("mixed record source parses");
    let core = compile_to_core(&module, &context).expect("mixed record source compiles");

    assert_eq!(
        core.types.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["Input", "Nested", "Output", "example.bounds@1.BufferId"]
    );
    let CoreType::Record { fields } = &core.types["Input"] else {
        panic!("Input is a named record");
    };
    assert_eq!(fields["scalar"], "U64");
    assert_eq!(fields["text"], "String<max=8,canonical=raw-utf8>");
    assert_eq!(fields["nested"], "a.b@1.Nested");
    assert_eq!(
        fields["values"],
        "List<String<max=4,canonical=raw-utf8>,max=2>"
    );
    assert_eq!(fields["foreign"], "example.bounds@1.BufferId");

    let changed_module = parse_module(&source.replace("String<max=8>", "String<max=9>"))
        .expect("field mutation source parses");
    let changed =
        compile_to_core(&changed_module, &context).expect("field mutation source compiles");
    assert_ne!(core.types["Input"], changed.types["Input"]);
    assert_ne!(
        digest_core_module(&core).expect("baseline Core digests"),
        digest_core_module(&changed).expect("changed Core digests")
    );
}

#[test]
fn continue_obstructed_reason_rejects_local_expression() {
    let source = "package a.b@1;\n\
        type Input = { id: String<max=16>, };\n\
        type Output = { id: String<max=16>, };\n\
        intent t(input: Input) returns Output\n\
          profile p.read\n\
          basis none\n\
          budget <= p.tiny {\n\
          require true else continue obstructed {\n\
            reason: input.id,\n\
            provided: input.id,\n\
          };\n\
          return { id: input.id };\n\
        }";
    let module = parse_module(source).expect("local reason source parses");
    let errors =
        compile_to_core(&module, &pure_context()).expect_err("local reason rejects before Core");

    assert!(errors
        .iter()
        .all(|err| err.stage == CompilerStage::TypeCheck));
    assert_eq!(
        errors
            .iter()
            .map(|err| err.kind)
            .collect::<Vec<CompilerErrorKind>>(),
        vec![CompilerErrorKind::UnsupportedSourceShape]
    );
}

#[test]
fn initial_core_lowering_makes_no_canonical_or_target_claim() {
    let module = parse_module(BOUNDED_HELLO).expect("fixture parses");
    let core = compile_to_core(&module, &hello_context()).expect("fixture compiles to Core");

    assert_eq!(
        core.required_core_capabilities,
        [] as [std::string::String; 0]
    );
    assert!(core
        .imports
        .iter()
        .all(|import| import.kind.as_str() != "target"));
    assert_eq!(core.api_version, "edict.core/v1");
}

fn compile_effectful_source(source: &str) -> edict_syntax::CoreModule {
    let context = effectful_context();
    let module = parse_module(source).expect("effectful source parses");
    compile_to_core(&module, &context).expect("effectful source compiles")
}

fn effectful_context() -> CompilerContext {
    let dir = temp_case_dir("effectful-context");
    let target = write_json(
        &dir,
        "target-profile-facts.json",
        effectful_target_profile_facts(),
    );
    let lawpack = write_json(&dir, "lawpack-facts.json", effectful_lawpack_facts());
    let context =
        load_compiler_context_from_authority_fact_files([target.as_path(), lawpack.as_path()])
            .expect("authority facts load");
    fs::remove_dir_all(&dir).expect("remove temporary effectful-context directory");
    context
}

fn compile_pure_source(source: &str) -> edict_syntax::CoreModule {
    let module = parse_module(source).expect("pure source parses");
    compile_to_core(&module, &pure_context()).expect("pure source compiles to Core")
}

fn only_require_node(core: &edict_syntax::CoreModule) -> &CoreNode {
    let intent = core.intents.get("t").expect("compiled intent");
    assert_eq!(intent.body.nodes.len(), 1);
    &intent.body.nodes[0]
}

fn assert_reason<'a>(
    reason: &'a CoreObstructionReason,
    kind: &str,
    payload_keys: impl IntoIterator<Item = &'a str>,
) {
    assert_eq!(reason.kind, kind);
    let mut actual = reason
        .payload
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let mut expected = payload_keys.into_iter().collect::<Vec<_>>();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

fn replace_required(source: &str, needle: &str, replacement: &str) -> String {
    assert!(
        source.contains(needle),
        "fixture mutation target was not present: {needle:?}"
    );
    let mutated = source.replace(needle, replacement);
    assert_ne!(
        mutated, source,
        "fixture mutation did not change source for target: {needle:?}"
    );
    mutated
}

fn temp_case_dir(name: &str) -> PathBuf {
    let sequence = TEMP_CASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "edict-compiler-spine-{name}-{}-{sequence}",
        std::process::id(),
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("remove stale temp compiler-spine directory");
    }
    fs::create_dir_all(&dir).expect("create temp compiler-spine directory");
    dir
}

fn write_json(dir: &Path, name: &str, contents: impl AsRef<str>) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents.as_ref()).expect("write authority-facts JSON");
    path
}

fn effectful_target_profile_facts() -> &'static str {
    r#"{
      "apiVersion": "edict.authority-facts/v1",
      "source": {
        "kind": "targetProfile",
        "coordinate": "echo.dpo@1",
        "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
      },
      "operationProfiles": [
        {
          "source": "p.effectful",
          "core": "continuum.profile.write/v1",
          "allowedWriteClasses": ["replace"]
        }
      ],
      "effectWriteClasses": [],
      "budgets": []
    }"#
}

fn effectful_lawpack_facts() -> &'static str {
    r#"{
      "apiVersion": "edict.authority-facts/v1",
      "source": {
        "kind": "lawpack",
        "coordinate": "hello.optics@1",
        "digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
      },
      "operationProfiles": [],
      "effectWriteClasses": [
        {
          "effect": "target.replace",
          "writeClass": "replace"
        }
      ],
      "budgets": [
        {
          "source": "p.tiny",
          "maxSteps": 8,
          "maxAllocatedBytes": 1024,
          "maxOutputBytes": 256
        }
      ]
    }"#
}
