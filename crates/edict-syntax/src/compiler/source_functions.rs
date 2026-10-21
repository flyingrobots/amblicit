//! First-order source functions keep executable authority in the source module.
use std::collections::BTreeMap;

use super::{
    compare_op, compatible, error, expr_span, expression_identity, next_local, BinOp,
    CompilerErrorKind, CompilerStage, CoreExpr, CoreFunction, CorePureBinding, CorePureBlock, Expr,
    FunctionDecl, HelperCost, LetStatement, LocalEnvironment, LocalRef, Span, Stmt, TypeChecker,
    TypeKind, TypeRef, TypeShape, TypedValue, UnOp,
};

const VALUE_CELL_BYTES: u64 = 64;

#[derive(Debug, Clone)]
pub(super) struct Signature {
    params: Vec<TypeShape>,
    result: TypeShape,
}

impl TypeChecker<'_> {
    pub(super) fn check_source_functions(&mut self) -> BTreeMap<String, CoreFunction> {
        for definition in &self.resolved.source_functions {
            let params = definition
                .params
                .iter()
                .map(|param| self.type_ref_shape(&param.ty, param.span))
                .collect::<Option<Vec<_>>>();
            let result = self.type_ref_shape(&definition.returns, definition.span);
            if let (Some(params), Some(result)) = (params, result) {
                let coordinate = format!("{}.{}", self.resolved.coordinate, definition.name);
                if self
                    .resolved
                    .pure_functions
                    .values()
                    .any(|fact| fact.coordinate == coordinate)
                    || self
                        .resolved
                        .effect_signatures
                        .values()
                        .any(|fact| fact.coordinate == coordinate)
                {
                    self.errors.push(error(
                        CompilerStage::Resolve,
                        CompilerErrorKind::UnresolvedFunction,
                        "source function collides with imported authority",
                        definition.span,
                    ));
                } else {
                    self.function_signatures
                        .insert(definition.name.clone(), Signature { params, result });
                }
            }
        }
        let mut functions = BTreeMap::new();
        for definition in &self.resolved.source_functions {
            if let Some(function) = self.check_source_function(definition) {
                functions.insert(definition.name.clone(), function);
            }
        }
        if !self.errors.is_empty() {
            return functions;
        }
        match crate::core_ir::validate_function_graph(&self.resolved.coordinate, &functions) {
            Ok(heights) => {
                // Callees precede callers. Each body occurrence still contributes
                // its cost, unlike the identity set used only for graph height.
                let mut definitions = self.resolved.source_functions.iter().collect::<Vec<_>>();
                definitions.sort_by_key(|definition| {
                    (heights[&definition.name], definition.name.as_str())
                });
                for definition in definitions {
                    if let Some(cost) = self.source_function_cost(definition) {
                        self.function_costs.insert(definition.name.clone(), cost);
                    }
                }
            }
            Err(graph_failure) => {
                let failure = graph_failure.integrity();
                let kind = if failure.kind()
                    == crate::core_ir::CoreTypeIntegrityFailureKind::ReferenceCycle
                {
                    CompilerErrorKind::UnsupportedSourceShape
                } else {
                    CompilerErrorKind::InvalidBound
                };
                let span = graph_failure
                    .function_name()
                    .and_then(|name| {
                        self.resolved
                            .source_functions
                            .iter()
                            .find(|definition| definition.name == name)
                    })
                    .map_or(Span::new(0, 0), |definition| definition.span);
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    kind,
                    failure.to_string(),
                    span,
                ));
            }
        }
        functions
    }

    fn check_source_function(&mut self, definition: &FunctionDecl) -> Option<CoreFunction> {
        let signature = self.function_signatures.get(&definition.name)?.clone();
        self.input_proof_constraints.clear();
        self.local_bounds.clear();
        let mut env = LocalEnvironment::default();
        let params = definition
            .params
            .iter()
            .zip(&signature.params)
            .enumerate()
            .map(|(index, (param, shape))| {
                let local = LocalRef {
                    id: format!("arg.{index}"),
                    alpha_name: format!("$arg{index}"),
                    ty: shape.coord.clone(),
                };
                env.insert(param.name.clone(), (local.clone(), shape.clone()));
                local
            })
            .collect();
        let mut locals = Vec::new();
        let mut bindings = Vec::new();
        let mut result = None;
        let mut saw_return = false;
        let errors_before = self.errors.len();
        let mut local_index = 0;
        for statement in &definition.body.stmts {
            if saw_return {
                self.unsupported_stmt(statement_span(statement), "statement after function return");
                break;
            }
            match statement {
                Stmt::Let {
                    name,
                    ty,
                    value,
                    els: None,
                    span,
                } => {
                    let stmt = LetStatement {
                        name,
                        ty: ty.as_ref(),
                        value,
                        handler: None,
                        span: *span,
                    };
                    let Some((value_checked, shape)) = self.check_source_let(&stmt, &env) else {
                        env.poison(name);
                        continue;
                    };
                    let local = next_local(&mut local_index, shape.coord.clone());
                    self.record_binding_bound(&local, &value_checked.expr);
                    bindings.push(CorePureBinding {
                        binding: local.clone(),
                        value: value_checked.expr,
                    });
                    locals.push(local.clone());
                    env.insert(name.clone(), (local, shape));
                }
                Stmt::Return { value, span } => {
                    saw_return = true;
                    result = self.check_source_return(value, &env, &signature.result, *span);
                }
                _ => {
                    self.unsupported_stmt(
                        statement_span(statement),
                        "statement in a pure function",
                    );
                    return None;
                }
            }
        }
        if !saw_return {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "function must end in a return",
                definition.span,
            ));
        }
        if self.errors.len() != errors_before {
            return None;
        }
        let result = result?;
        Some(CoreFunction {
            params,
            return_type: signature.result.coord,
            body: CorePureBlock {
                locals,
                bindings,
                result,
            },
        })
    }

    fn check_source_let(
        &mut self,
        stmt: &LetStatement<'_>,
        env: &LocalEnvironment,
    ) -> Option<(TypedValue, TypeShape)> {
        let annotation = match stmt.ty {
            Some(ty) => {
                let Some(shape) = self.type_ref_shape(ty, stmt.span) else {
                    self.check_expr_with_unavailable_expected_type(stmt.value, env);
                    return None;
                };
                Some(shape)
            }
            None => None,
        };
        let value = self.check_expr_with_expected(stmt.value, env, annotation.as_ref())?;
        let shape = self.pure_let_binding_shape(stmt, &value, annotation)?;
        Some((value, shape))
    }

    fn check_source_return(
        &mut self,
        value: &Expr,
        env: &LocalEnvironment,
        expected: &TypeShape,
        span: Span,
    ) -> Option<CoreExpr> {
        let checked = self.check_expr_with_expected(value, env, Some(expected))?;
        if !compatible(expected, &checked.ty) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "function result does not match declared return type",
                span,
            ));
            return None;
        }
        Some(checked.expr)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_source_call(
        &mut self,
        name: &str,
        signature: &Signature,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "source functions are nongeneric",
                span,
            ));
            return None;
        }
        if args.len() != signature.params.len() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "source function argument count does not match signature",
                span,
            ));
        }
        let mut arguments = Vec::new();
        let mut accepted = args.len() == signature.params.len();
        for (argument, parameter) in args.iter().zip(&signature.params) {
            let Some(value) = self.check_expr_with_expected(argument, env, Some(parameter)) else {
                accepted = false;
                continue;
            };
            if !compatible(parameter, &value.ty) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    "source function argument type does not match signature",
                    expr_span(argument),
                ));
                accepted = false;
                continue;
            }
            arguments.push(value.expr);
        }
        for argument in args.iter().skip(signature.params.len()) {
            self.check_expr_with_unavailable_expected_type(argument, env);
        }
        if expected.is_some_and(|expected| !compatible(expected, &signature.result)) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "source function result does not match its use",
                span,
            ));
            return None;
        }
        if !accepted {
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: format!("{}.{}", self.resolved.coordinate, name),
                type_args: Vec::new(),
                args: arguments,
            },
            ty: signature.result.clone(),
        })
    }

    fn source_function_cost(&mut self, definition: &FunctionDecl) -> Option<HelperCost> {
        let nested = self.helper_cost_for_block(&definition.body)?;
        let signature = self.function_signatures[&definition.name].clone();
        let mut own = HelperCost {
            steps: 1,
            ..HelperCost::default()
        };
        for shape in signature
            .params
            .iter()
            .chain(std::iter::once(&signature.result))
        {
            let (steps, _) = self.source_value_bound(shape, definition.span)?;
            own = self.checked_helper_cost_add(
                own,
                HelperCost {
                    steps,
                    ..HelperCost::default()
                },
                definition.span,
            )?;
        }
        self.checked_helper_cost_add(nested, own, definition.span)
    }

    fn source_value_bound(&mut self, shape: &TypeShape, span: Span) -> Option<(u64, u64)> {
        if contains_external_action_request(shape) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "request-bearing values are not supported by source function accounting",
                span,
            ));
            return None;
        }
        value_bound(shape).or_else(|| self.function_bound_failure(span))
    }

    fn function_bound_failure<T>(&mut self, span: Span) -> Option<T> {
        self.errors.push(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::InvalidBound,
            "source function work or value bound overflows",
            span,
        ));
        None
    }

    /// Full bounded operand storage conservatively bounds the UTF-8 and count
    /// scans, as it already does for byte comparison work.
    fn byte_scan_steps(&self, expr: &Expr) -> u64 {
        match expr {
            Expr::Call { callee, args, .. } if is_prelude_call(callee, &["isUtf8", "count"]) => {
                args.first()
                    .and_then(|argument| {
                        self.source_value_bounds.get(&expression_identity(argument))
                    })
                    .map_or(0, |bound| bound.1)
            }
            _ => 0,
        }
    }

    pub(super) fn record_source_expression_cost(
        &mut self,
        expr: &Expr,
        shape: &TypeShape,
    ) -> Option<()> {
        let span = expr_span(expr);
        let (cells, bytes) = self.source_value_bound(shape, span)?;
        self.source_value_bounds
            .insert(expression_identity(expr), (cells, bytes));
        let mut own = HelperCost {
            steps: cells
                .checked_add(1)
                .or_else(|| self.function_bound_failure(span))?,
            allocated_bytes: bytes,
            output_bytes: 0,
        };
        let arguments: Vec<&Expr> = match expr {
            Expr::Binary {
                op: BinOp::Add | BinOp::Sub,
                lhs,
                rhs,
                ..
            } => vec![lhs, rhs],
            Expr::Call { callee, args, .. }
                if is_prelude_call(callee, &["len", "slice", "isUtf8", "count"]) =>
            {
                args.iter().collect()
            }
            _ => Vec::new(),
        };
        // Intrinsics independently validate their operands after evaluating them.
        for argument in arguments {
            let steps = self
                .source_value_bounds
                .get(&expression_identity(argument))
                .map_or(0, |bound| bound.0);
            own = self.checked_helper_cost_add(
                own,
                HelperCost {
                    steps,
                    ..HelperCost::default()
                },
                span,
            )?;
        }
        own = self.checked_helper_cost_add(
            own,
            HelperCost {
                steps: self.byte_scan_steps(expr),
                ..HelperCost::default()
            },
            span,
        )?;
        if matches!(expr, Expr::Binary { op: BinOp::Add, .. })
            || matches!(expr, Expr::Call { callee, .. } if is_prelude_call(callee, &["slice"]))
        {
            let steps = match &shape.kind {
                TypeKind::Bytes { max, .. } => *max,
                TypeKind::String { max, .. } => max
                    .checked_mul(4)
                    .or_else(|| self.function_bound_failure(span))?,
                _ => 0,
            };
            own = self.checked_helper_cost_add(
                own,
                HelperCost {
                    steps,
                    ..HelperCost::default()
                },
                span,
            )?;
        }
        if matches!(
            expr,
            Expr::Binary {
                op: BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or,
                ..
            } | Expr::Unary { op: UnOp::Not, .. }
        ) {
            // Reification is an If plus a selected Boolean constant expression.
            own = self.checked_helper_cost_add(
                own,
                HelperCost {
                    steps: 1,
                    ..HelperCost::default()
                },
                span,
            )?;
        }
        self.source_expression_costs
            .insert(expression_identity(expr), own);
        Some(())
    }

    pub(super) fn record_source_predicate_cost(&mut self, expr: &Expr) -> Option<()> {
        let span = expr_span(expr);
        let mut steps = 1_u64;
        let mut allocated_bytes = 0;
        if let Expr::Binary { op, lhs, rhs, .. } = expr {
            if compare_op(*op).is_some() {
                // Full bounded storage also bounds byte/text comparison work,
                // and is conservative for scalar and aggregate comparisons.
                let work = [lhs.as_ref(), rhs.as_ref()]
                    .into_iter()
                    .filter_map(|value| {
                        self.source_value_bounds
                            .get(&expression_identity(value))
                            .map(|bound| bound.1)
                    })
                    .max()
                    .unwrap_or(0);
                steps = steps
                    .checked_add(work)
                    .or_else(|| self.function_bound_failure(span))?;
            }
        } else if matches!(
            expr,
            Expr::Ident { .. } | Expr::Field { .. } | Expr::Call { .. } | Expr::If { .. }
        ) {
            steps += 2; // The Boolean equality's synthetic true operand.
            allocated_bytes = VALUE_CELL_BYTES;
        }
        self.source_predicate_costs.insert(
            expression_identity(expr),
            HelperCost {
                steps,
                allocated_bytes,
                ..HelperCost::default()
            },
        );
        Some(())
    }

    pub(super) fn source_intent_frame_cost(
        &mut self,
        input: &TypeShape,
        output: &TypeShape,
        span: Span,
    ) -> Option<HelperCost> {
        let (input_cells, input_bytes) = self.source_value_bound(input, span)?;
        let (output_cells, output_bytes) = self.source_value_bound(output, span)?;
        let Some(steps) = input_cells
            .checked_add(output_cells)
            .and_then(|cells| cells.checked_mul(2))
        else {
            return self.function_bound_failure(span);
        };
        let Some(allocated_bytes) = input_bytes.checked_add(output_bytes) else {
            return self.function_bound_failure(span);
        };
        let Some(output_bytes) = encoded_bound(output) else {
            return self.function_bound_failure(span);
        };
        Some(HelperCost {
            steps,
            allocated_bytes,
            output_bytes,
        })
    }
}

fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Let { span, .. }
        | Stmt::Effect { span, .. }
        | Stmt::ExternalActionRequest { span, .. }
        | Stmt::Require { span, .. }
        | Stmt::Guarantee { span, .. }
        | Stmt::Assert { span, .. }
        | Stmt::If { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. } => *span,
    }
}

fn contains_external_action_request(shape: &TypeShape) -> bool {
    match &shape.kind {
        TypeKind::ExternalActionRequest { .. } => true,
        TypeKind::Nominal { representation, .. } => {
            contains_external_action_request(representation)
        }
        TypeKind::List { item, .. } => contains_external_action_request(item),
        TypeKind::Record(fields) => fields.values().any(contains_external_action_request),
        TypeKind::Bool
        | TypeKind::Int { .. }
        | TypeKind::Bytes { .. }
        | TypeKind::String { .. } => false,
    }
}

// Portable conservative value-storage accounting: one 64-byte value cell,
// plus bounded payload and child cells. It does not use host pointer widths.
fn value_bound(shape: &TypeShape) -> Option<(u64, u64)> {
    match &shape.kind {
        TypeKind::Bool | TypeKind::Int { .. } => Some((1, VALUE_CELL_BYTES)),
        TypeKind::Bytes { max, .. } => Some((1, VALUE_CELL_BYTES.checked_add(*max)?)),
        TypeKind::String { max, .. } => {
            Some((1, VALUE_CELL_BYTES.checked_add(max.checked_mul(4)?)?))
        }
        TypeKind::Nominal { representation, .. } => value_bound(representation),
        TypeKind::List { item, max } => {
            let (cells, bytes) = value_bound(item)?;
            Some((
                1_u64.checked_add(cells.checked_mul(*max)?)?,
                VALUE_CELL_BYTES.checked_add(bytes.checked_mul(*max)?)?,
            ))
        }
        TypeKind::Record(fields) => fields.iter().try_fold(
            (1_u64, VALUE_CELL_BYTES),
            |(cells, bytes), (name, shape)| {
                let (child_cells, child_bytes) = value_bound(shape)?;
                Some((
                    cells.checked_add(1)?.checked_add(child_cells)?,
                    bytes
                        .checked_add(VALUE_CELL_BYTES)?
                        .checked_add(u64::try_from(name.len()).ok()?)?
                        .checked_add(child_bytes)?,
                ))
            },
        ),
        TypeKind::ExternalActionRequest { .. } => None,
    }
}

// Definite-length CBOR headers and integer cells use at most nine bytes.
fn encoded_bound(shape: &TypeShape) -> Option<u64> {
    match &shape.kind {
        TypeKind::Bool => Some(1),
        TypeKind::Int { .. } => Some(9),
        TypeKind::Bytes { max, .. } => 9_u64.checked_add(*max),
        TypeKind::String { max, .. } => 9_u64.checked_add(max.checked_mul(4)?),
        TypeKind::Nominal { representation, .. } => encoded_bound(representation),
        TypeKind::List { item, max } => 9_u64.checked_add(encoded_bound(item)?.checked_mul(*max)?),
        TypeKind::Record(fields) => fields.iter().try_fold(9_u64, |sum, (name, shape)| {
            sum.checked_add(9)?
                .checked_add(u64::try_from(name.len()).ok()?)?
                .checked_add(encoded_bound(shape)?)
        }),
        TypeKind::ExternalActionRequest { .. } => None,
    }
}

fn is_prelude_call(callee: &Expr, names: &[&str]) -> bool {
    matches!(callee, Expr::Ident { name, .. } if names.contains(&name.as_str()))
}
