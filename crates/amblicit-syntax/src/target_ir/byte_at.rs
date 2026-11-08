//! Independently establish byte lookup signature and strict evaluation domain.
use super::{
    expression_type_coordinate, resolved_core_type, BTreeMap, CoreExpr, CoreModule, CorePredicate,
    CoreType, CoreValue, LocalRef, TargetPureFunctionFact,
};
use crate::core_ir::CompareOp;

pub(super) const OPERATION: &str = "core.bytes.at";

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    let ([coordinate], [bytes, index]) = (type_args, args) else {
        return None;
    };
    if !matches!(
        resolved_core_type(core, coordinate),
        Some(CoreType::Bytes { .. })
    ) || expression_type_coordinate(core, functions, bytes, available).as_ref()
        != Some(coordinate)
        || expression_type_coordinate(core, functions, index, available).as_deref() != Some("U64")
    {
        return None;
    }
    Some("U8".into())
}

pub(super) fn is_total(
    core: &CoreModule,
    type_args: &[String],
    args: &[CoreExpr],
    constraints: &[&CorePredicate],
) -> bool {
    let ([coordinate], [bytes, index]) = (type_args, args) else {
        return false;
    };
    let Some(CoreType::Bytes { min, .. }) = resolved_core_type(core, coordinate) else {
        return false;
    };
    if let CoreExpr::Const(CoreValue::Int { width, value }) = index {
        if width == "U64"
            && value
                .parse::<u64>()
                .ok()
                .zip(min)
                .is_some_and(|(index, minimum)| index < minimum)
        {
            return true;
        }
    }
    let length = CoreExpr::Call {
        callee: super::byte_length::OPERATION.into(),
        type_args: vec![coordinate.clone()],
        args: vec![bytes.clone()],
    };
    constraints
        .iter()
        .any(|p| establishes(p, true, index, &length))
}

fn establishes(
    predicate: &CorePredicate,
    holds: bool,
    index: &CoreExpr,
    length: &CoreExpr,
) -> bool {
    match predicate {
        CorePredicate::Compare { op, left, right } => {
            if left == index && right == length {
                matches!((op, holds), (CompareOp::Lt, true) | (CompareOp::Ge, false))
            } else if left == length && right == index {
                matches!((op, holds), (CompareOp::Gt, true) | (CompareOp::Le, false))
            } else {
                false
            }
        }
        CorePredicate::Not(inner) => establishes(inner, !holds, index, length),
        CorePredicate::All(items) if holds => items
            .iter()
            .any(|item| establishes(item, true, index, length)),
        CorePredicate::Any(items) if !holds => items
            .iter()
            .any(|item| establishes(item, false, index, length)),
        _ => false,
    }
}
