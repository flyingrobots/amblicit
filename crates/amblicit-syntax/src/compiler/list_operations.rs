//! Immutable worklist primitives, with explicit source proofs for partial domains.
use super::{
    compatible, error, integer_shape, CompareOp, CompilerErrorKind, CompilerStage, CoreExpr,
    CorePredicate, CoreValue, Expr, LocalEnvironment, Span, TypeChecker, TypeKind, TypeRef,
    TypeShape, TypedValue,
};

pub(super) const NAMES: &[&str] = &["listLen", "listAt", "listAppend", "listConcat"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkPass {
    Discover,
    Meter,
}

impl TypeChecker<'_> {
    pub(super) fn check_list_operation(
        &mut self,
        name: &str,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<TypedValue> {
        let arity = if name == "listLen" { 1 } else { 2 };
        if !type_args.is_empty() || args.len() != arity {
            return self.list_error(
                CompilerErrorKind::UnsupportedSourceShape,
                "list operation has invalid arity or authored type arguments",
                span,
            );
        }
        let list = self.check_expr(&args[0], env)?;
        let TypeKind::List { item, max } = &list.ty.kind else {
            return self.list_error(
                CompilerErrorKind::TypeMismatch,
                "list operation requires a bounded structural List",
                span,
            );
        };
        let coordinate = list.ty.value_type_coord();
        let length = CoreExpr::Call {
            callee: "core.list.length".into(),
            type_args: vec![coordinate.clone()],
            args: vec![list.expr.clone()],
        };
        let mut coordinates = vec![coordinate];
        let mut operands = vec![list.expr.clone()];
        let (operation, result) = match name {
            "listLen" => ("length", integer_shape("U64")),
            "listAt" => {
                let index_type = integer_shape("U64");
                let index = self.check_expr_with_expected(&args[1], env, Some(&index_type))?;
                if index.ty != index_type {
                    return self.list_error(
                        CompilerErrorKind::TypeMismatch,
                        "listAt index must have type U64",
                        span,
                    );
                }
                if !self
                    .input_proof_constraints
                    .iter()
                    .any(|p| proves(p, true, &index.expr, &length))
                {
                    return self.list_error(
                        CompilerErrorKind::UnsupportedSourceShape,
                        "listAt requires proof of index < listLen(list) before evaluation",
                        span,
                    );
                }
                operands.push(index.expr);
                ("at", item.as_ref().clone())
            }
            "listAppend" => {
                let value = self.check_expr_with_expected(&args[1], env, Some(item))?;
                if !compatible(item, &value.ty) {
                    return self.list_error(
                        CompilerErrorKind::TypeMismatch,
                        "listAppend element does not fit the list item type",
                        span,
                    );
                }
                let capacity = CoreExpr::Const(CoreValue::Int {
                    width: "U64".into(),
                    value: max.to_string(),
                });
                if !self
                    .input_proof_constraints
                    .iter()
                    .any(|p| proves(p, true, &length, &capacity))
                {
                    return self.list_error(
                        CompilerErrorKind::UnsupportedSourceShape,
                        "listAppend requires proof of listLen(list) < declared capacity",
                        span,
                    );
                }
                operands.push(value.expr);
                ("append", list.ty.clone())
            }
            "listConcat" => {
                let (right, result) = self.check_list_concat(&args[1], item, *max, env, span)?;
                coordinates.push(right.ty.value_type_coord());
                operands.push(right.expr);
                ("concat", result)
            }
            _ => return None,
        };
        self.core_capabilities.insert("core.list/v1".into());
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: format!("core.list.{operation}"),
                type_args: coordinates,
                args: operands,
            },
            ty: result,
        })
    }

    fn check_list_concat(
        &mut self,
        right: &Expr,
        item: &TypeShape,
        max: u64,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<(TypedValue, TypeShape)> {
        let right = self.check_expr(right, env)?;
        let TypeKind::List {
            item: right_item,
            max: right_max,
        } = &right.ty.kind
        else {
            return self.list_error(
                CompilerErrorKind::TypeMismatch,
                "listConcat requires two bounded lists",
                span,
            );
        };
        if item.value_type_coord() != right_item.value_type_coord() {
            return self.list_error(
                CompilerErrorKind::TypeMismatch,
                "listConcat requires identical item coordinates",
                span,
            );
        }
        let Some(max) = max.checked_add(*right_max) else {
            return self.list_error(
                CompilerErrorKind::InvalidBound,
                "listConcat result bound overflows U64",
                span,
            );
        };
        let result = TypeShape::canonical_structural(TypeKind::List {
            item: Box::new(item.clone()),
            max,
        })?;
        Some((right, result))
    }

    fn list_error<T>(&mut self, kind: CompilerErrorKind, message: &str, span: Span) -> Option<T> {
        self.errors
            .push(error(CompilerStage::TypeCheck, kind, message, span));
        None
    }
}

fn proves(predicate: &CorePredicate, truth: bool, lower: &CoreExpr, upper: &CoreExpr) -> bool {
    match predicate {
        CorePredicate::Not(inner) => proves(inner, !truth, lower, upper),
        CorePredicate::All(items) if truth => items.iter().any(|p| proves(p, true, lower, upper)),
        CorePredicate::Any(items) if !truth => items.iter().any(|p| proves(p, false, lower, upper)),
        CorePredicate::Compare { op, left, right } => match (op, truth) {
            (CompareOp::Lt, true) | (CompareOp::Ge, false) => left == lower && right == upper,
            (CompareOp::Gt, true) | (CompareOp::Le, false) => right == lower && left == upper,
            _ => false,
        },
        _ => false,
    }
}
