//! Independently validate a byte-length primitive at the untrusted Core boundary.
use super::{
    expression_type_coordinate, resolved_core_type, BTreeMap, CoreExpr, CoreModule, CoreType,
    LocalRef, TargetPureFunctionFact,
};

pub(super) const OPERATION: &str = "core.bytes.length";

pub(super) fn type_coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    type_args: &[String],
    args: &[CoreExpr],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    #[cfg(test)]
    tests::VALIDATION_VISITS.with(|count| count.set(count.get() + 1));
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
    Some("U64".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::{CompareOp, CorePredicate, CoreValue};
    use std::cell::Cell;

    std::thread_local! {
        pub(super) static VALIDATION_VISITS: Cell<usize> = const { Cell::new(0) };
    }

    #[test]
    fn nested_byte_length_validation_work_is_bounded() {
        let core = CoreModule {
            functions: BTreeMap::new(),
            api_version: crate::core_ir::CORE_API_VERSION.to_owned(),
            coordinate: "length.work@1".to_owned(),
            imports: Vec::new(),
            types: BTreeMap::new(),
            intents: BTreeMap::new(),
            required_core_capabilities: Vec::new(),
        };
        let bytes = || CoreExpr::Const(CoreValue::Bytes(vec![7]));
        let length = |operand| CoreExpr::Call {
            callee: OPERATION.to_owned(),
            type_args: vec!["Bytes<exact=1>".to_owned()],
            args: vec![operand],
        };
        let depth = 8;
        let mut operand = bytes();
        for _ in 1..depth {
            operand = CoreExpr::If {
                predicate: Box::new(CorePredicate::Compare {
                    op: CompareOp::Ge,
                    left: length(operand),
                    right: CoreExpr::Const(CoreValue::Int {
                        width: "U64".to_owned(),
                        value: "0".to_owned(),
                    }),
                }),
                then_value: Box::new(bytes()),
                else_value: Box::new(bytes()),
            };
        }
        VALIDATION_VISITS.with(|count| count.set(0));
        assert_eq!(
            expression_type_coordinate(&core, &[], &length(operand), &BTreeMap::new()),
            Some("U64".to_owned())
        );
        let visits = VALIDATION_VISITS.with(Cell::get);
        println!("{depth} byte-length nodes required {visits} validation visits");
        assert!(
            visits <= 4 * depth,
            "validation work exceeded its linear allowance"
        );
    }
}
