//! Hostile structured pairs that the order judgment admits, so that only one
//! correspondence comparison stands between them and an accepted projection.
//!
//! Each case changes one compared property, keeps a control that the tamper
//! happened, and must be refused by emission and verification with the
//! structured correspondence subject.

use super::structured::{lower_structured, order_mut, pair, refuses, INTENT, STRUCTURED_SOURCE};
use super::*;
use amblicit_syntax::{CoreValue, TargetIrIntent, TargetIrOrderEntry, TargetIrStep};

const ROOT_EFFECT: usize = 2;
const NESTED_LOOP: usize = 1;
const NESTED_BRANCH: usize = 1;
const NESTED_STEP: usize = 0;
const NESTED_BINDING: usize = 1;
const ROOT_MESSAGE_BINDING: usize = 0;
const RETURNED_BINDING: usize = 2;
const MESSAGE_LET: &str = "  let message = input.item.message;\n";
const RETURNED_LET: &str = "  let returned = message;\n";

/// Replace a fixture line that occurs exactly once, so a changed fixture
/// cannot silently drop the tamper.
fn source_with(anchor: &str, replacement: &str) -> String {
    assert_eq!(
        STRUCTURED_SOURCE.matches(anchor).count(),
        1,
        "anchor {anchor:?}"
    );
    let source = STRUCTURED_SOURCE.replace(anchor, replacement);
    assert_ne!(source, STRUCTURED_SOURCE);
    source
}

fn lowered(source: &str) -> (CoreModule, TargetIrArtifact) {
    let (core, report) = lower_structured(source);
    (core, report.artifact.expect("structured Target"))
}

fn intent_mut(target: &mut TargetIrArtifact) -> &mut TargetIrIntent {
    target.intents.get_mut(INTENT).expect("intent")
}

fn nested_step_mut(target: &mut TargetIrArtifact) -> &mut TargetIrStep {
    &mut intent_mut(target).steps[NESTED_STEP]
}

fn swap_root_instructions(target: &mut TargetIrArtifact, first: &str, second: &str) {
    let order = order_mut(target);
    let position = |id: &str| {
        order
            .iter()
            .position(
                |entry| matches!(entry, TargetIrOrderEntry::Instruction(found) if found == id),
            )
            .expect("instruction in the root order")
    };
    let (first, second) = (position(first), position(second));
    order.swap(first, second);
}

fn core_nodes_mut(core: &mut CoreModule) -> &mut Vec<CoreNode> {
    &mut core
        .intents
        .get_mut(INTENT)
        .expect("Core intent")
        .body
        .nodes
}

fn core_loop_mut(core: &mut CoreModule) -> (&LocalRef, &mut CoreBlock) {
    match &mut core_nodes_mut(core)[NESTED_LOOP] {
        CoreNode::For { binder, body, .. } => Some((&*binder, body)),
        _ => None,
    }
    .expect("fixture root node is the bounded loop")
}

fn core_branch_binding_mut(body: &mut CoreBlock) -> &mut Option<LocalRef> {
    match &mut body.nodes[NESTED_BRANCH] {
        CoreNode::Branch { binding, .. } => Some(binding),
        _ => None,
    }
    .expect("fixture loop body holds the statement branch")
}

/// Refuse at emission and verification as a structured correspondence failure.
fn refuses_correspondence(
    core: &CoreModule,
    target: &TargetIrArtifact,
    projection: &ResultProjectionArtifact,
) {
    refuses(core, target, projection);
    let subject = format!("{INTENT}.structuredControl");
    let emitted = emit_result_projection(core, target, INTENT).expect_err("emit must refuse");
    assert_eq!(emitted.subject(), subject);
    let verified = verify_result_projection(
        core,
        target,
        INTENT,
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect_err("verify must refuse");
    assert_eq!(verified.subject(), subject);
}

/// Refuse a Target-only tamper of an honestly lowered pair.
fn refuses_target_change(core: &CoreModule, target: &TargetIrArtifact, changed: &TargetIrArtifact) {
    let projection = emit_result_projection(core, target, INTENT).expect("baseline projection");
    assert_ne!(changed, target);
    refuses_correspondence(core, changed, &projection);
}

/// Refuse a Core-side tamper whose Target is repinned to the changed Core.
fn refuses_core_change(mutate: impl FnOnce(&mut CoreModule)) {
    let (core, target) = pair();
    let projection = emit_result_projection(&core, &target, INTENT).expect("baseline projection");
    let mut changed_core = core.clone();
    mutate(&mut changed_core);
    assert_ne!(changed_core, core);
    let mut changed_target = target.clone();
    repin_target_core(&changed_core, &mut changed_target);
    refuses_correspondence(&changed_core, &changed_target, &projection);
}

#[test]
fn structured_projection_refuses_reordered_same_scope_root_effects() {
    let (core, target) = lowered(&source_with(
        RETURNED_LET,
        "  let second: hello.GreetingReceipt = hello.createGreeting(input.item)\n    else { alreadyExists(again) => hello.AlreadyExists };\n  let returned = message;\n",
    ));
    let mut changed = target.clone();
    swap_root_instructions(
        &mut changed,
        "createGreeting.step.1",
        "createGreeting.step.2",
    );
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_reordered_same_scope_root_requirements() {
    let (core, target) = lowered(&source_with(
        RETURNED_LET,
        "  require input.item.message != \"a\" else hello.AlreadyExists;\n  require input.item.message != \"b\" else hello.AlreadyExists;\n  let returned = message;\n",
    ));
    let mut changed = target.clone();
    swap_root_instructions(
        &mut changed,
        "createGreeting.require.1",
        "createGreeting.require.2",
    );
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_reordered_same_scope_root_bindings() {
    let (core, target) = lowered(&source_with(
        MESSAGE_LET,
        "  let message = input.item.message;\n  let other = input.item.message;\n",
    ));
    let mut changed = target.clone();
    swap_root_instructions(
        &mut changed,
        "createGreeting.binding.0",
        "createGreeting.binding.1",
    );
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_a_redirected_root_result_binding() {
    let (core, target) = pair();
    let mut changed = target.clone();
    let bindings = &mut intent_mut(&mut changed).pure_bindings;
    bindings[RETURNED_BINDING].value = bindings[ROOT_MESSAGE_BINDING].value.clone();
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_a_changed_nested_binding_payload() {
    let (core, target) = pair();
    let mut changed = target.clone();
    let input = core.intents[INTENT]
        .body
        .locals
        .iter()
        .find(|local| local.ty == core.intents[INTENT].input)
        .cloned()
        .expect("application input local");
    intent_mut(&mut changed).pure_bindings[NESTED_BINDING].value = CoreExpr::Field {
        base: Box::new(CoreExpr::Local { reference: input }),
        field: "item".to_owned(),
    };
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_a_changed_obstruction_arm_value() {
    let (core, target) = pair();
    let mut changed = target.clone();
    let arms = &mut nested_step_mut(&mut changed).obstruction_arms;
    assert!(!arms.is_empty(), "fixture step has an obstruction arm");
    for arm in arms.values_mut() {
        arm.value = CoreExpr::Const(CoreValue::Null);
    }
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_a_forged_obstruction_failure_key() {
    let (core, target) = pair();
    let mut changed = target.clone();
    nested_step_mut(&mut changed)
        .obstruction_failures
        .push("zz.forged".to_owned());
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_a_target_arm_binder_used_twice() {
    let (core, target) = pair();
    let mut changed = target.clone();
    let step = nested_step_mut(&mut changed);
    let arm = step
        .obstruction_arms
        .values()
        .next()
        .cloned()
        .expect("fixture step has an obstruction arm");
    step.obstruction_arms.insert("zz.forged".to_owned(), arm);
    step.obstruction_failures = step.obstruction_arms.keys().cloned().collect();
    refuses_target_change(&core, &target, &changed);
}

#[test]
fn structured_projection_refuses_a_core_arm_binder_used_twice() {
    refuses_core_change(|core| {
        let obstruction_map = match &mut core_nodes_mut(core)[ROOT_EFFECT] {
            CoreNode::Effect {
                obstruction_map, ..
            } => Some(obstruction_map),
            _ => None,
        }
        .expect("fixture root node is the root effect");
        let arm = obstruction_map
            .values()
            .next()
            .cloned()
            .expect("fixture effect has an obstruction arm");
        obstruction_map.insert("zz.forged".to_owned(), arm);
    });
}

#[test]
fn structured_projection_refuses_a_duplicated_child_local_declaration() {
    refuses_core_change(|core| {
        let (_, body) = core_loop_mut(core);
        let duplicate = body.locals.first().cloned().expect("loop body local");
        body.locals.push(duplicate);
    });
}

#[test]
fn structured_projection_refuses_a_changed_loop_binder_declaration() {
    refuses_core_change(|core| {
        let (binder, body) = core_loop_mut(core);
        let id = binder.id.clone();
        let local = body
            .locals
            .iter_mut()
            .find(|local| local.id == id)
            .expect("loop binder declared in its body");
        "$forgedAlpha".clone_into(&mut local.alpha_name);
    });
}

#[test]
fn structured_projection_refuses_an_undeclared_child_binding() {
    refuses_core_change(|core| {
        let (binder, body) = core_loop_mut(core);
        let id = binder.id.clone();
        let before = body.locals.len();
        body.locals.retain(|local| local.id == id);
        assert!(body.locals.len() < before, "a non-binder local was dropped");
    });
}

#[test]
fn structured_projection_refuses_a_statement_branch_given_a_value_binding() {
    refuses_core_change(|core| {
        let (_, body) = core_loop_mut(core);
        let local = LocalRef {
            id: "local.98".to_owned(),
            alpha_name: "$local98".to_owned(),
            ty: "Unit".to_owned(),
        };
        body.locals.push(local.clone());
        *core_branch_binding_mut(body) = Some(local);
    });
}

#[test]
fn structured_projection_accepts_a_canonically_equal_failure_list() {
    let (core, target) = pair();
    let honest = emit_result_projection(&core, &target, INTENT).expect("honest emission");
    let mut changed = target.clone();
    let step = nested_step_mut(&mut changed);
    let label = step
        .obstruction_failures
        .first()
        .cloned()
        .expect("fixture step has an obstruction failure");
    step.obstruction_failures.push(label);
    assert_ne!(changed, target, "the failure list must actually change");
    assert_eq!(
        amblicit_syntax::encode_target_ir_artifact(&changed).expect("encode changed"),
        amblicit_syntax::encode_target_ir_artifact(&target).expect("encode honest"),
        "canonical encoding treats the failure list as a set",
    );
    let emitted = emit_result_projection(&core, &changed, INTENT)
        .expect("a canonically equal Target must emit");
    assert_eq!(emitted.canonical_bytes(), honest.canonical_bytes());
    verify_result_projection(
        &core,
        &changed,
        INTENT,
        honest.canonical_bytes(),
        honest.digest(),
    )
    .expect("a canonically equal Target must verify");
}
