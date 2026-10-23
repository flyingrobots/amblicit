//! Independent signature and input-domain checks for raw byte slicing.
use super::{
    expression_type_coordinate, resolved_core_type, unsigned_subtraction::proven_order, BTreeMap,
    CoreExpr, CoreModule, CorePredicate, CoreType, LocalRef, TargetPureFunctionFact,
};

pub(super) const OPERATION: &str = "core.bytes.slice";

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    let ([coordinate], [bytes, start, end]) = (type_args, args) else {
        return None;
    };
    let CoreType::Bytes { max, .. } = resolved_core_type(core, coordinate)? else {
        return None;
    };
    if expression_type_coordinate(core, functions, bytes, available).as_ref() != Some(coordinate)
        || expression_type_coordinate(core, functions, start, available).as_deref() != Some("U64")
        || expression_type_coordinate(core, functions, end, available).as_deref() != Some("U64")
    {
        return None;
    }
    Some(format!("Bytes<max={max}>"))
}

pub(super) fn is_total(
    type_args: &[String],
    args: &[CoreExpr],
    constraints: &[&CorePredicate],
) -> bool {
    let ([coordinate], [bytes, start, end]) = (type_args, args) else {
        return false;
    };
    let length = CoreExpr::Call {
        callee: super::byte_length::OPERATION.into(),
        type_args: vec![coordinate.clone()],
        args: vec![bytes.clone()],
    };
    proven_order(end, start, constraints) && proven_order(&length, end, constraints)
}
