//! Independently validate a single-byte count at the untrusted Core boundary.
use super::{
    expression_type_coordinate, resolved_core_type, BTreeMap, CoreExpr, CoreModule, CoreType,
    CoreValue, LocalRef, TargetPureFunctionFact,
};
use crate::core_ir::{classify_core_type_reference, CoreTypeReference};

pub(super) const OPERATION: &str = "core.bytes.count";

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    let ([coordinate], [operand, CoreExpr::Const(CoreValue::Bytes(needle))]) = (type_args, args)
    else {
        return None;
    };
    if needle.len() != 1
        || !matches!(
            resolved_core_type(core, coordinate),
            Some(CoreType::Bytes { .. })
        )
        || expression_type_coordinate(core, functions, operand, available).as_ref()
            != Some(coordinate)
    {
        return None;
    }
    Some("U64".into())
}

/// A count never exceeds the operand's byte maximum. Only structural operand
/// coordinates carry that maximum without the module's type table, so named
/// operands fall back to the caller's conservative width bound.
pub(super) fn result_bound(type_args: &[String]) -> Option<u64> {
    let [coordinate] = type_args else { return None };
    match classify_core_type_reference(coordinate)? {
        CoreTypeReference::Structural(CoreType::Bytes { max, .. }) => Some(max),
        _ => None,
    }
}
