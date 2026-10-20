//! Target-owned totality check over untrusted Core, independent of the compiler proof.
use super::{
    expression_type_coordinate, BTreeMap, CoreExpr, CoreModule, CorePredicate, CoreValue, LocalRef,
    TargetPureFunctionFact,
};
use crate::core_ir::CompareOp;

pub(super) const OPERATION: &str = "core.integer.subtract";

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    #[cfg(test)]
    tests::VALIDATION_VISITS.with(|count| count.set(count.get() + 1));
    let [width] = type_args else { return None };
    let [left, right] = args else { return None };
    if !matches!(width.as_str(), "U32" | "U64") {
        return None;
    }
    [left, right]
        .iter()
        .all(|operand| {
            expression_type_coordinate(core, functions, operand, available).as_ref() == Some(width)
        })
        .then(|| width.clone())
}

pub(super) fn proven_order(
    left: &CoreExpr,
    right: &CoreExpr,
    constraints: &[&CorePredicate],
) -> bool {
    left == right
        || constant(right) == Some(0)
        || matches!((constant(left), constant(right)), (Some(a), Some(b)) if a >= b)
        || constraints
            .iter()
            .any(|constraint| proves_order(constraint, left, right))
}

fn constant(value: &CoreExpr) -> Option<u64> {
    let CoreExpr::Const(CoreValue::Int { width, value }) = value else {
        return None;
    };
    match width.as_str() {
        "U32" => value.parse::<u32>().ok().map(u64::from),
        "U64" => value.parse().ok(),
        _ => None,
    }
}

fn proves_order(predicate: &CorePredicate, minuend: &CoreExpr, subtrahend: &CoreExpr) -> bool {
    match predicate {
        CorePredicate::All(items) => items
            .iter()
            .any(|item| proves_order(item, minuend, subtrahend)),
        CorePredicate::Compare { op, left, right } => match op {
            CompareOp::Lt | CompareOp::Le => left == subtrahend && right == minuend,
            CompareOp::Gt | CompareOp::Ge => left == minuend && right == subtrahend,
            CompareOp::Eq => {
                (left == minuend && right == subtrahend) || (left == subtrahend && right == minuend)
            }
            CompareOp::Ne => false,
        },
        CorePredicate::True
        | CorePredicate::False
        | CorePredicate::Not(_)
        | CorePredicate::Any(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    std::thread_local! {
        pub(super) static VALIDATION_VISITS: Cell<usize> = const { Cell::new(0) };
    }

    fn integer(value: &str) -> CoreExpr {
        CoreExpr::Const(CoreValue::Int {
            width: "U64".to_owned(),
            value: value.to_owned(),
        })
    }

    fn assert_bounded_validation(wrap: fn(CoreExpr) -> CoreExpr) {
        let core = CoreModule {
            functions: BTreeMap::new(),
            api_version: crate::core_ir::CORE_API_VERSION.to_owned(),
            coordinate: "arithmetic.work@1".to_owned(),
            imports: Vec::new(),
            types: BTreeMap::new(),
            intents: BTreeMap::new(),
            required_core_capabilities: Vec::new(),
        };
        let depth = 8;
        let expression = (0..depth).fold(integer("9"), |left, _| {
            wrap(CoreExpr::Call {
                callee: OPERATION.to_owned(),
                type_args: vec!["U64".to_owned()],
                args: vec![left, integer("0")],
            })
        });
        VALIDATION_VISITS.with(|count| count.set(0));
        assert_eq!(
            expression_type_coordinate(&core, &[], &expression, &BTreeMap::new()),
            Some("U64".to_owned())
        );
        let visits = VALIDATION_VISITS.with(Cell::get);
        println!("{depth} subtraction nodes required {visits} validation visits");
        assert!(
            visits <= 4 * depth,
            "{depth} subtraction nodes required {visits} validation visits"
        );
    }

    #[test]
    fn nested_subtraction_validation_work_is_bounded() {
        assert_bounded_validation(std::convert::identity);
    }

    #[test]
    fn conditional_subtraction_validation_work_is_bounded() {
        assert_bounded_validation(|value| CoreExpr::If {
            predicate: Box::new(CorePredicate::True),
            then_value: Box::new(value),
            else_value: Box::new(integer("9")),
        });
    }

    #[test]
    fn record_field_subtraction_validation_work_is_bounded() {
        assert_bounded_validation(|value| CoreExpr::Field {
            base: Box::new(CoreExpr::Record {
                fields: BTreeMap::from([("value".to_owned(), value)]),
            }),
            field: "value".to_owned(),
        });
    }
}
