//! Target-owned Option signatures and presence proofs; source refinement is not trusted.
use super::{
    core_type_fits, expression_fits_declared_type, expression_type_coordinate, resolved_core_type,
    BTreeMap, CompareOp, CoreExpr, CoreModule, CorePredicate, CoreType, CoreValue, LocalRef,
    TargetPureFunctionFact,
};
use crate::core_ir::render_self_describing_core_type;

/// Core capability that every `core.option.*` call requires.
pub(super) const CAPABILITY: &str = "core.option/v1";
const IS_SOME: &str = "core.option.isSome";
const UNWRAP: &str = "core.option.unwrap";

pub(super) fn is_operation(callee: &str) -> bool {
    matches!(
        callee,
        "core.option.some"
            | "core.option.none"
            | "core.option.isSome"
            | "core.option.default"
            | "core.option.unwrap"
    )
}

/// The result coordinate Target derives for an Option call, or `None` when
/// its capability, item type argument or operands do not check.
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
        .any(|capability| capability == CAPABILITY)
    {
        return None;
    }
    let [item] = type_args else {
        return None;
    };
    resolved_core_type(core, item)?;
    let option = render_self_describing_core_type(&CoreType::Option { item: item.clone() })?;
    let operand_is_option =
        |operand: &CoreExpr| has_option_item(core, functions, operand, item, available);
    let fits =
        |value: &CoreExpr| expression_fits_declared_type(core, functions, value, item, available);
    match (callee, args) {
        ("core.option.some", [value]) => fits(value).then_some(option),
        ("core.option.none", []) => Some(option),
        ("core.option.isSome", [operand]) => operand_is_option(operand).then(|| "Bool".to_owned()),
        ("core.option.unwrap", [operand]) => operand_is_option(operand).then(|| item.clone()),
        ("core.option.default", [operand, fallback]) => {
            (operand_is_option(operand) && fits(fallback)).then(|| item.clone())
        }
        _ => None,
    }
}

/// The operand is independently an `Option` whose item is exactly `item`.
fn has_option_item(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    operand: &CoreExpr,
    item: &str,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    let Some(coordinate) = expression_type_coordinate(core, functions, operand, available) else {
        return false;
    };
    let Some(CoreType::Option { item: actual }) = resolved_core_type(core, &coordinate) else {
        return false;
    };
    core_type_fits(core, &actual, item) && core_type_fits(core, item, &actual)
}

/// `unwrap` is total only on a local that a dominating fact proves present.
pub(super) fn is_total(callee: &str, args: &[CoreExpr], constraints: &[&CorePredicate]) -> bool {
    if callee != UNWRAP {
        return true;
    }
    let [CoreExpr::Local { reference }] = args else {
        return false;
    };
    constraints
        .iter()
        .any(|predicate| establishes(predicate, true, reference))
}

fn establishes(predicate: &CorePredicate, holds: bool, reference: &LocalRef) -> bool {
    match predicate {
        CorePredicate::Compare {
            op,
            left: CoreExpr::Call { callee, args, .. },
            right: CoreExpr::Const(CoreValue::Bool(value)),
        } if callee == IS_SOME
            && matches!(args.as_slice(), [CoreExpr::Local { reference: tested }] if tested == reference) =>
        {
            match op {
                CompareOp::Eq => holds == *value,
                CompareOp::Ne => holds != *value,
                _ => false,
            }
        }
        CorePredicate::Not(inner) => establishes(inner, !holds, reference),
        CorePredicate::All(items) if holds => {
            items.iter().any(|item| establishes(item, true, reference))
        }
        CorePredicate::Any(items) if !holds => {
            items.iter().any(|item| establishes(item, false, reference))
        }
        _ => false,
    }
}

/// The presence facts a statement branch predicate gives its `then` block.
///
/// Only `isSome` comparisons are retained, so byte, list and arithmetic
/// proofs keep their existing rule that statement branches prove nothing.
pub(super) fn then_block_presence(predicate: &CorePredicate) -> Option<CorePredicate> {
    let mut facts = Vec::new();
    collect_presence(predicate, &mut facts);
    (!facts.is_empty()).then_some(CorePredicate::All(facts))
}

fn collect_presence(predicate: &CorePredicate, facts: &mut Vec<CorePredicate>) {
    match predicate {
        CorePredicate::All(items) => {
            for item in items {
                collect_presence(item, facts);
            }
        }
        CorePredicate::Compare {
            left: CoreExpr::Call { callee, .. },
            ..
        } if callee == IS_SOME => facts.push(predicate.clone()),
        _ => {}
    }
}
