//! Independently validate a UTF-8 predicate at the untrusted Core boundary.
use super::{
    expression_type_coordinate, resolved_core_type, BTreeMap, CoreExpr, CoreModule, CoreType,
    LocalRef, TargetPureFunctionFact,
};

pub(super) const OPERATION: &str = "core.bytes.isUtf8";

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    let ([coordinate], [operand]) = (type_args, args) else {
        return None;
    };
    if !matches!(
        resolved_core_type(core, coordinate),
        Some(CoreType::Bytes { .. })
    ) || expression_type_coordinate(core, functions, operand, available).as_ref()
        != Some(coordinate)
    {
        return None;
    }
    Some("Bool".into())
}
