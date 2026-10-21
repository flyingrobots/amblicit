//! Evaluation-scope traversal for partial Core operations at the Target boundary.
use super::{
    byte_slice, unsigned_addition, unsigned_subtraction, CoreExpr, CoreIntent, CoreNode,
    CorePredicate, CoreRequireFailureArm, InputConstraint,
};
use crate::core_ir::{CoreBlock, MAX_CORE_GRAPH_DEPTH};

pub(super) fn intent_is_total(intent: &CoreIntent) -> bool {
    intent
        .basis
        .as_ref()
        .is_none_or(|basis| expression_is_total(basis, &[]))
        && intent
            .input_constraints
            .iter()
            .all(|constraint| predicate_is_total(&constraint.predicate, &[]))
        && block_is_total(&intent.body, &intent.input_constraints)
}

fn block_is_total(block: &CoreBlock, constraints: &[InputConstraint]) -> bool {
    block_is_total_at_depth(
        block,
        &constraints
            .iter()
            .map(|constraint| &constraint.predicate)
            .collect::<Vec<_>>(),
        0,
    )
}

fn block_is_total_at_depth(
    block: &CoreBlock,
    constraints: &[&CorePredicate],
    depth: usize,
) -> bool {
    if depth > MAX_CORE_GRAPH_DEPTH {
        return false;
    }
    block.nodes.iter().all(|node| match node {
        CoreNode::Let { value, .. } => expression_is_total_at_depth(value, constraints, depth),
        CoreNode::Require { predicate, arm } => {
            let (CoreRequireFailureArm::Terminal { reason }
            | CoreRequireFailureArm::ContinueObstructed { reason }) = arm;
            predicate_is_total_at_depth(predicate, constraints, depth)
                && reason
                    .payload
                    .values()
                    .all(|value| expression_is_total_at_depth(value, constraints, depth))
        }
        CoreNode::Effect {
            input,
            obstruction_map,
            ..
        } => {
            expression_is_total_at_depth(input, constraints, depth)
                && obstruction_map
                    .values()
                    .all(|arm| expression_is_total_at_depth(&arm.value, constraints, depth))
        }
        CoreNode::ExternalActionRequest {
            input,
            authority_scope,
            basis,
            budget,
            ..
        } => [
            input,
            authority_scope,
            basis,
            &budget.max_settlement_bytes,
            &budget.max_attempts,
        ]
        .iter()
        .all(|value| expression_is_total_at_depth(value, constraints, depth)),
        CoreNode::For { iter, body, .. } => {
            expression_is_total_at_depth(iter, constraints, depth)
                && block_is_total_at_depth(body, constraints, depth + 1)
        }
        CoreNode::Branch {
            predicate,
            then_block,
            else_block,
            ..
        } => {
            predicate_is_total_at_depth(predicate, constraints, depth)
                && block_is_total_at_depth(then_block, constraints, depth + 1)
                && block_is_total_at_depth(else_block, constraints, depth + 1)
        }
    }) && expression_is_total_at_depth(&block.result, constraints, depth)
}

fn predicate_is_total(predicate: &CorePredicate, constraints: &[InputConstraint]) -> bool {
    predicate_is_total_at_depth(
        predicate,
        &constraints
            .iter()
            .map(|constraint| &constraint.predicate)
            .collect::<Vec<_>>(),
        0,
    )
}

fn predicate_is_total_at_depth(
    predicate: &CorePredicate,
    constraints: &[&CorePredicate],
    depth: usize,
) -> bool {
    if depth > MAX_CORE_GRAPH_DEPTH {
        return false;
    }
    match predicate {
        CorePredicate::True | CorePredicate::False => true,
        CorePredicate::Not(inner) => predicate_is_total_at_depth(inner, constraints, depth + 1),
        CorePredicate::All(items) => {
            let mut prefix = constraints.to_vec();
            for item in items {
                if !predicate_is_total_at_depth(item, &prefix, depth + 1) {
                    return false;
                }
                prefix.push(item);
            }
            true
        }
        CorePredicate::Any(items) => items
            .iter()
            .all(|item| predicate_is_total_at_depth(item, constraints, depth + 1)),
        CorePredicate::Compare { left, right, .. } => {
            expression_is_total_at_depth(left, constraints, depth + 1)
                && expression_is_total_at_depth(right, constraints, depth + 1)
        }
    }
}

pub(super) fn expression_is_total(expression: &CoreExpr, constraints: &[InputConstraint]) -> bool {
    expression_is_total_at_depth(
        expression,
        &constraints
            .iter()
            .map(|constraint| &constraint.predicate)
            .collect::<Vec<_>>(),
        0,
    )
}

fn expression_is_total_at_depth(
    expression: &CoreExpr,
    constraints: &[&CorePredicate],
    depth: usize,
) -> bool {
    if depth > MAX_CORE_GRAPH_DEPTH {
        return false;
    }
    match expression {
        CoreExpr::Local { .. } | CoreExpr::Const(_) => true,
        CoreExpr::Field { base, .. } => expression_is_total_at_depth(base, constraints, depth + 1),
        CoreExpr::Record { fields } => fields
            .values()
            .all(|value| expression_is_total_at_depth(value, constraints, depth + 1)),
        CoreExpr::List { values } => values
            .iter()
            .all(|value| expression_is_total_at_depth(value, constraints, depth + 1)),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            predicate_is_total_at_depth(predicate, constraints, depth + 1)
                && expression_is_total_at_depth(then_value, constraints, depth + 1)
                && expression_is_total_at_depth(else_value, constraints, depth + 1)
        }
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } => {
            if !args
                .iter()
                .all(|arg| expression_is_total_at_depth(arg, constraints, depth + 1))
            {
                return false;
            }
            match callee.as_str() {
                unsigned_addition::OPERATION => {
                    unsigned_addition::is_total(type_args, args, constraints)
                }
                unsigned_subtraction::OPERATION => {
                    let [left, right] = args.as_slice() else {
                        return false;
                    };
                    unsigned_subtraction::proven_order(left, right, constraints)
                }
                byte_slice::OPERATION => byte_slice::is_total(type_args, args, constraints),
                _ => true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{block_is_total, expression_is_total};
    use crate::core_ir::{CompareOp, CoreBlock, CoreExpr, CoreNode, CorePredicate, CoreValue};

    fn leaf() -> CoreExpr {
        CoreExpr::Const(CoreValue::Bool(true))
    }

    fn fields(depth: usize) -> CoreExpr {
        (0..depth).fold(leaf(), |base, _| CoreExpr::Field {
            base: Box::new(base),
            field: "value".into(),
        })
    }

    #[test]
    fn totality_expression_depth_has_an_exact_boundary() {
        assert!(expression_is_total(&fields(128), &[]));
        assert!(!expression_is_total(&fields(129), &[]));
    }

    #[test]
    fn totality_mixed_depth_does_not_reset_at_predicates() {
        let mixed = (0..64).fold(leaf(), |left, _| CoreExpr::If {
            predicate: Box::new(CorePredicate::Compare {
                op: CompareOp::Eq,
                left,
                right: leaf(),
            }),
            then_value: Box::new(leaf()),
            else_value: Box::new(leaf()),
        });
        assert!(expression_is_total(&mixed, &[]));
        assert!(!expression_is_total(
            &CoreExpr::Field {
                base: Box::new(mixed),
                field: "value".into()
            },
            &[]
        ));
    }

    fn block() -> CoreBlock {
        CoreBlock {
            locals: Vec::new(),
            nodes: Vec::new(),
            result: leaf(),
        }
    }

    #[test]
    fn totality_block_depth_has_an_exact_boundary() {
        for (blocks, expressions) in [(128, 0), (64, 64)] {
            let nested = (0..blocks).fold(
                CoreBlock {
                    result: fields(expressions),
                    ..block()
                },
                |then_block, _| CoreBlock {
                    nodes: vec![CoreNode::Branch {
                        binding: None,
                        predicate: CorePredicate::True,
                        then_block,
                        else_block: block(),
                    }],
                    ..block()
                },
            );
            assert!(block_is_total(&nested, &[]));
            let beyond = CoreBlock {
                nodes: vec![CoreNode::Branch {
                    binding: None,
                    predicate: CorePredicate::True,
                    then_block: nested,
                    else_block: block(),
                }],
                ..block()
            };
            assert!(!block_is_total(&beyond, &[]));
        }
    }
}
