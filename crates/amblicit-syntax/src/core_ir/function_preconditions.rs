//! Source-function `preconditions`: their integrity and their call-site meaning.
use std::collections::BTreeMap;

use super::{
    source_functions::{scope_predicate, walk_predicate},
    validate_core_predicate_types, CoreExpr, CoreFunction, CoreMatchArm, CoreModule, CorePredicate,
    CoreTypeIntegrityFailure, CoreTypeIntegrityFailureKind, CoreTypeIntegrityState, LocalRef,
};

/// The most conjuncts a function's preconditions may hold in total, counting
/// through nested Core `all` (`EDICT-CORE-FN-PRECONDITION-001`). Re-proving a
/// call then scans the dominating facts at most this many times, which keeps
/// the proof work in the same class as checking a subtraction.
pub const MAX_FUNCTION_PRECONDITION_CONJUNCTS: usize = 16;

/// Conjuncts of a predicate list, counting through nested Core `all`.
pub(crate) fn precondition_conjuncts(preconditions: &[CorePredicate]) -> usize {
    fn count(predicate: &CorePredicate) -> usize {
        match predicate {
            CorePredicate::All(items) => items.iter().map(count).sum(),
            CorePredicate::True
            | CorePredicate::False
            | CorePredicate::Not(_)
            | CorePredicate::Any(_)
            | CorePredicate::Compare { .. } => 1,
        }
    }
    preconditions.iter().map(count).sum()
}

/// A precondition is a typed predicate over the function's parameters alone.
/// It may not call a source function, fold, or match, so a call site can
/// instantiate it by substituting argument expressions for parameters.
pub(super) fn validate_preconditions(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    path: &str,
    function: &CoreFunction,
) -> Result<(), CoreTypeIntegrityFailure> {
    let params = function
        .params
        .iter()
        .map(|local| (local.id.as_str(), local))
        .collect::<BTreeMap<_, _>>();
    let prefix = format!("{}.", module.coordinate);
    for (index, precondition) in function.preconditions.iter().enumerate() {
        let at = format!("{path}.preconditions[{index}]");
        validate_core_predicate_types(module, state, precondition, &at, 0)?;
        scope_predicate(precondition, 0, &params, &at)?;
        walk_predicate(precondition, 0, &mut |expression| match expression {
            CoreExpr::Call { callee, .. }
                if callee
                    .strip_prefix(&prefix)
                    .is_some_and(|name| module.functions.contains_key(name)) =>
            {
                Err(invalid(&at))
            }
            CoreExpr::Fold { .. } | CoreExpr::Match { .. } => Err(invalid(&at)),
            // Exhaustive on purpose: a new expression form must decide here.
            CoreExpr::Call { .. }
            | CoreExpr::Local { .. }
            | CoreExpr::Const(_)
            | CoreExpr::Variant { .. }
            | CoreExpr::Record { .. }
            | CoreExpr::List { .. }
            | CoreExpr::Field { .. }
            | CoreExpr::If { .. } => Ok(()),
        })?;
    }
    // Counted only after every predicate passed the depth-checked walks above.
    if precondition_conjuncts(&function.preconditions) > MAX_FUNCTION_PRECONDITION_CONJUNCTS {
        return Err(invalid(&format!("{path}.preconditions")));
    }
    Ok(())
}

fn invalid(path: &str) -> CoreTypeIntegrityFailure {
    CoreTypeIntegrityFailure::new(CoreTypeIntegrityFailureKind::InvalidDefinition, path)
}

/// The obligation a call creates: the precondition with each parameter local
/// replaced by the argument expression in the same position. Locals that are
/// not parameters are left unchanged.
pub(crate) fn instantiate_precondition(
    precondition: &CorePredicate,
    params: &[LocalRef],
    args: &[CoreExpr],
) -> CorePredicate {
    let bindings = params.iter().zip(args).collect::<Vec<_>>();
    predicate(precondition, &bindings)
}

type Bindings<'a> = [(&'a LocalRef, &'a CoreExpr)];

fn predicate(value: &CorePredicate, bindings: &Bindings<'_>) -> CorePredicate {
    match value {
        CorePredicate::True => CorePredicate::True,
        CorePredicate::False => CorePredicate::False,
        CorePredicate::Not(inner) => CorePredicate::Not(Box::new(predicate(inner, bindings))),
        CorePredicate::All(items) => {
            CorePredicate::All(items.iter().map(|item| predicate(item, bindings)).collect())
        }
        CorePredicate::Any(items) => {
            CorePredicate::Any(items.iter().map(|item| predicate(item, bindings)).collect())
        }
        CorePredicate::Compare { op, left, right } => CorePredicate::Compare {
            op: *op,
            left: expression(left, bindings),
            right: expression(right, bindings),
        },
    }
}

fn expression(value: &CoreExpr, bindings: &Bindings<'_>) -> CoreExpr {
    let boxed = |inner: &CoreExpr| Box::new(expression(inner, bindings));
    match value {
        CoreExpr::Local { reference } => bindings
            .iter()
            .find(|(param, _)| *param == reference)
            .map_or_else(|| value.clone(), |(_, argument)| (*argument).clone()),
        CoreExpr::Const(_) => value.clone(),
        CoreExpr::Variant { ty, case, payload } => CoreExpr::Variant {
            ty: ty.clone(),
            case: case.clone(),
            payload: payload.as_deref().map(boxed),
        },
        CoreExpr::Fold {
            initial,
            step,
            bound,
            state_type,
        } => CoreExpr::Fold {
            initial: boxed(initial),
            step: step.clone(),
            bound: *bound,
            state_type: state_type.clone(),
        },
        CoreExpr::Match { scrutinee, arms } => CoreExpr::Match {
            scrutinee: boxed(scrutinee),
            arms: arms
                .iter()
                .map(|arm| CoreMatchArm {
                    case: arm.case.clone(),
                    binder: arm.binder.clone(),
                    body: expression(&arm.body, bindings),
                })
                .collect(),
        },
        CoreExpr::Record { fields } => CoreExpr::Record {
            fields: fields
                .iter()
                .map(|(name, field)| (name.clone(), expression(field, bindings)))
                .collect(),
        },
        CoreExpr::List { values } => CoreExpr::List {
            values: values
                .iter()
                .map(|item| expression(item, bindings))
                .collect(),
        },
        CoreExpr::Field { base, field } => CoreExpr::Field {
            base: boxed(base),
            field: field.clone(),
        },
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } => CoreExpr::Call {
            callee: callee.clone(),
            type_args: type_args.clone(),
            args: args.iter().map(|item| expression(item, bindings)).collect(),
        },
        CoreExpr::If {
            predicate: condition,
            then_value,
            else_value,
        } => CoreExpr::If {
            predicate: Box::new(predicate(condition, bindings)),
            then_value: boxed(then_value),
            else_value: boxed(else_value),
        },
    }
}
