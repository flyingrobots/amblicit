//! Small depth-boundary witnesses; no stack-exhaustion or stress workload.
use std::collections::BTreeMap;

use edict_syntax::{
    encode_core_module, lower_to_target_ir, validate_core_module_type_integrity,
    CanonicalErrorKind, CompareOp, CoreBlock, CoreBudget, CoreExpr, CoreIntent, CoreModule,
    CoreNode, CorePredicate, CoreTypeIntegrityFailureKind, CoreValue, InputConstraint,
    InputConstraintSource, ResourceRef, TargetIrLoweringFacts, TargetLoweringFailureKind,
    TargetLoweringStatus,
};

const LIMIT: usize = 128;

fn leaf() -> CoreExpr {
    CoreExpr::Const(CoreValue::Bool(true))
}

fn field_chain(depth: usize) -> CoreExpr {
    (0..depth).fold(leaf(), |base, _| CoreExpr::Field {
        base: Box::new(base),
        field: "value".into(),
    })
}

fn predicate_chain(depth: usize) -> CorePredicate {
    (0..depth).fold(CorePredicate::True, |inner, _| {
        CorePredicate::Not(Box::new(inner))
    })
}

fn mixed_chain(pairs: usize) -> CoreExpr {
    (0..pairs).fold(leaf(), |inner, _| CoreExpr::If {
        predicate: Box::new(CorePredicate::Compare {
            op: CompareOp::Eq,
            left: inner,
            right: leaf(),
        }),
        then_value: Box::new(leaf()),
        else_value: Box::new(leaf()),
    })
}

fn block() -> CoreBlock {
    CoreBlock {
        locals: Vec::new(),
        nodes: Vec::new(),
        result: leaf(),
    }
}

fn block_chain(depth: usize, result: CoreExpr) -> CoreBlock {
    (0..depth).fold(CoreBlock { result, ..block() }, |inner, _| CoreBlock {
        nodes: vec![CoreNode::Branch {
            binding: None,
            predicate: CorePredicate::True,
            then_block: inner,
            else_block: block(),
        }],
        ..block()
    })
}

fn module() -> CoreModule {
    CoreModule {
        functions: BTreeMap::new(),
        api_version: "edict.core/v1".into(),
        coordinate: "depth.example@1".into(),
        imports: Vec::new(),
        types: BTreeMap::new(),
        intents: BTreeMap::from([(
            "check".into(),
            CoreIntent {
                input: "Bool".into(),
                output: "Bool".into(),
                required_operation_profile: "continuum.profile.read-only/v1".into(),
                basis: None,
                input_constraints: Vec::new(),
                core_evaluation_budget: CoreBudget {
                    max_steps: 1024,
                    max_allocated_bytes: 4096,
                    max_output_bytes: 1024,
                },
                body: block(),
            },
        )]),
        required_core_capabilities: Vec::new(),
    }
}

fn assert_depth_rejected(core: &CoreModule, expected_path: &str) {
    let Err(failure) = validate_core_module_type_integrity(core) else {
        panic!("over-limit graph must fail integrity validation");
    };
    assert_eq!(failure.kind(), CoreTypeIntegrityFailureKind::DepthExceeded);
    assert_eq!(failure.path(), expected_path);
    assert_eq!(
        encode_core_module(core).unwrap_err().kind(),
        CanonicalErrorKind::UnsupportedValue
    );
    let report = lower_to_target_ir(
        core,
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
    assert_eq!(report.status, TargetLoweringStatus::Unsupported);
    assert!(report.artifact.is_none());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(
        report.failures[0].kind,
        TargetLoweringFailureKind::InvalidCoreIdentity
    );
    assert_eq!(report.failures[0].detail, expected_path);
}

#[test]
fn core_graph_expression_depth_has_an_exact_boundary() {
    let mut core = module();
    core.intents.get_mut("check").unwrap().basis = Some(field_chain(LIMIT));
    validate_core_module_type_integrity(&core).unwrap();
    core.intents.get_mut("check").unwrap().basis = Some(field_chain(LIMIT + 1));
    assert_depth_rejected(
        &core,
        &format!("intents.check.basis{}", ".field.base".repeat(LIMIT + 1)),
    );
}

#[test]
fn core_graph_predicate_depth_has_an_exact_boundary() {
    let mut core = module();
    core.intents
        .get_mut("check")
        .unwrap()
        .input_constraints
        .push(InputConstraint {
            coordinate: "depth.example@1.check.where.0".into(),
            source: InputConstraintSource::Where,
            predicate: predicate_chain(LIMIT),
        });
    validate_core_module_type_integrity(&core).unwrap();
    core.intents.get_mut("check").unwrap().input_constraints[0].predicate =
        predicate_chain(LIMIT + 1);
    assert_depth_rejected(
        &core,
        &format!(
            "intents.check.inputConstraints[0].predicate{}",
            ".not".repeat(LIMIT + 1)
        ),
    );
}

#[test]
fn core_graph_expression_predicate_transitions_share_one_depth_budget() {
    let mut core = module();
    core.intents.get_mut("check").unwrap().body.result = mixed_chain(LIMIT / 2);
    validate_core_module_type_integrity(&core).unwrap();
    core.intents.get_mut("check").unwrap().body.result = CoreExpr::Field {
        base: Box::new(mixed_chain(LIMIT / 2)),
        field: "value".into(),
    };
    assert_depth_rejected(
        &core,
        &format!(
            "intents.check.body.result.field.base{}",
            ".if.predicate.compare.left".repeat(LIMIT / 2)
        ),
    );
}

#[test]
fn core_graph_nested_blocks_share_the_expression_depth_budget() {
    for (blocks, expressions) in [(LIMIT, 0), (LIMIT / 2, LIMIT / 2)] {
        let mut core = module();
        core.intents.get_mut("check").unwrap().body = block_chain(blocks, field_chain(expressions));
        validate_core_module_type_integrity(&core).unwrap();
        let expected = if expressions == 0 {
            core.intents.get_mut("check").unwrap().body = block_chain(blocks + 1, leaf());
            format!(
                "intents.check.body{}",
                ".nodes[0].branch.then".repeat(blocks + 1)
            )
        } else {
            core.intents.get_mut("check").unwrap().body =
                block_chain(blocks, field_chain(expressions + 1));
            format!(
                "intents.check.body{}.result{}",
                ".nodes[0].branch.then".repeat(blocks),
                ".field.base".repeat(expressions + 1)
            )
        };
        assert_depth_rejected(&core, &expected);
    }
}
