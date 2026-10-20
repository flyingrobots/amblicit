//! Canonical ordering integrity for the explicitly selected v2 artifact.

use std::collections::BTreeMap;

use super::{
    TargetIrArtifact, TargetIrIntent, TargetIrPureBinding, TargetIrRequireFailure,
    TargetIrRequirement, TargetIrStep, ECHO_DPO_TARGET_PROFILE, ECHO_ORDERED_SPAN_IR_DOMAIN,
};
use crate::core_ir::{CoreExpr, CorePredicate, LocalRef, CORE_APPLICATION_INPUT_LOCAL_ID};
use crate::MAX_CANONICAL_NESTING_DEPTH;

pub(crate) fn artifact_has_valid_execution_order(artifact: &TargetIrArtifact) -> bool {
    if artifact.domain != ECHO_ORDERED_SPAN_IR_DOMAIN {
        return artifact
            .intents
            .values()
            .all(|intent| intent.execution_order.is_none());
    }
    artifact.target_profile.coordinate == ECHO_DPO_TARGET_PROFILE
        && artifact.intents.values().all(intent_has_valid_order)
}

#[derive(Clone, Copy)]
enum Instruction<'a> {
    Binding(&'a TargetIrPureBinding),
    Step(&'a TargetIrStep),
    Require(&'a TargetIrRequirement),
}

fn intent_has_valid_order(intent: &TargetIrIntent) -> bool {
    let Some(order) = &intent.execution_order else {
        return false;
    };
    if !intent.external_action_requests.is_empty() {
        return false;
    }
    let mut instructions = intent
        .pure_bindings
        .iter()
        .map(|node| (node.id.as_str(), Instruction::Binding(node)))
        .chain(
            intent
                .steps
                .iter()
                .map(|node| (node.id.as_str(), Instruction::Step(node))),
        )
        .chain(
            intent
                .requirements
                .iter()
                .map(|node| (node.id.as_str(), Instruction::Require(node))),
        )
        .collect::<BTreeMap<_, _>>();
    let count = intent.pure_bindings.len() + intent.steps.len() + intent.requirements.len();
    if instructions.len() != count || order.len() != count || instructions.contains_key("") {
        return false;
    }
    let mut available = LocalAvailability::default();
    if !intent
        .basis
        .as_ref()
        .is_none_or(|basis| available.expression(basis, 0))
        || !intent
            .input_constraints
            .iter()
            .all(|constraint| available.predicate(&constraint.predicate, 0))
    {
        return false;
    }
    for id in order {
        let Some(instruction) = instructions.remove(id.as_str()) else {
            return false;
        };
        if !available.instruction(instruction) {
            return false;
        }
    }
    instructions.is_empty() && available.expression(&intent.result, 0)
}

#[derive(Default)]
struct LocalAvailability<'a> {
    input: Option<&'a LocalRef>,
    producers: BTreeMap<&'a str, &'a LocalRef>,
}

impl<'a> LocalAvailability<'a> {
    fn introduce(&mut self, binding: &'a LocalRef) -> bool {
        !binding.id.is_empty()
            && binding.id != CORE_APPLICATION_INPUT_LOCAL_ID
            && self.producers.insert(&binding.id, binding).is_none()
    }

    fn reference(&mut self, reference: &'a LocalRef) -> bool {
        if reference.id == CORE_APPLICATION_INPUT_LOCAL_ID {
            // The artifact has no separate input declaration. Core validation
            // proves its type; the encoder additionally requires consistency.
            *self.input.get_or_insert(reference) == reference
        } else {
            self.producers.get(reference.id.as_str()).copied() == Some(reference)
        }
    }

    fn instruction(&mut self, instruction: Instruction<'a>) -> bool {
        match instruction {
            Instruction::Binding(node) => {
                self.expression(&node.value, 0) && self.introduce(&node.binding)
            }
            Instruction::Step(node) => {
                if !self.expression(&node.input, 0) {
                    return false;
                }
                for arm in node.obstruction_arms.values() {
                    if !self.introduce(&arm.binder) {
                        return false;
                    }
                    let valid = self.expression(&arm.value, 0);
                    self.producers.remove(arm.binder.id.as_str());
                    if !valid {
                        return false;
                    }
                }
                self.introduce(&node.binding)
            }
            Instruction::Require(node) => {
                let (TargetIrRequireFailure::Terminal { reason }
                | TargetIrRequireFailure::ContinueObstructed { reason }) = &node.on_failure;
                self.predicate(&node.predicate, 0)
                    && reason
                        .payload
                        .values()
                        .all(|value| self.expression(value, 0))
            }
        }
    }

    fn expression(&mut self, expression: &'a CoreExpr, depth: usize) -> bool {
        if depth > MAX_CANONICAL_NESTING_DEPTH {
            return false;
        }
        match expression {
            CoreExpr::Local { reference } => self.reference(reference),
            CoreExpr::Const(_) => true,
            CoreExpr::Record { fields } => fields
                .values()
                .all(|value| self.expression(value, depth + 1)),
            CoreExpr::Field { base, .. } => self.expression(base, depth + 1),
            CoreExpr::Call { args, .. } => args.iter().all(|arg| self.expression(arg, depth + 1)),
            CoreExpr::If {
                predicate,
                then_value,
                else_value,
            } => {
                self.predicate(predicate, depth + 1)
                    && self.expression(then_value, depth + 1)
                    && self.expression(else_value, depth + 1)
            }
        }
    }

    fn predicate(&mut self, predicate: &'a CorePredicate, depth: usize) -> bool {
        if depth > MAX_CANONICAL_NESTING_DEPTH {
            return false;
        }
        match predicate {
            CorePredicate::True | CorePredicate::False => true,
            CorePredicate::Not(inner) => self.predicate(inner, depth + 1),
            CorePredicate::All(predicates) | CorePredicate::Any(predicates) => predicates
                .iter()
                .all(|predicate| self.predicate(predicate, depth + 1)),
            CorePredicate::Compare { left, right, .. } => {
                self.expression(left, depth + 1) && self.expression(right, depth + 1)
            }
        }
    }
}
