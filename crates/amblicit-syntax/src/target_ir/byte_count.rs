//! Independently validate a single-byte count at the untrusted Core boundary.
use super::{
    expression_type_coordinate, resolved_core_type, BTreeMap, CoreExpr, CoreModule, CoreType,
    CoreValue, LocalRef, TargetPureFunctionFact,
};

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

/// A count never exceeds its operand's byte maximum, resolved through the
/// module's type table so named and imported byte coordinates keep it.
pub(super) fn result_bound(core: &CoreModule, type_args: &[String]) -> Option<u64> {
    let [coordinate] = type_args else { return None };
    match resolved_core_type(core, coordinate)? {
        CoreType::Bytes { max, .. } => Some(max),
        _ => None,
    }
}
