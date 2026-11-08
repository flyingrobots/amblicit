//! Public source-to-projection evidence for structured operation bodies.

use super::*;
use amblicit_syntax::{CoreValue, TargetIrOrderEntry, ECHO_STRUCTURED_SPAN_IR_DOMAIN};

pub(super) const STRUCTURED_SOURCE: &str =
    include_str!("../../../../fixtures/lawpack/hello-echo/create-greetings.amblicit");
pub(super) const INTENT: &str = "createGreeting";

fn lowering() -> (CoreModule, TargetLoweringReport) {
    lower_structured(STRUCTURED_SOURCE)
}

/// Lower one source under the explicitly selected structured v3 domain.
pub(super) fn lower_structured(source: &str) -> (CoreModule, TargetLoweringReport) {
    let bundle = decode_lawpack_bundle(MANIFEST_BYTES, EXPORTS_BYTES).expect("lawpack");
    let adapter = decode_lawpack_adapter(&bundle, "echo.dpo@1", ADAPTER_BYTES).expect("adapter");
    let module = parse_module(source).expect("structured source parses");
    let preparation = prepare_lawpack_compilation(&module, &bundle, &adapter).expect("prepare");
    let core = amblicit_syntax::compile_to_core(&module, preparation.compiler_context())
        .expect("structured source compiles");
    let mut facts = preparation.target_ir_facts().clone();
    ECHO_STRUCTURED_SPAN_IR_DOMAIN.clone_into(&mut facts.target_ir_domain);
    let report = lower_to_target_ir(&core, &facts);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    (core, report)
}

pub(super) fn pair() -> (CoreModule, TargetIrArtifact) {
    let (core, report) = lowering();
    (core, report.artifact.expect("structured Target"))
}

pub(super) fn order_mut(target: &mut TargetIrArtifact) -> &mut Vec<TargetIrOrderEntry> {
    target
        .intents
        .get_mut(INTENT)
        .expect("intent")
        .execution_order
        .as_mut()
        .expect("order")
}

#[test]
fn structured_source_emits_and_independently_verifies_root_result_sources() {
    let (core, report) = lowering();
    let projection = report.result_projections.get(INTENT).unwrap_or_else(|| {
        panic!(
            "structured result projection: {:?}",
            report.result_projection_failures
        )
    });
    assert!(report.result_projection_failures.is_empty());
    let target = report.artifact.as_ref().expect("Target");
    let verified = verify_result_projection(
        &core,
        target,
        INTENT,
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("verify structured projection");
    assert_eq!(verified.projection(), projection.projection());
    let ResultProjectionExpr::Record { fields } = &verified.projection().expression else {
        panic!("record projection");
    };
    assert_eq!(
        fields["key"],
        ResultProjectionExpr::Source {
            source: ResultProjectionSource::CapabilityResult {
                step_id: "createGreeting.step.1".to_owned()
            },
            path: vec!["key".to_owned()],
        }
    );
    assert_eq!(
        fields["message"],
        ResultProjectionExpr::Source {
            source: ResultProjectionSource::PureBinding {
                binding_id: "createGreeting.binding.2".to_owned()
            },
            path: vec![],
        }
    );
    let repeated = emit_result_projection(&core, target, INTENT).expect("repeat emission");
    assert_eq!(repeated.canonical_bytes(), projection.canonical_bytes());
    assert_eq!(target.intents[INTENT].steps.len(), 2);
    assert_eq!(target.intents[INTENT].pure_bindings.len(), 3);
}

pub(super) fn refuses(
    core: &CoreModule,
    target: &TargetIrArtifact,
    projection: &ResultProjectionArtifact,
) {
    assert_eq!(
        emit_result_projection(core, target, INTENT)
            .expect_err("emit must refuse")
            .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
    assert_eq!(
        verify_result_projection(
            core,
            target,
            INTENT,
            projection.canonical_bytes(),
            projection.digest()
        )
        .expect_err("verify must refuse")
        .kind(),
        ResultProjectionFailureKind::CoreTargetMismatch
    );
}

#[test]
fn structured_projection_refuses_changed_nested_instructions_and_order() {
    let (core, target) = pair();
    let projection = emit_result_projection(&core, &target, INTENT).expect("projection");
    for mutation in 0..8 {
        let mut changed = target.clone();
        match mutation {
            0 => {
                changed
                    .intents
                    .get_mut(INTENT)
                    .expect("intent")
                    .steps
                    .remove(0);
            }
            1 => {
                let TargetIrOrderEntry::For(entry) = &mut order_mut(&mut changed)[1] else {
                    panic!("loop")
                };
                entry.bound = CoreBound::Literal(3);
            }
            2 => {
                let TargetIrOrderEntry::For(entry) = &mut order_mut(&mut changed)[1] else {
                    panic!("loop")
                };
                let TargetIrOrderEntry::Branch(branch) = &mut entry.body[1] else {
                    panic!("branch")
                };
                branch.predicate = CorePredicate::False;
            }
            3 => {
                let TargetIrOrderEntry::For(entry) = &mut order_mut(&mut changed)[1] else {
                    panic!("loop")
                };
                entry.body.swap(0, 1);
            }
            4 => {
                changed
                    .intents
                    .get_mut(INTENT)
                    .expect("intent")
                    .requirements[0]
                    .predicate = CorePredicate::True;
            }
            5 => {
                changed.intents.get_mut(INTENT).expect("intent").steps[0]
                    .obstruction_arms
                    .clear();
            }
            6 => {
                order_mut(&mut changed).swap(1, 2);
            }
            7 => {
                changed.domain = "echo.span-ir/v2".to_owned();
            }
            _ => unreachable!(),
        }
        refuses(&core, &changed, &projection);
    }
}

#[test]
fn structured_projection_does_not_export_child_binding_or_effect_results() {
    let (core, target) = pair();
    let original = emit_result_projection(&core, &target, INTENT).expect("projection");
    for source in [
        ResultProjectionSource::PureBinding {
            binding_id: "createGreeting.binding.1".to_owned(),
        },
        ResultProjectionSource::CapabilityResult {
            step_id: "createGreeting.step.0".to_owned(),
        },
    ] {
        let mut candidate = original.projection().clone();
        candidate.expression = ResultProjectionExpr::Source {
            source,
            path: vec![],
        };
        let bytes = encode_result_projection(&candidate).expect("encode hostile source");
        let digest = digest_result_projection(&candidate).expect("hash hostile source");
        let failure = verify_result_projection(&core, &target, INTENT, &bytes, digest)
            .expect_err("child sources are not root results");
        assert!(matches!(
            failure.kind(),
            ResultProjectionFailureKind::UndeclaredProjectionSource
                | ResultProjectionFailureKind::UnknownCapabilityStep
        ));
    }
    let mut leaked_core = core.clone();
    let mut leaked_target = target.clone();
    let local = leaked_target.intents[INTENT].pure_bindings[1]
        .binding
        .clone();
    leaked_core
        .intents
        .get_mut(INTENT)
        .expect("Core")
        .body
        .result = CoreExpr::Local {
        reference: local.clone(),
    };
    leaked_target
        .intents
        .get_mut(INTENT)
        .expect("Target")
        .result = CoreExpr::Local { reference: local };
    repin_target_core(&leaked_core, &mut leaked_target);
    refuses(&leaked_core, &leaked_target, &original);
}

#[test]
fn structured_projection_refuses_unrepresented_child_results_and_locals() {
    let (core, target) = pair();
    let original = emit_result_projection(&core, &target, INTENT).expect("projection");
    for mutation in 0..2 {
        let mut changed_core = core.clone();
        let mut changed_target = target.clone();
        let CoreNode::For { body, .. } = &mut changed_core
            .intents
            .get_mut(INTENT)
            .expect("Core")
            .body
            .nodes[1]
        else {
            panic!("loop")
        };
        if mutation == 0 {
            body.result = CoreExpr::Const(CoreValue::Bool(true));
        } else {
            body.locals.clear();
        }
        repin_target_core(&changed_core, &mut changed_target);
        refuses(&changed_core, &changed_target, &original);
    }
}
