//! Target-owned judgment of source-function preconditions over untrusted Core.
//! The compiler's proof is not consulted: Target re-proves each call site from
//! the facts that dominate it, and seeds each body with its own preconditions.
use super::{unsigned_subtraction::proven_order, CoreExpr, CoreModule, CorePredicate};
use crate::core_ir::{instantiate_precondition, CompareOp, CoreFunction, LocalRef};

/// The source function a callee names, when it names one.
pub(super) fn source_function<'a>(
    module: &'a CoreModule,
    callee: &str,
) -> Option<&'a CoreFunction> {
    callee
        .strip_prefix(module.coordinate.as_str())?
        .strip_prefix('.')
        .and_then(|name| module.functions.get(name))
}

/// Whether every precondition of a source-function call, instantiated with
/// its arguments, follows from the dominating facts. Non-source calls and
/// precondition-free functions impose nothing.
pub(super) fn call_is_established(
    module: &CoreModule,
    callee: &str,
    args: &[CoreExpr],
    facts: &[&CorePredicate],
) -> bool {
    let Some(function) = source_function(module, callee) else {
        return true;
    };
    args.len() == function.params.len()
        && function.preconditions.iter().all(|precondition| {
            let obligation = instantiate_precondition(precondition, &function.params, args);
            discharged(&obligation, facts)
        })
}

/// A precondition names parameters and pure operations only: no source call,
/// fold or match, so call-site instantiation never re-binds or evaluates.
pub(super) fn has_admissible_shape(module: &CoreModule, predicate: &CorePredicate) -> bool {
    match predicate {
        CorePredicate::True | CorePredicate::False => true,
        CorePredicate::Not(inner) => has_admissible_shape(module, inner),
        CorePredicate::All(items) | CorePredicate::Any(items) => {
            items.iter().all(|item| has_admissible_shape(module, item))
        }
        CorePredicate::Compare { left, right, .. } => {
            expression_is_admissible(module, left) && expression_is_admissible(module, right)
        }
    }
}

fn expression_is_admissible(module: &CoreModule, expression: &CoreExpr) -> bool {
    match expression {
        CoreExpr::Fold { .. } | CoreExpr::Match { .. } => false,
        CoreExpr::Local { .. } | CoreExpr::Const(_) => true,
        CoreExpr::Variant { payload, .. } => payload
            .as_deref()
            .is_none_or(|payload| expression_is_admissible(module, payload)),
        CoreExpr::Field { base, .. } => expression_is_admissible(module, base),
        CoreExpr::Record { fields } => fields
            .values()
            .all(|value| expression_is_admissible(module, value)),
        CoreExpr::List { values } => values
            .iter()
            .all(|value| expression_is_admissible(module, value)),
        CoreExpr::Call { callee, args, .. } => {
            source_function(module, callee).is_none()
                && args
                    .iter()
                    .all(|value| expression_is_admissible(module, value))
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            has_admissible_shape(module, predicate)
                && expression_is_admissible(module, then_value)
                && expression_is_admissible(module, else_value)
        }
    }
}

/// Whether every local a precondition reads, including inside conditional
/// predicates, is exactly one of the parameters. A precondition naming any
/// other local would let a call site's unrelated local satisfy it.
pub(super) fn names_only(predicate: &CorePredicate, params: &[LocalRef]) -> bool {
    match predicate {
        CorePredicate::True | CorePredicate::False => true,
        CorePredicate::Not(inner) => names_only(inner, params),
        CorePredicate::All(items) | CorePredicate::Any(items) => {
            items.iter().all(|item| names_only(item, params))
        }
        CorePredicate::Compare { left, right, .. } => {
            expression_names_only(left, params) && expression_names_only(right, params)
        }
    }
}

fn expression_names_only(expression: &CoreExpr, params: &[LocalRef]) -> bool {
    match expression {
        CoreExpr::Local { reference } => params.contains(reference),
        CoreExpr::Const(_) => true,
        CoreExpr::Fold { .. } | CoreExpr::Match { .. } => false,
        CoreExpr::Variant { payload, .. } => payload
            .as_deref()
            .is_none_or(|payload| expression_names_only(payload, params)),
        CoreExpr::Field { base, .. } => expression_names_only(base, params),
        CoreExpr::Record { fields } => fields
            .values()
            .all(|value| expression_names_only(value, params)),
        CoreExpr::List { values } | CoreExpr::Call { args: values, .. } => values
            .iter()
            .all(|value| expression_names_only(value, params)),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            names_only(predicate, params)
                && expression_names_only(then_value, params)
                && expression_names_only(else_value, params)
        }
    }
}

fn discharged(obligation: &CorePredicate, facts: &[&CorePredicate]) -> bool {
    match obligation {
        CorePredicate::True => true,
        CorePredicate::All(items) => items.iter().all(|item| discharged(item, facts)),
        CorePredicate::Compare {
            op: CompareOp::Le,
            left,
            right,
        } => proven_order(right, left, facts),
        CorePredicate::Compare {
            op: CompareOp::Ge,
            left,
            right,
        } => proven_order(left, right, facts),
        _ => facts.iter().any(|fact| contains(fact, obligation)),
    }
}

fn contains(fact: &CorePredicate, wanted: &CorePredicate) -> bool {
    match fact {
        CorePredicate::All(items) if fact != wanted => {
            items.iter().any(|item| contains(item, wanted))
        }
        _ => fact == wanted || mirrors(fact, wanted),
    }
}

/// Whether `wanted` is `fact` with its operands swapped and its operator
/// mirrored (`a < b` and `b > a`; `a == b` and `b == a`). Borrows only.
fn mirrors(fact: &CorePredicate, wanted: &CorePredicate) -> bool {
    let (
        CorePredicate::Compare { op, left, right },
        CorePredicate::Compare {
            op: wanted_op,
            left: wanted_left,
            right: wanted_right,
        },
    ) = (fact, wanted)
    else {
        return false;
    };
    let mirrored = match op {
        CompareOp::Lt => CompareOp::Gt,
        CompareOp::Gt => CompareOp::Lt,
        CompareOp::Le => CompareOp::Ge,
        CompareOp::Ge => CompareOp::Le,
        CompareOp::Eq => CompareOp::Eq,
        CompareOp::Ne => CompareOp::Ne,
    };
    *wanted_op == mirrored && wanted_left == right && wanted_right == left
}

#[cfg(test)]
mod tests {
    use super::{names_only, CoreExpr, CorePredicate, LocalRef};
    use crate::core_ir::{CompareOp, CoreValue};

    fn local(id: &str, alpha: &str) -> LocalRef {
        LocalRef {
            id: id.into(),
            alpha_name: alpha.into(),
            ty: "U64".into(),
        }
    }

    fn compare(left: CoreExpr, right: CoreExpr) -> CorePredicate {
        CorePredicate::Compare {
            op: CompareOp::Le,
            left,
            right,
        }
    }

    /// Target's own scope rule, independent of Core integrity (which refuses
    /// the same inputs first inside `lower_to_target_ir`).
    #[test]
    fn names_only_admits_exactly_the_parameters() {
        let params = [local("arg.0", "$arg0"), local("arg.1", "$arg1")];
        let param = |index: usize| CoreExpr::Local {
            reference: params[index].clone(),
        };
        let stranger = CoreExpr::Local {
            reference: local("local.0", "$local0"),
        };
        let renamed = CoreExpr::Local {
            reference: local("arg.0", "$other"),
        };
        assert!(names_only(&compare(param(0), param(1)), &params));
        assert!(!names_only(&compare(stranger.clone(), param(1)), &params));
        assert!(!names_only(&compare(renamed, param(1)), &params));
        // A local hidden in a conditional's own predicate is still found.
        let hidden = CoreExpr::If {
            predicate: Box::new(compare(stranger, param(0))),
            then_value: Box::new(CoreExpr::Const(CoreValue::Bool(true))),
            else_value: Box::new(CoreExpr::Const(CoreValue::Bool(false))),
        };
        assert!(!names_only(&compare(hidden, param(1)), &params));
    }
}
