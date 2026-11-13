//! Intent-level failure maps: `let x = f(...) else { Name => Obstruction };`.
//!
//! The map must name each failure of the callee exactly once by its bare
//! name; every arm is a terminal obstruction written like a `require` target.
use std::collections::BTreeMap;

use super::{
    error, next_local, BodyState, CompilerErrorKind, CompilerStage, CoreNode,
    CoreObstructionReason, LetStatement, LocalEnvironment, LocalRef, ObstructionHandler,
    TypeChecker,
};

impl TypeChecker<'_> {
    /// `let x = f(...) else { Name => Obstruction, ... };` in an intent body.
    pub(super) fn check_failure_mapped_let(
        &mut self,
        stmt: &LetStatement<'_>,
        handler: &ObstructionHandler,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
        state: &mut BodyState,
    ) {
        let name = self
            .source_call_name(stmt.value)
            .unwrap_or_default()
            .to_owned();
        let failures = self.function_failures(&name).to_vec();
        let failure_map = self.check_failure_map(stmt, handler, &failures, env);
        let annotation = match stmt.ty {
            Some(annotation) => {
                let Some(shape) = self.type_ref_shape(annotation, stmt.span) else {
                    self.check_expr_with_unavailable_expected_type(stmt.value, env);
                    return;
                };
                Some(shape)
            }
            None => None,
        };
        let value = self.with_permitted_call(stmt.value, |this| {
            this.check_expr_with_expected(stmt.value, env, annotation.as_ref())
        });
        let (Some(value), Some(failure_map)) = (value, failure_map) else {
            return;
        };
        let Some(shape) = self.pure_let_binding_shape(stmt, &value, annotation) else {
            return;
        };
        let binding = next_local(&mut state.local_index, shape.coord.clone());
        self.record_binding_bound(&binding, &value.expr);
        state.nodes.push(CoreNode::Attempt {
            binding: binding.clone(),
            value: value.expr,
            failure_map,
        });
        locals.push(binding.clone());
        env.insert(stmt.name.to_owned(), (binding, shape));
    }

    fn check_failure_map(
        &mut self,
        stmt: &LetStatement<'_>,
        handler: &ObstructionHandler,
        failures: &[String],
        env: &LocalEnvironment,
    ) -> Option<BTreeMap<String, CoreObstructionReason>> {
        let mut map = BTreeMap::new();
        let mut accepted = true;
        match handler {
            ObstructionHandler::Single(target) => {
                let [failure] = failures else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::IncompleteFailureMapping,
                        format!(
                            "`else Obstruction` needs exactly one failure; the call can fail with {}",
                            describe(failures)
                        ),
                        stmt.span,
                    ));
                    return None;
                };
                let reason = self.check_terminal_require_reason(target, env)?;
                map.insert(failure.clone(), reason);
                return Some(map);
            }
            ObstructionHandler::Map(arms) => {
                for arm in arms {
                    let coordinate = failures
                        .iter()
                        .find(|coordinate| self.failures.names[*coordinate] == arm.failure)
                        .cloned();
                    let reason = self.check_terminal_require_reason(&arm.target, env);
                    let Some(coordinate) = coordinate else {
                        self.errors.push(error(
                            CompilerStage::TypeCheck,
                            CompilerErrorKind::UnknownFailure,
                            format!(
                                "`{}` is not a failure of this call, which can fail with {}",
                                arm.failure,
                                describe(failures)
                            ),
                            arm.span,
                        ));
                        accepted = false;
                        continue;
                    };
                    if arm.binder.is_some() {
                        self.failure_shape::<()>(
                            format!("failure `{}` has no payload to bind", arm.failure),
                            arm.span,
                        );
                        accepted = false;
                    }
                    if map.contains_key(&coordinate) {
                        self.errors.push(error(
                            CompilerStage::TypeCheck,
                            CompilerErrorKind::DuplicateObstructionFailure,
                            format!("failure `{}` is mapped more than once", arm.failure),
                            arm.span,
                        ));
                        accepted = false;
                        continue;
                    }
                    match reason {
                        Some(reason) => {
                            map.insert(coordinate, reason);
                        }
                        None => accepted = false,
                    }
                }
            }
        }
        let missing: Vec<_> = failures
            .iter()
            .filter(|failure| !map.contains_key(*failure))
            .map(|failure| self.failures.names[failure].clone())
            .collect();
        if accepted && !missing.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::IncompleteFailureMapping,
                format!("the failure map does not handle {}", missing.join(", ")),
                stmt.span,
            ));
            return None;
        }
        accepted.then_some(map)
    }
}

fn describe(failures: &[String]) -> String {
    if failures.is_empty() {
        "no failures".to_owned()
    } else {
        failures.join(", ")
    }
}
