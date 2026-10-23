//! Target-owned upper-bound proof over untrusted Core, independent of source checking.
use super::totality::Scope;
use super::{CoreExpr, CorePredicate, CoreValue};
use crate::core_ir::{CompareOp, MAX_CORE_GRAPH_DEPTH};

pub(super) const OPERATION: &str = "core.integer.add";

pub(super) fn is_total(
    core: &Scope<'_>,
    type_args: &[String],
    args: &[CoreExpr],
    constraints: &[&CorePredicate],
) -> bool {
    let [width] = type_args else { return false };
    let max = match width.as_str() {
        "U32" => u64::from(u32::MAX),
        "U64" => u64::MAX,
        _ => return false,
    };
    let [left, right] = args else { return false };
    let mut limits = Vec::new();
    for constraint in constraints {
        collect_limits(constraint, &mut limits);
    }
    bound(core, left, max, &limits, 0)
        .zip(bound(core, right, max, &limits, 0))
        .and_then(|(left, right)| left.checked_add(right))
        .is_some_and(|sum| sum <= max)
}

fn constant(expr: &CoreExpr) -> Option<u64> {
    let CoreExpr::Const(CoreValue::Int { width, value }) = expr else {
        return None;
    };
    match width.as_str() {
        "U32" => value.parse::<u32>().ok().map(u64::from),
        "U64" => value.parse().ok(),
        _ => None,
    }
}

fn collect_limits<'a>(predicate: &'a CorePredicate, limits: &mut Vec<(&'a CoreExpr, u64)>) {
    match predicate {
        CorePredicate::All(items) => {
            for item in items {
                collect_limits(item, limits);
            }
        }
        CorePredicate::Compare { op, left, right } => {
            let strict = matches!(op, CompareOp::Lt | CompareOp::Gt);
            if matches!(op, CompareOp::Le | CompareOp::Lt | CompareOp::Eq) {
                if let Some(value) = constant(right) {
                    limits.push((
                        left,
                        if strict {
                            value.saturating_sub(1)
                        } else {
                            value
                        },
                    ));
                }
            }
            if matches!(op, CompareOp::Ge | CompareOp::Gt | CompareOp::Eq) {
                if let Some(value) = constant(left) {
                    limits.push((
                        right,
                        if strict {
                            value.saturating_sub(1)
                        } else {
                            value
                        },
                    ));
                }
            }
        }
        _ => {}
    }
}

fn bound(
    core: &Scope<'_>,
    expr: &CoreExpr,
    max: u64,
    limits: &[(&CoreExpr, u64)],
    depth: usize,
) -> Option<u64> {
    // Target refuses when traversal cannot establish a bound. Source uses
    // the width maximum at its cutoff, so a source-accepted sum with zero
    // can still be refused here. Target remains the final authority.
    if depth > MAX_CORE_GRAPH_DEPTH {
        return None;
    }
    let structural = if let Some(value) = constant(expr) {
        value
    } else if let CoreExpr::Local { reference } = expr {
        core.local_bound(reference)
            .map_or(max, |upper| upper.min(max))
    } else if let CoreExpr::Call { callee, args, .. } = expr {
        if callee == OPERATION {
            let [left, right] = args.as_slice() else {
                return None;
            };
            bound(core, left, max, limits, depth + 1)?.checked_add(bound(
                core,
                right,
                max,
                limits,
                depth + 1,
            )?)?
        } else if callee == super::byte_count::OPERATION {
            let CoreExpr::Call { type_args, .. } = expr else {
                return None;
            };
            super::byte_count::result_bound(core.module(), type_args)
                .map_or(max, |bound| bound.min(max))
        } else {
            max
        }
    } else {
        max
    };
    Some(
        limits
            .iter()
            .filter(|(term, _)| *term == expr)
            .map(|(_, upper)| *upper)
            .fold(structural, u64::min),
    )
}

/// The proven upper bound a pure binding of unsigned width carries to later
/// evaluation in the same scope, when it is below the width maximum.
pub(super) fn binding_bound(
    core: &Scope<'_>,
    binding: &crate::core_ir::LocalRef,
    value: &CoreExpr,
    constraints: &[&CorePredicate],
) -> Option<u64> {
    let max = match binding.ty.as_str() {
        "U32" => u64::from(u32::MAX),
        "U64" => u64::MAX,
        _ => return None,
    };
    let mut limits = Vec::new();
    for constraint in constraints {
        collect_limits(constraint, &mut limits);
    }
    bound(core, value, max, &limits, 0).filter(|upper| *upper < max)
}
