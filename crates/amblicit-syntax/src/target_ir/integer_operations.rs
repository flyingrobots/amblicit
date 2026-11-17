//! Target-owned signatures for checked integer operations and `min`/`max`.
//!
//! Target derives each result type from the width type argument and the
//! independently typed operands; it does not trust source checking. Checked
//! operations are total (their failure is the `none` case) and need
//! `core.option/v1`; `min` and `max` are total and return the width.
use super::{
    expression_type_coordinate, BTreeMap, CoreExpr, CoreModule, LocalRef, TargetPureFunctionFact,
};
use crate::core_ir::{render_self_describing_core_type, CoreType};

const CHECKED: &[&str] = &[
    "core.integer.checkedAdd",
    "core.integer.checkedSub",
    "core.integer.checkedMul",
    "core.integer.checkedDiv",
    "core.integer.checkedRem",
];
pub(super) const MIN: &str = "core.integer.min";
pub(super) const MAX: &str = "core.integer.max";

pub(super) fn is_operation(callee: &str) -> bool {
    CHECKED.contains(&callee) || callee == MIN || callee == MAX
}

/// The result coordinate Target derives, or `None` when the width, operand
/// types, arity or capability do not check.
pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    callee: &str,
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    let [width] = type_args else { return None };
    let [left, right] = args else { return None };
    if !matches!(width.as_str(), "U8" | "U32" | "U64") {
        return None;
    }
    let operands_fit = [left, right].iter().all(|operand| {
        expression_type_coordinate(core, functions, operand, available).as_ref() == Some(width)
    });
    if !operands_fit {
        return None;
    }
    if !CHECKED.contains(&callee) {
        return Some(width.clone());
    }
    let capable = core
        .required_core_capabilities
        .iter()
        .any(|capability| capability == super::option_operations::CAPABILITY);
    if !capable {
        return None;
    }
    render_self_describing_core_type(&CoreType::Option {
        item: width.clone(),
    })
}
