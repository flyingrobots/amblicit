//! Independent signature, authority and bound checks for raw byte concatenation.
use super::{
    expression_type_coordinate, resolved_core_type, BTreeMap, CoreExpr, CoreModule, CoreType,
    LocalRef, TargetPureFunctionFact,
};

pub(super) const OPERATION: &str = "core.bytes.concat";

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    let ([left_coordinate, right_coordinate], [left, right]) = (type_args, args) else {
        return None;
    };
    let CoreType::Bytes { max: left_max, .. } = resolved_core_type(core, left_coordinate)? else {
        return None;
    };
    let CoreType::Bytes { max: right_max, .. } = resolved_core_type(core, right_coordinate)? else {
        return None;
    };
    if expression_type_coordinate(core, functions, left, available).as_ref()
        != Some(left_coordinate)
        || expression_type_coordinate(core, functions, right, available).as_ref()
            != Some(right_coordinate)
    {
        return None;
    }
    let max = left_max.checked_add(right_max)?;
    Some(format!("Bytes<max={max}>"))
}
