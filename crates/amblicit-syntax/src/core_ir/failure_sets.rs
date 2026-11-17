//! Independent Core judgment for typed function failures (EDICT-CORE-FAIL-001).
//!
//! It runs after the type-integrity judgment, so graph depth is already
//! bounded. Every rule here is checked from Core alone; nothing trusts the
//! source compiler.
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use super::source_functions::{walk, walk_predicate};
use super::{
    CoreBlock, CoreExpr, CoreModule, CoreNode, CorePredicate, CoreRequireFailureArm,
    CoreTypeIntegrityFailure, CoreTypeIntegrityFailureKind,
};

/// The Core capability a module needs once any function declares failures.
pub(crate) const CORE_FAILURE_CAPABILITY: &str = "core.failure/v1";
pub(crate) const MAX_RECORD_ORDER_WORK: usize = 1 << 22;

type Failure = CoreTypeIntegrityFailure;

fn invalid(path: &str) -> Failure {
    CoreTypeIntegrityFailure::new(CoreTypeIntegrityFailureKind::InvalidDefinition, path)
}

/// Checked expressions expose failures through explicit terminals or through
/// a source callee's declared set. No unknown call is assumed to fail.
pub(crate) fn expression_can_fail(
    expression: &CoreExpr,
    mut callee_can_fail: impl FnMut(&str) -> bool,
    remaining: &mut usize,
) -> Result<bool, Failure> {
    let mut can_fail = false;
    walk(expression, 0, &mut |node| {
        *remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| invalid("records.failureOrderWork"))?;
        can_fail |= match node {
            CoreExpr::Fail { .. } => true,
            CoreExpr::Call { callee, .. } => callee_can_fail(callee),
            _ => false,
        };
        Ok(())
    })?;
    Ok(can_fail)
}

struct Sets<'a> {
    module: &'a CoreModule,
    declared: BTreeMap<&'a str, &'a [String]>,
    record_order_work: Cell<usize>,
}

impl<'a> Sets<'a> {
    /// The declared set of the module function named by a call coordinate.
    fn callee(&self, callee: &str) -> Option<&'a [String]> {
        let name = callee.strip_prefix(&format!("{}.", self.module.coordinate))?;
        self.declared.get(name).copied()
    }

    /// Record what one expression node can produce. A failing fold step is
    /// refused at `path`, because a fold can neither propagate nor map it.
    fn classify(
        &self,
        expression: &CoreExpr,
        produced: &mut BTreeSet<String>,
        path: &str,
    ) -> Result<(), Failure> {
        match expression {
            CoreExpr::Fail { failure } => {
                produced.insert(failure.clone());
            }
            CoreExpr::Call { callee, .. } => {
                produced.extend(self.callee(callee).unwrap_or_default().iter().cloned());
            }
            CoreExpr::Fold { step, .. } if !self.callee(step).unwrap_or_default().is_empty() => {
                return Err(invalid(path));
            }
            CoreExpr::Record { .. } | CoreExpr::OrderedRecord { .. } => {
                let mut remaining = self.record_order_work.get();
                let can_fail = expression_can_fail(
                    expression,
                    |callee| !self.callee(callee).unwrap_or_default().is_empty(),
                    &mut remaining,
                )?;
                self.record_order_work.set(remaining);
                if can_fail != matches!(expression, CoreExpr::OrderedRecord { .. }) {
                    return Err(invalid(path));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The failures an expression can produce.
    fn produced(&self, expression: &CoreExpr, path: &str) -> Result<BTreeSet<String>, Failure> {
        let mut produced = BTreeSet::new();
        walk(expression, 0, &mut |node| {
            self.classify(node, &mut produced, path)
        })?;
        Ok(produced)
    }

    fn failure_free(&self, expression: &CoreExpr, path: &str) -> Result<(), Failure> {
        if self.produced(expression, path)?.is_empty() {
            Ok(())
        } else {
            Err(invalid(path))
        }
    }

    fn failure_free_predicate(&self, predicate: &CorePredicate, path: &str) -> Result<(), Failure> {
        let mut produced = BTreeSet::new();
        walk_predicate(predicate, 0, &mut |node| {
            self.classify(node, &mut produced, path)
        })?;
        if produced.is_empty() {
            Ok(())
        } else {
            Err(invalid(path))
        }
    }

    fn function(&self, name: &str) -> Result<(), Failure> {
        let function = &self.module.functions[name];
        let path = format!("functions.{name}");
        let failures = format!("{path}.failures");
        if !function.failures.windows(2).all(|pair| pair[0] < pair[1])
            || function.failures.iter().any(String::is_empty)
        {
            return Err(invalid(&failures));
        }
        let body = format!("{path}.body");
        let mut produced = BTreeSet::new();
        for binding in &function.body.bindings {
            produced.extend(self.produced(&binding.value, &body)?);
        }
        produced.extend(self.produced(&function.body.result, &body)?);
        if produced.iter().ne(function.failures.iter()) {
            return Err(invalid(&failures));
        }
        Ok(())
    }

    fn block(&self, block: &CoreBlock, path: &str) -> Result<(), Failure> {
        for (index, node) in block.nodes.iter().enumerate() {
            self.node(node, &format!("{path}.nodes[{index}]"))?;
        }
        self.failure_free(&block.result, &format!("{path}.result"))
    }

    fn node(&self, node: &CoreNode, path: &str) -> Result<(), Failure> {
        match node {
            CoreNode::Attempt {
                value, failure_map, ..
            } => {
                let CoreExpr::Call { callee, args, .. } = value else {
                    return Err(invalid(path));
                };
                let declared = self.callee(callee).unwrap_or_default();
                if declared.is_empty() {
                    return Err(invalid(path));
                }
                for argument in args {
                    self.failure_free(argument, path)?;
                }
                if failure_map.keys().ne(declared.iter()) {
                    return Err(invalid(&format!("{path}.failureMap")));
                }
                for reason in failure_map.values() {
                    for payload in reason.payload.values() {
                        self.failure_free(payload, path)?;
                    }
                }
                Ok(())
            }
            CoreNode::Let { value, .. } => self.failure_free(value, path),
            CoreNode::Require { predicate, arm } => {
                self.failure_free_predicate(predicate, path)?;
                let (CoreRequireFailureArm::Terminal { reason }
                | CoreRequireFailureArm::ContinueObstructed { reason }) = arm;
                reason
                    .payload
                    .values()
                    .try_for_each(|payload| self.failure_free(payload, path))
            }
            CoreNode::Effect {
                input,
                obstruction_map,
                ..
            } => {
                self.failure_free(input, path)?;
                obstruction_map
                    .values()
                    .try_for_each(|arm| self.failure_free(&arm.value, path))
            }
            CoreNode::ExternalActionRequest {
                input,
                authority_scope,
                basis,
                budget,
                ..
            } => [
                input,
                authority_scope.as_ref(),
                basis.as_ref(),
                &budget.max_settlement_bytes,
                &budget.max_attempts,
            ]
            .into_iter()
            .try_for_each(|expression| self.failure_free(expression, path)),
            CoreNode::For { iter, body, .. } => {
                self.failure_free(iter, path)?;
                self.block(body, &format!("{path}.for.body"))
            }
            CoreNode::Branch {
                predicate,
                then_block,
                else_block,
                ..
            } => {
                self.failure_free_predicate(predicate, path)?;
                self.block(then_block, &format!("{path}.branch.then"))?;
                self.block(else_block, &format!("{path}.branch.else"))
            }
        }
    }
}

/// Judge declared failure sets, `fail` placement, `attempt` maps and the
/// `core.failure/v1` capability.
pub(super) fn validate_failure_sets(module: &CoreModule) -> Result<(), Failure> {
    let declares = module
        .functions
        .values()
        .any(|function| !function.failures.is_empty());
    let capable = module
        .required_core_capabilities
        .iter()
        .any(|capability| capability == CORE_FAILURE_CAPABILITY);
    if declares != capable {
        return Err(invalid("requiredCoreCapabilities"));
    }
    let sets = Sets {
        module,
        record_order_work: Cell::new(MAX_RECORD_ORDER_WORK),
        declared: module
            .functions
            .iter()
            .map(|(name, function)| (name.as_str(), function.failures.as_slice()))
            .collect(),
    };
    for name in module.functions.keys() {
        sets.function(name)?;
    }
    for (name, intent) in &module.intents {
        let path = format!("intents.{name}");
        if let Some(basis) = &intent.basis {
            sets.failure_free(basis, &format!("{path}.basis"))?;
        }
        for (index, constraint) in intent.input_constraints.iter().enumerate() {
            sets.failure_free_predicate(
                &constraint.predicate,
                &format!("{path}.inputConstraints[{index}].predicate"),
            )?;
        }
        sets.block(&intent.body, &format!("{path}.body"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::expression_can_fail;
    use crate::core_ir::{CoreExpr, CoreTypeIntegrityFailureKind};

    #[test]
    fn record_failure_analysis_refuses_exhausted_work_without_assuming_totality() {
        let record = CoreExpr::OrderedRecord {
            fields: vec![
                (
                    "z".into(),
                    CoreExpr::Fail {
                        failure: "test@1.Stop".into(),
                    },
                ),
                (
                    "a".into(),
                    CoreExpr::Call {
                        callee: "test@1.partial".into(),
                        type_args: vec![],
                        args: vec![],
                    },
                ),
            ],
        };
        let mut exact = 3;
        assert!(
            expression_can_fail(&record, |callee| callee == "test@1.partial", &mut exact)
                .expect("three expression nodes fit")
        );
        assert_eq!(exact, 0);
        let mut short = 2;
        let failure = expression_can_fail(&record, |_| false, &mut short)
            .expect_err("a terminal does not excuse unvisited expressions");
        assert_eq!(
            failure.kind(),
            CoreTypeIntegrityFailureKind::InvalidDefinition
        );
        assert_eq!(failure.path(), "records.failureOrderWork");
    }
}
