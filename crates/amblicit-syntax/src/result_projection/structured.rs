//! Match the structured body before selecting the sources visible at its root.
//! This checks correspondence, not adapter or runtime execution authority.

use super::{
    failure, validate_pure_binding_matches_core, ProjectionById, ProjectionByLocal,
    ProjectionSources, ResultProjectionFailure, ResultProjectionFailureKind,
};
use crate::{
    CoreBlock, CoreExpr, CoreIntent, CoreNode, CoreRequireFailureArm, CoreValue, LocalRef,
    TargetIrIntent, TargetIrOrderEntry, TargetIrRequireFailure, ECHO_STRUCTURED_SPAN_IR_DOMAIN,
    MAX_CANONICAL_NESTING_DEPTH,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn resolve_sources(
    core: &CoreIntent,
    target: &TargetIrIntent,
    intent_name: &str,
    input: &LocalRef,
) -> Result<ProjectionSources, ResultProjectionFailure> {
    let invalid = || {
        failure(
            ResultProjectionFailureKind::CoreTargetMismatch,
            format!("{intent_name}.structuredControl"),
        )
    };
    // This existing independent order judgment enforces unique identities,
    // table coverage, literal bounds and lexical availability in every arm.
    if !crate::target_ir::execution_order::intent_has_valid_execution_order(
        ECHO_STRUCTURED_SPAN_IR_DOMAIN,
        target,
    ) {
        return Err(invalid());
    }
    let order = target.execution_order.as_deref().ok_or_else(invalid)?;
    let mut matcher = BodyMatch {
        target,
        intent_name,
        counts: Counts::default(),
    };
    if !matcher.block(&core.body, order, Some(input), 0)
        || matcher.counts.bindings != target.pure_bindings.len()
        || matcher.counts.steps != target.steps.len()
        || matcher.counts.requirements != target.requirements.len()
    {
        return Err(invalid());
    }
    let root_locals = core
        .body
        .nodes
        .iter()
        .filter_map(|node| match node {
            CoreNode::Let { binding, .. } | CoreNode::Effect { binding, .. } => {
                Some(binding.id.as_str())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let (pure_by_local, pure_by_id) = root_sources(
        &root_locals,
        target
            .pure_bindings
            .iter()
            .map(|binding| (&binding.id, &binding.binding)),
    );
    let (capability_by_local, capability_by_step) = root_sources(
        &root_locals,
        target.steps.iter().map(|step| (&step.id, &step.binding)),
    );
    Ok((
        pure_by_local,
        pure_by_id,
        capability_by_local,
        capability_by_step,
    ))
}

fn root_sources<'a>(
    root_locals: &BTreeSet<&str>,
    instructions: impl Iterator<Item = (&'a String, &'a LocalRef)>,
) -> (ProjectionByLocal, ProjectionById) {
    let mut by_local = BTreeMap::new();
    let mut by_id = BTreeMap::new();
    for (id, binding) in instructions {
        if root_locals.contains(binding.id.as_str()) {
            by_local.insert(binding.id.clone(), (id.clone(), binding.clone()));
            by_id.insert(id.clone(), binding.clone());
        }
    }
    (by_local, by_id)
}

#[derive(Default)]
struct Counts {
    bindings: usize,
    steps: usize,
    requirements: usize,
    loops: usize,
    branches: usize,
}

struct BodyMatch<'a> {
    target: &'a TargetIrIntent,
    intent_name: &'a str,
    counts: Counts,
}

impl BodyMatch<'_> {
    fn block(
        &mut self,
        core: &CoreBlock,
        order: &[TargetIrOrderEntry],
        binder: Option<&LocalRef>,
        depth: usize,
    ) -> bool {
        if depth > MAX_CANONICAL_NESTING_DEPTH
            || core.nodes.len() != order.len()
            || (depth > 0 && core.result != CoreExpr::Const(CoreValue::Null))
        {
            return false;
        }
        let locals = core
            .locals
            .iter()
            .map(|local| (local.id.as_str(), local))
            .collect::<BTreeMap<_, _>>();
        if locals.len() != core.locals.len()
            || binder
                .is_some_and(|binding| locals.get(binding.id.as_str()).copied() != Some(binding))
        {
            return false;
        }
        for (node, entry) in core.nodes.iter().zip(order) {
            if let CoreNode::Let { binding, .. } | CoreNode::Effect { binding, .. } = node {
                if locals.get(binding.id.as_str()).copied() != Some(binding) {
                    return false;
                }
            }
            if !self.node(node, entry, depth) {
                return false;
            }
        }
        true
    }

    fn node(&mut self, core: &CoreNode, entry: &TargetIrOrderEntry, depth: usize) -> bool {
        match (core, entry) {
            (CoreNode::Let { binding, value }, TargetIrOrderEntry::Instruction(id)) => {
                let index = self.counts.bindings;
                self.counts.bindings += 1;
                self.target.pure_bindings.get(index).is_some_and(|target| {
                    &target.id == id
                        && validate_pure_binding_matches_core(
                            target,
                            binding,
                            value,
                            self.intent_name,
                            index,
                        )
                        .is_ok()
                })
            }
            (
                CoreNode::Effect {
                    binding,
                    effect,
                    input,
                    obstruction_map,
                },
                TargetIrOrderEntry::Instruction(id),
            ) => {
                let index = self.counts.steps;
                self.counts.steps += 1;
                self.target.steps.get(index).is_some_and(|target| {
                    &target.id == id && target.id == format!("{}.step.{index}", self.intent_name)
                        && &target.binding == binding && &target.effect == effect && &target.input == input
                        // Adapters can rename failure keys. Their mapping authority is
                        // checked by the provider; the Core arm values must survive.
                        && arms_match(obstruction_map, &target.obstruction_arms)
                        && target.obstruction_failures.iter().eq(target.obstruction_arms.keys())
                })
            }
            (CoreNode::Require { predicate, arm }, TargetIrOrderEntry::Instruction(id)) => {
                let index = self.counts.requirements;
                self.counts.requirements += 1;
                self.target.requirements.get(index).is_some_and(|target| {
                    &target.id == id
                        && target.id == format!("{}.require.{index}", self.intent_name)
                        && &target.predicate == predicate
                        && match (arm, &target.on_failure) {
                            (
                                CoreRequireFailureArm::Terminal { reason: core },
                                TargetIrRequireFailure::Terminal { reason: target },
                            )
                            | (
                                CoreRequireFailureArm::ContinueObstructed { reason: core },
                                TargetIrRequireFailure::ContinueObstructed { reason: target },
                            ) => core == target,
                            _ => false,
                        }
                })
            }
            (
                CoreNode::For {
                    binder,
                    iter,
                    bound,
                    body,
                },
                TargetIrOrderEntry::For(target),
            ) => {
                let index = self.counts.loops;
                self.counts.loops += 1;
                target.id == format!("{}.for.{index}", self.intent_name)
                    && binder == &target.binder
                    && iter == &target.iterable
                    && bound == &target.bound
                    && self.block(body, &target.body, Some(binder), depth + 1)
            }
            (
                CoreNode::Branch {
                    binding: None,
                    predicate,
                    then_block,
                    else_block,
                },
                TargetIrOrderEntry::Branch(target),
            ) => {
                let index = self.counts.branches;
                self.counts.branches += 1;
                target.id == format!("{}.branch.{index}", self.intent_name)
                    && predicate == &target.predicate
                    && self.block(then_block, &target.then_entries, None, depth + 1)
                    && self.block(else_block, &target.else_entries, None, depth + 1)
            }
            _ => false,
        }
    }
}

fn arms_match(
    core: &BTreeMap<String, crate::CoreObstructionArm>,
    target: &BTreeMap<String, crate::CoreObstructionArm>,
) -> bool {
    let core_by_binder = core
        .values()
        .map(|arm| (arm.binder.id.as_str(), arm))
        .collect::<BTreeMap<_, _>>();
    let target_by_binder = target
        .values()
        .map(|arm| (arm.binder.id.as_str(), arm))
        .collect::<BTreeMap<_, _>>();
    core_by_binder.len() == core.len()
        && target_by_binder.len() == target.len()
        && core_by_binder == target_by_binder
}
