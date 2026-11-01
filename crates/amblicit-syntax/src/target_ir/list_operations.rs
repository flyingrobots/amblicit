//! Target-owned signature and totality checks; source proofs are not trusted.
use super::{
    expression_fits_declared_type, expression_type_coordinate, resolved_core_type, BTreeMap,
    CompareOp, CoreExpr, CoreModule, CorePredicate, CoreType, CoreValue, LocalRef,
    TargetPureFunctionFact,
};
use crate::core_ir::render_self_describing_core_type;

pub(super) fn is_operation(callee: &str) -> bool {
    matches!(
        callee,
        "core.list.length" | "core.list.at" | "core.list.append" | "core.list.concat"
    )
}

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    callee: &str,
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    if !core
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.list/v1")
    {
        return None;
    }
    let coordinate = type_args.first()?;
    let CoreType::List { item, max } = resolved_core_type(core, coordinate)? else {
        return None;
    };
    if !expression_fits_declared_type(core, functions, args.first()?, coordinate, available) {
        return None;
    }
    if matches!(callee, "core.list.append" | "core.list.concat") {
        let operand = expression_type_coordinate(core, functions, args.first()?, available)?;
        if resolved_core_type(core, &operand)? != resolved_core_type(core, coordinate)? {
            return None;
        }
    }
    match (callee, type_args, args) {
        ("core.list.length", [_], [_]) => Some("U64".into()),
        ("core.list.at", [_], [_, index]) => {
            expression_fits_declared_type(core, functions, index, "U64", available).then_some(item)
        }
        ("core.list.append", [_], [_, value]) => {
            expression_fits_declared_type(core, functions, value, &item, available)
                .then(|| coordinate.clone())
        }
        ("core.list.concat", [_, right_coordinate], [_, right]) => {
            let CoreType::List {
                item: right_item,
                max: right_max,
            } = resolved_core_type(core, right_coordinate)?
            else {
                return None;
            };
            if right_item != item
                || resolved_core_type(
                    core,
                    &expression_type_coordinate(core, functions, right, available)?,
                )? != resolved_core_type(core, right_coordinate)?
                || !expression_fits_declared_type(
                    core,
                    functions,
                    right,
                    right_coordinate,
                    available,
                )
            {
                return None;
            }
            render_self_describing_core_type(&CoreType::List {
                item,
                max: max.checked_add(right_max)?,
            })
        }
        _ => None,
    }
}

pub(super) fn is_total(
    core: &CoreModule,
    callee: &str,
    type_args: &[String],
    args: &[CoreExpr],
    constraints: &[&CorePredicate],
) -> bool {
    if !matches!(callee, "core.list.at" | "core.list.append") {
        return true;
    }
    let ([coordinate], [list, operand]) = (type_args, args) else {
        return false;
    };
    let Some(CoreType::List { max, .. }) = resolved_core_type(core, coordinate) else {
        return false;
    };
    let length = CoreExpr::Call {
        callee: "core.list.length".into(),
        type_args: vec![coordinate.clone()],
        args: vec![list.clone()],
    };
    let capacity = CoreExpr::Const(CoreValue::Int {
        width: "U64".into(),
        value: max.to_string(),
    });
    let (lower, upper) = if callee == "core.list.at" {
        (operand, &length)
    } else {
        (&length, &capacity)
    };
    constraints
        .iter()
        .any(|p| establishes(p, true, lower, upper))
}

fn establishes(predicate: &CorePredicate, holds: bool, lower: &CoreExpr, upper: &CoreExpr) -> bool {
    match predicate {
        CorePredicate::Compare { op, left, right } => {
            if left == lower && right == upper {
                matches!((op, holds), (CompareOp::Lt, true) | (CompareOp::Ge, false))
            } else if left == upper && right == lower {
                matches!((op, holds), (CompareOp::Gt, true) | (CompareOp::Le, false))
            } else {
                false
            }
        }
        CorePredicate::Not(inner) => establishes(inner, !holds, lower, upper),
        CorePredicate::All(items) if holds => {
            items.iter().any(|p| establishes(p, true, lower, upper))
        }
        CorePredicate::Any(items) if !holds => {
            items.iter().any(|p| establishes(p, false, lower, upper))
        }
        _ => false,
    }
}
