//! Pinned target pure intrinsics at the untrusted Core boundary.
//!
//! Target recomputes every intrinsic call's cost from operand bounds it
//! establishes itself (the operand's validated type, or a literal's actual
//! size), never from a claim carried by the artifact, and refuses an intent
//! whose intrinsic work exceeds its step budget or cannot be established.
use super::{
    expression_type_coordinate, resolved_core_type, BTreeMap, CoreExpr, CoreIntent, CoreModule,
    CoreNode, CorePredicate, CoreRequireFailureArm, CoreType, CoreValue, LocalRef,
    TargetIrLoweringFacts, TargetLoweringFailure, TargetLoweringFailureKind,
    TargetPureFunctionFact,
};
use crate::core_ir::{CoreBlock, MAX_CORE_GRAPH_DEPTH};

/// An admitted intrinsic must belong to exactly the lowering target profile.
pub(super) fn foreign_target(facts: &TargetIrLoweringFacts) -> Option<TargetLoweringFailure> {
    facts
        .pure_functions
        .iter()
        .find(|fact| fact.intrinsic_cost.is_some() && fact.lawpack != facts.target_profile)
        .map(|fact| TargetLoweringFailure {
            kind: TargetLoweringFailureKind::InvalidCoreIdentity,
            intent: None,
            node_index: None,
            detail: format!(
                "target pure intrinsic {} is pinned to another target profile",
                fact.coordinate
            ),
        })
}

/// Whether every intrinsic call reachable from the intent has an established
/// cost whose sum fits the intent's step budget.
pub(super) fn intent_cost_fits(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    intent: &CoreIntent,
) -> bool {
    if !pure_functions
        .iter()
        .any(|fact| fact.intrinsic_cost.is_some())
    {
        return true;
    }
    let mut meter = Meter {
        core,
        pure_functions,
        functions: BTreeMap::new(),
        visiting: Vec::new(),
        intrinsic_calls: 0,
    };
    let available = locals(intent.body.locals.iter());
    let constraints = intent
        .input_constraints
        .iter()
        .try_fold(0_u64, |steps, constraint| {
            steps.checked_add(meter.predicate(&constraint.predicate, &available, 0)?)
        });
    let basis = intent
        .basis
        .as_ref()
        .map_or(Some(0), |basis| meter.expression(basis, &available, 0));
    constraints
        .zip(basis)
        .and_then(|(constraints, basis)| constraints.checked_add(basis))
        .and_then(|steps| steps.checked_add(meter.block(&intent.body, &available, 0)?))
        .is_some_and(|steps| steps <= intent.core_evaluation_budget.max_steps)
}

fn locals<'a>(locals: impl Iterator<Item = &'a LocalRef>) -> BTreeMap<&'a str, &'a LocalRef> {
    locals.map(|local| (local.id.as_str(), local)).collect()
}

struct Meter<'a> {
    core: &'a CoreModule,
    pure_functions: &'a [TargetPureFunctionFact],
    /// Per helper: its metered steps and whether it reaches any intrinsic call.
    functions: BTreeMap<&'a str, (u64, bool)>,
    visiting: Vec<&'a str>,
    /// Intrinsic calls metered so far, counted whatever their cost, so a
    /// zero-cost call is still visible to the loop-body refusal.
    intrinsic_calls: u64,
}

impl<'a> Meter<'a> {
    fn block(
        &mut self,
        block: &'a CoreBlock,
        available: &BTreeMap<&str, &LocalRef>,
        depth: usize,
    ) -> Option<u64> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        // A nested block's own bindings (branch yields, loop bodies) are in
        // scope for its nodes but are not in the intent's local table.
        let mut scope = available.clone();
        scope.extend(locals(block.locals.iter()));
        let available = &scope;
        let mut steps = 0_u64;
        for node in &block.nodes {
            let node_steps = match node {
                CoreNode::Let { value, .. } => self.expression(value, available, depth + 1)?,
                CoreNode::Require { predicate, arm } => {
                    let (CoreRequireFailureArm::Terminal { reason }
                    | CoreRequireFailureArm::ContinueObstructed { reason }) = arm;
                    let mut steps = self.predicate(predicate, available, depth + 1)?;
                    for value in reason.payload.values() {
                        steps =
                            steps.checked_add(self.expression(value, available, depth + 1)?)?;
                    }
                    steps
                }
                CoreNode::Effect {
                    input,
                    obstruction_map,
                    ..
                } => {
                    let mut steps = self.expression(input, available, depth + 1)?;
                    for arm in obstruction_map.values() {
                        steps = steps.checked_add(self.expression(
                            &arm.value,
                            available,
                            depth + 1,
                        )?)?;
                    }
                    steps
                }
                CoreNode::ExternalActionRequest {
                    input,
                    authority_scope,
                    basis,
                    budget,
                    ..
                } => [
                    input,
                    authority_scope,
                    basis,
                    &budget.max_settlement_bytes,
                    &budget.max_attempts,
                ]
                .into_iter()
                .try_fold(0_u64, |steps, value| {
                    steps.checked_add(self.expression(value, available, depth + 1)?)
                })?,
                // Loop bodies are charged once per iteration bound elsewhere;
                // an intrinsic inside one is refused rather than undercharged,
                // judged by presence (including through helpers), not cost.
                CoreNode::For { iter, body, .. } => {
                    let iter = self.expression(iter, available, depth + 1)?;
                    let before = self.intrinsic_calls;
                    if self.block(body, available, depth + 1)? != 0
                        || self.intrinsic_calls != before
                    {
                        return None;
                    }
                    iter
                }
                CoreNode::Branch {
                    predicate,
                    then_block,
                    else_block,
                    ..
                } => self
                    .predicate(predicate, available, depth + 1)?
                    .checked_add(
                        self.block(then_block, available, depth + 1)?
                            .max(self.block(else_block, available, depth + 1)?),
                    )?,
            };
            steps = steps.checked_add(node_steps)?;
        }
        steps.checked_add(self.expression(&block.result, available, depth + 1)?)
    }

    fn predicate(
        &mut self,
        predicate: &'a CorePredicate,
        available: &BTreeMap<&str, &LocalRef>,
        depth: usize,
    ) -> Option<u64> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        match predicate {
            CorePredicate::True | CorePredicate::False => Some(0),
            CorePredicate::Not(inner) => self.predicate(inner, available, depth + 1),
            CorePredicate::All(items) | CorePredicate::Any(items) => {
                items.iter().try_fold(0_u64, |steps, item| {
                    steps.checked_add(self.predicate(item, available, depth + 1)?)
                })
            }
            CorePredicate::Compare { left, right, .. } => self
                .expression(left, available, depth + 1)?
                .checked_add(self.expression(right, available, depth + 1)?),
        }
    }

    fn expression(
        &mut self,
        expression: &'a CoreExpr,
        available: &BTreeMap<&str, &LocalRef>,
        depth: usize,
    ) -> Option<u64> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let children = match expression {
            CoreExpr::Local { .. } | CoreExpr::Const(_) => Some(0),
            CoreExpr::Fold {
                initial,
                step,
                bound,
                ..
            } => {
                let before = self.expression(initial, available, depth + 1)?;
                let step_cost =
                    self.call(step, std::slice::from_ref(initial.as_ref()), available)?;
                before.checked_add(step_cost.checked_mul(*bound)?)
            }
            CoreExpr::Match { scrutinee, arms } => {
                let before = self.expression(scrutinee, available, depth + 1)?;
                let mut largest = 0;
                for arm in arms {
                    let mut scope = available.clone();
                    if let Some(binder) = &arm.binder {
                        if scope.insert(binder.id.as_str(), binder).is_some() {
                            return None;
                        }
                    }
                    largest = largest.max(self.expression(&arm.body, &scope, depth + 1)?);
                }
                before.checked_add(largest)
            }
            CoreExpr::Variant { payload, .. } => match payload {
                Some(payload) => self.expression(payload, available, depth + 1),
                None => Some(0),
            },
            CoreExpr::Field { base, .. } => self.expression(base, available, depth + 1),
            CoreExpr::Record { fields } => fields.values().try_fold(0_u64, |steps, value| {
                steps.checked_add(self.expression(value, available, depth + 1)?)
            }),
            CoreExpr::List { values } | CoreExpr::Call { args: values, .. } => {
                values.iter().try_fold(0_u64, |steps, value| {
                    steps.checked_add(self.expression(value, available, depth + 1)?)
                })
            }
            CoreExpr::If {
                predicate,
                then_value,
                else_value,
            } => self
                .predicate(predicate, available, depth + 1)?
                .checked_add(
                    self.expression(then_value, available, depth + 1)?
                        .max(self.expression(else_value, available, depth + 1)?),
                ),
        }?;
        let CoreExpr::Call { callee, args, .. } = expression else {
            return Some(children);
        };
        children.checked_add(self.call(callee, args, available)?)
    }

    fn call(
        &mut self,
        callee: &'a str,
        args: &'a [CoreExpr],
        available: &BTreeMap<&str, &LocalRef>,
    ) -> Option<u64> {
        let mut intrinsics = self
            .pure_functions
            .iter()
            .filter(|fact| fact.coordinate == callee);
        if let Some(fact) = intrinsics.next() {
            if intrinsics.next().is_some() {
                return None;
            }
            let Some(cost) = &fact.intrinsic_cost else {
                return Some(0);
            };
            self.intrinsic_calls = self.intrinsic_calls.checked_add(1)?;
            if cost.per_argument_max.len() != args.len() {
                return None;
            }
            return cost.per_argument_max.iter().zip(args).try_fold(
                cost.base,
                |steps, (coefficient, argument)| {
                    if *coefficient == 0 {
                        return Some(steps);
                    }
                    let maximum = self.established_max(argument, available)?;
                    steps.checked_add(coefficient.checked_mul(maximum)?)
                },
            );
        }
        let Some(name) = callee
            .strip_prefix(self.core.coordinate.as_str())
            .and_then(|suffix| suffix.strip_prefix('.'))
        else {
            return Some(0);
        };
        let Some((name, function)) = self.core.functions.get_key_value(name) else {
            return Some(0);
        };
        if let Some(&(steps, reaches_intrinsic)) = self.functions.get(name.as_str()) {
            if reaches_intrinsic {
                self.intrinsic_calls = self.intrinsic_calls.checked_add(1)?;
            }
            return Some(steps);
        }
        // Core bounds call height and expression depth separately: a helper's
        // body starts a fresh expression depth, and the call stack has its own
        // limit.
        if self.visiting.contains(&name.as_str()) || self.visiting.len() >= MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        self.visiting.push(name);
        let calls_before = self.intrinsic_calls;
        let scope = locals(function.params.iter().chain(&function.body.locals));
        let steps = function
            .body
            .bindings
            .iter()
            .try_fold(0_u64, |steps, binding| {
                steps.checked_add(self.expression(&binding.value, &scope, 0)?)
            })
            .and_then(|steps| {
                steps.checked_add(self.expression(&function.body.result, &scope, 0)?)
            });
        self.visiting.pop();
        let steps = steps?;
        self.functions
            .insert(name, (steps, self.intrinsic_calls != calls_before));
        Some(steps)
    }

    /// The size bound Target itself establishes for a charged argument.
    /// A conditional is bounded by its larger branch, each established the same way.
    fn established_max(
        &self,
        argument: &CoreExpr,
        available: &BTreeMap<&str, &LocalRef>,
    ) -> Option<u64> {
        self.established_max_at(argument, available, 0)
    }

    fn established_max_at(
        &self,
        argument: &CoreExpr,
        available: &BTreeMap<&str, &LocalRef>,
        depth: usize,
    ) -> Option<u64> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        match argument {
            CoreExpr::Fold { .. } => None,
            CoreExpr::Match { scrutinee, arms } => {
                let mut largest = 0;
                let valid = super::variant_matches::every(
                    self.core,
                    self.pure_functions,
                    scrutinee,
                    arms,
                    available,
                    |body, scope| {
                        let Some(bound) = self.established_max_at(body, scope, depth + 1) else {
                            return false;
                        };
                        largest = largest.max(bound);
                        true
                    },
                );
                valid.then_some(largest)
            }
            CoreExpr::List { values } => u64::try_from(values.len()).ok(),
            CoreExpr::Const(CoreValue::Bytes(bytes)) => u64::try_from(bytes.len()).ok(),
            CoreExpr::If {
                then_value,
                else_value,
                ..
            } => Some(
                self.established_max_at(then_value, available, depth + 1)?
                    .max(self.established_max_at(else_value, available, depth + 1)?),
            ),
            _ => {
                let coordinate = expression_type_coordinate(
                    self.core,
                    self.pure_functions,
                    argument,
                    available,
                )?;
                match resolved_core_type(self.core, &coordinate)? {
                    CoreType::Bytes { max, .. } | CoreType::List { max, .. } => Some(max),
                    _ => None,
                }
            }
        }
    }
}
