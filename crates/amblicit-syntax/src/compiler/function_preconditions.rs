//! Function `where` preconditions: checked once against the parameters,
//! assumed by the body, and proven again at every call site.
use super::{
    error, expr_span, unsigned_subtraction::proven_order, CompareOp, CompilerErrorKind,
    CompilerStage, CoreExpr, CorePredicate, FunctionDecl, LocalEnvironment, LocalRef, Span,
    TypeChecker,
};
use crate::core_ir::{
    instantiate_precondition, precondition_conjuncts, precondition_nodes,
    MAX_FUNCTION_PRECONDITION_CONJUNCTS, MAX_FUNCTION_PRECONDITION_NODES,
};

impl TypeChecker<'_> {
    /// Compile every function's preconditions in its parameter frame. Each
    /// predicate may assume the predicates before it. Runs after all
    /// signatures exist and before any body, so callers see every contract.
    pub(super) fn check_source_preconditions(&mut self) {
        for definition in &self.resolved.source_functions {
            if definition.preconditions.is_empty() {
                continue;
            }
            let Some(signature) = self.function_signatures.get(&definition.name).cloned() else {
                continue;
            };
            let (env, _) = parameter_frame(definition, &signature.params);
            self.input_proof_constraints.clear();
            let mut preconditions = Vec::new();
            for predicate in &definition.preconditions {
                self.checking_precondition_of = Some(definition.name.clone());
                let checked = self.check_predicate(predicate, &env);
                self.checking_precondition_of = None;
                let Some(checked) = checked else {
                    continue;
                };
                if let Some(reason) = self.unsupported_precondition(&checked) {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        format!("function `where` precondition {reason}"),
                        expr_span(predicate),
                    ));
                    continue;
                }
                self.input_proof_constraints.push(checked.clone());
                preconditions.push(checked);
            }
            self.input_proof_constraints.clear();
            self.check_precondition_bounds(definition, &preconditions);
            if let Some(signature) = self.function_signatures.get_mut(&definition.name) {
                signature.preconditions = preconditions;
            }
        }
    }

    /// Both caps of `EDICT-CORE-FN-PRECONDITION-001`, which bound the work of
    /// re-proving every call: conjuncts (fact scans) and nodes (copies).
    fn check_precondition_bounds(
        &mut self,
        definition: &FunctionDecl,
        preconditions: &[CorePredicate],
    ) {
        let conjuncts = precondition_conjuncts(preconditions);
        let nodes = precondition_nodes(preconditions);
        for (what, count, limit) in [
            ("conjuncts", conjuncts, MAX_FUNCTION_PRECONDITION_CONJUNCTS),
            ("Core nodes", nodes, MAX_FUNCTION_PRECONDITION_NODES),
        ] {
            if count > limit {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::InvalidBound,
                    format!(
                        "preconditions of `{}` hold {count} {what}; at most {limit} are allowed",
                        definition.name
                    ),
                    definition.span,
                ));
            }
        }
    }

    /// A precondition names parameters and pure operations only. It may not
    /// call a source function, fold, or match: none of those could be
    /// instantiated at a call site without evaluating or re-binding anything.
    fn unsupported_precondition(&self, predicate: &CorePredicate) -> Option<&'static str> {
        let prefix = format!("{}.", self.resolved.coordinate);
        let source_call = |callee: &str| {
            callee
                .strip_prefix(&prefix)
                .is_some_and(|name| self.function_signatures.contains_key(name))
        };
        find_in_predicate(predicate, &|expression| match expression {
            CoreExpr::Call { callee, .. } if source_call(callee) => {
                Some("cannot call a source function")
            }
            CoreExpr::Fold { .. } => Some("cannot fold"),
            CoreExpr::Match { .. } => Some("cannot match"),
            // Exhaustive on purpose: a new expression form must decide here.
            CoreExpr::Call { .. }
            | CoreExpr::Local { .. }
            | CoreExpr::Const(_)
            | CoreExpr::Variant { .. }
            | CoreExpr::Record { .. }
            | CoreExpr::List { .. }
            | CoreExpr::Field { .. }
            | CoreExpr::If { .. } => None,
        })
    }

    /// Prove each callee precondition, instantiated with the call's argument
    /// expressions, from the facts that dominate the call. Reports the first
    /// unproven conjunct at the call span.
    pub(super) fn discharge_preconditions(
        &mut self,
        name: &str,
        preconditions: &[CorePredicate],
        params: &[LocalRef],
        arguments: &[CoreExpr],
        span: Span,
    ) -> bool {
        for (index, precondition) in preconditions.iter().enumerate() {
            let obligation = instantiate_precondition(precondition, params, arguments);
            if !discharged(&obligation, &self.input_proof_constraints) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnprovenPrecondition,
                    format!(
                        "call to `{name}` does not establish its precondition where.{index}; \
                         prove it with an intent `where`, an enclosing `if`, an earlier `&&` \
                         operand, or the caller's own `where`"
                    ),
                    span,
                ));
                return false;
            }
        }
        true
    }
}

/// The parameter locals and lexical frame a function body and its
/// preconditions share.
pub(super) fn parameter_frame(
    definition: &FunctionDecl,
    shapes: &[super::TypeShape],
) -> (LocalEnvironment, Vec<LocalRef>) {
    let mut env = LocalEnvironment::default();
    let params = parameter_locals(shapes);
    for ((param, shape), local) in definition.params.iter().zip(shapes).zip(&params) {
        env.insert(param.name.clone(), (local.clone(), shape.clone()));
    }
    (env, params)
}

/// The positional `arg.N` locals of a source function, in parameter order.
pub(super) fn parameter_locals(shapes: &[super::TypeShape]) -> Vec<LocalRef> {
    shapes
        .iter()
        .enumerate()
        .map(|(index, shape)| LocalRef {
            id: format!("arg.{index}"),
            alpha_name: format!("$arg{index}"),
            ty: shape.coord.clone(),
        })
        .collect()
}

/// Whether the facts entail an instantiated obligation. An order (`<=`, `>=`)
/// uses the same proof as subtraction and slicing; any other comparison or
/// connective must appear as a fact conjunct, up to operand orientation.
fn discharged(obligation: &CorePredicate, facts: &[CorePredicate]) -> bool {
    match obligation {
        CorePredicate::True => true,
        CorePredicate::All(items) => items.iter().all(|item| discharged(item, facts)),
        CorePredicate::Compare {
            op: CompareOp::Le,
            left,
            right,
        } => proven_order(right, left, facts),
        CorePredicate::Compare {
            op: CompareOp::Ge,
            left,
            right,
        } => proven_order(left, right, facts),
        _ => facts.iter().any(|fact| has_conjunct(fact, obligation)),
    }
}

fn has_conjunct(fact: &CorePredicate, wanted: &CorePredicate) -> bool {
    same_fact(fact, wanted)
        || matches!(fact, CorePredicate::All(items) if items.iter().any(|item| has_conjunct(item, wanted)))
}

fn same_fact(fact: &CorePredicate, wanted: &CorePredicate) -> bool {
    if fact == wanted {
        return true;
    }
    let (
        CorePredicate::Compare {
            op: fact_op,
            left: fact_left,
            right: fact_right,
        },
        CorePredicate::Compare { op, left, right },
    ) = (fact, wanted)
    else {
        return false;
    };
    let flipped = match op {
        CompareOp::Lt => CompareOp::Gt,
        CompareOp::Gt => CompareOp::Lt,
        CompareOp::Le => CompareOp::Ge,
        CompareOp::Ge => CompareOp::Le,
        CompareOp::Eq => CompareOp::Eq,
        CompareOp::Ne => CompareOp::Ne,
    };
    *fact_op == flipped && fact_left == right && fact_right == left
}

fn find_in_predicate(
    predicate: &CorePredicate,
    visit: &impl Fn(&CoreExpr) -> Option<&'static str>,
) -> Option<&'static str> {
    match predicate {
        CorePredicate::True | CorePredicate::False => None,
        CorePredicate::Not(inner) => find_in_predicate(inner, visit),
        CorePredicate::All(items) | CorePredicate::Any(items) => {
            items.iter().find_map(|item| find_in_predicate(item, visit))
        }
        CorePredicate::Compare { left, right, .. } => {
            find_in_expression(left, visit).or_else(|| find_in_expression(right, visit))
        }
    }
}

fn find_in_expression(
    expression: &CoreExpr,
    visit: &impl Fn(&CoreExpr) -> Option<&'static str>,
) -> Option<&'static str> {
    if let Some(found) = visit(expression) {
        return Some(found);
    }
    match expression {
        CoreExpr::Local { .. } | CoreExpr::Const(_) | CoreExpr::Fold { .. } => None,
        CoreExpr::Match { scrutinee, .. }
        | CoreExpr::Field {
            base: scrutinee, ..
        } => find_in_expression(scrutinee, visit),
        CoreExpr::Variant { payload, .. } => payload
            .as_deref()
            .and_then(|payload| find_in_expression(payload, visit)),
        CoreExpr::Record { fields } => fields
            .values()
            .find_map(|value| find_in_expression(value, visit)),
        CoreExpr::List { values } | CoreExpr::Call { args: values, .. } => values
            .iter()
            .find_map(|value| find_in_expression(value, visit)),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => find_in_predicate(predicate, visit)
            .or_else(|| find_in_expression(then_value, visit))
            .or_else(|| find_in_expression(else_value, visit)),
    }
}
