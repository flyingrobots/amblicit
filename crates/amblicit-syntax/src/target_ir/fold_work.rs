//! Independent conservative Core work accounting for fold-capable modules.
//!
//! No compiler cost table or artifact-supplied estimate is trusted. Values are
//! charged in portable 64-byte cells; imported work comes only from sealed facts.
use super::{
    expression_type_coordinate, is_typed_prelude_operation, resolved_core_type, CoreBlock,
    CoreBound, CoreBudget, CoreExpr, CoreIntent, CoreModule, CoreNode, CorePredicate,
    CoreRequireFailureArm, CoreType, CoreValue, LocalRef, TargetPureFunctionFact,
    MAX_CORE_GRAPH_DEPTH, MAX_CORE_TYPE_DEPTH,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Default)]
struct Cost {
    steps: u64,
    bytes: u64,
}

impl Cost {
    fn add(self, rhs: Self) -> Option<Self> {
        Some(Self {
            steps: self.steps.checked_add(rhs.steps)?,
            bytes: self.bytes.checked_add(rhs.bytes)?,
        })
    }
    fn times(self, n: u64) -> Option<Self> {
        Some(Self {
            steps: self.steps.checked_mul(n)?,
            bytes: self.bytes.checked_mul(n)?,
        })
    }
    fn maximum(self, rhs: Self) -> Self {
        Self {
            steps: self.steps.max(rhs.steps),
            bytes: self.bytes.max(rhs.bytes),
        }
    }
    fn steps(n: u64) -> Self {
        Self { steps: n, bytes: 0 }
    }
}

#[derive(Clone, Copy)]
struct Value {
    cells: u64,
    bytes: u64,
}
impl Value {
    const CELL: Self = Self {
        cells: 1,
        bytes: 64,
    };
    fn add(self, rhs: Self) -> Option<Self> {
        Some(Self {
            cells: self.cells.checked_add(rhs.cells)?,
            bytes: self.bytes.checked_add(rhs.bytes)?,
        })
    }
    fn maximum(self, rhs: Self) -> Self {
        Self {
            cells: self.cells.max(rhs.cells),
            bytes: self.bytes.max(rhs.bytes),
        }
    }
    fn evaluation(self) -> Option<Cost> {
        Some(Cost {
            steps: self.cells.checked_add(1)?,
            bytes: self.bytes,
        })
    }
}

type Scope<'a> = BTreeMap<&'a str, &'a LocalRef>;
fn scope<'a>(items: impl Iterator<Item = &'a LocalRef>) -> Scope<'a> {
    items.map(|local| (local.id.as_str(), local)).collect()
}

pub(super) fn intent_cost_fits(
    core: &CoreModule,
    facts: &[TargetPureFunctionFact],
    intent: &CoreIntent,
) -> bool {
    if !core
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.fold/v1" || c == "core.list/v1")
    {
        return true;
    }
    let mut meter = Meter {
        core,
        facts,
        visiting: Vec::new(),
        cached: BTreeMap::new(),
    };
    meter.intent(intent).is_some_and(|cost| {
        cost.steps <= intent.core_evaluation_budget.max_steps
            && cost.bytes <= intent.core_evaluation_budget.max_allocated_bytes
    })
}

struct Meter<'a> {
    core: &'a CoreModule,
    facts: &'a [TargetPureFunctionFact],
    visiting: Vec<&'a str>,
    cached: BTreeMap<&'a str, Cost>,
}

impl<'a> Meter<'a> {
    fn value_type(&self, name: &str, depth: usize) -> Option<Value> {
        if depth > MAX_CORE_TYPE_DEPTH {
            return None;
        }
        let child = |ty: &str| self.value_type(ty, depth + 1);
        match resolved_core_type(self.core, name)? {
            CoreType::Bool | CoreType::Unit | CoreType::Int { .. } => Some(Value::CELL),
            CoreType::Bytes { max, .. } => Some(Value {
                cells: 1,
                bytes: 64_u64.checked_add(max)?,
            }),
            CoreType::String { max, .. } => Some(Value {
                cells: 1,
                bytes: 64_u64.checked_add(max.checked_mul(4)?)?,
            }),
            CoreType::Nominal { representation, .. } => child(&representation),
            CoreType::Record { fields } => {
                fields.iter().try_fold(Value::CELL, |total, (key, ty)| {
                    total.add(Self::field(key, child(ty)?)?)
                })
            }
            CoreType::List { item, max } => {
                let item = child(&item)?;
                Value::CELL.add(Value {
                    cells: item.cells.checked_mul(max)?,
                    bytes: item.bytes.checked_mul(max)?,
                })
            }
            CoreType::Variant { cases } => {
                let mut payload = Value::CELL;
                let mut label = 0;
                for (case, ty) in cases {
                    label = label.max(u64::try_from(case.len()).ok()?);
                    payload = payload.maximum(ty.as_deref().map_or(Some(Value::CELL), child)?);
                }
                payload.add(Value {
                    cells: 6,
                    bytes: 399_u64
                        .checked_add(u64::try_from(name.len()).ok()?)?
                        .checked_add(label)?,
                })
            }
            CoreType::Option { .. }
            | CoreType::Map { .. }
            | CoreType::CapabilityRef { .. }
            | CoreType::ExternalActionRequest { .. } => None,
        }
    }

    fn field(key: &str, value: Value) -> Option<Value> {
        value.add(Value {
            cells: 1,
            bytes: 64_u64.checked_add(u64::try_from(key.len()).ok()?)?,
        })
    }

    fn shape_value(&self, shape: &super::ComparisonOperandShape, depth: usize) -> Option<Value> {
        use super::ComparisonOperandShape as Shape;
        if depth > MAX_CORE_TYPE_DEPTH {
            return None;
        }
        match shape {
            Shape::Coordinate(ty) => self.value_type(ty, depth),
            Shape::EmptyList => Some(Value::CELL),
            Shape::Record(fields) => fields.iter().try_fold(Value::CELL, |total, (key, item)| {
                total.add(Self::field(key, self.shape_value(item, depth + 1)?)?)
            }),
            Shape::List { item, max } => {
                let item = self.shape_value(item, depth + 1)?;
                Value::CELL.add(Value {
                    cells: item.cells.checked_mul(*max)?,
                    bytes: item.bytes.checked_mul(*max)?,
                })
            }
        }
    }

    fn value(&self, expr: &CoreExpr, available: &Scope<'_>, depth: usize) -> Option<Value> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        match expr {
            CoreExpr::Const(CoreValue::Null) => Some(Value::CELL),
            CoreExpr::Field { base, field } => self.shape_value(
                &super::field_expression_shape(self.core, self.facts, base, field, available)?,
                0,
            ),
            CoreExpr::Record { fields } => {
                fields.iter().try_fold(Value::CELL, |total, (key, expr)| {
                    total.add(Self::field(key, self.value(expr, available, depth + 1)?)?)
                })
            }
            CoreExpr::List { values } => values.iter().try_fold(Value::CELL, |total, expr| {
                total.add(self.value(expr, available, depth + 1)?)
            }),
            CoreExpr::If {
                then_value,
                else_value,
                ..
            } => Some(
                self.value(then_value, available, depth + 1)?
                    .maximum(self.value(else_value, available, depth + 1)?),
            ),
            CoreExpr::Match { arms, .. } => {
                let mut result = Value::CELL;
                for arm in arms {
                    let mut inner = available.clone();
                    if let Some(binder) = &arm.binder {
                        inner.insert(&binder.id, binder);
                    }
                    result = result.maximum(self.value(&arm.body, &inner, depth + 1)?);
                }
                Some(result)
            }
            _ => self.value_type(
                &expression_type_coordinate(self.core, self.facts, expr, available)?,
                0,
            ),
        }
    }

    fn intent(&mut self, intent: &'a CoreIntent) -> Option<Cost> {
        let input = self.value_type(&intent.input, 0)?;
        let output = self.value_type(&intent.output, 0)?;
        let mut cost = Cost {
            steps: input.cells.checked_add(output.cells)?.checked_mul(2)?,
            bytes: input.bytes.checked_add(output.bytes)?,
        };
        let available = scope(intent.body.locals.iter());
        for constraint in &intent.input_constraints {
            cost = cost.add(self.predicate(&constraint.predicate, &available, 0)?)?;
        }
        if let Some(basis) = &intent.basis {
            cost = cost.add(self.expression(basis, &available, 0)?)?;
        }
        cost.add(self.block(&intent.body, &available, 0)?)
    }

    fn function(&mut self, callee: &'a str) -> Option<Cost> {
        let name = callee
            .strip_prefix(self.core.coordinate.as_str())?
            .strip_prefix('.')?;
        let (name, function) = self.core.functions.get_key_value(name)?;
        if let Some(cost) = self.cached.get(name.as_str()) {
            return Some(*cost);
        }
        if self.visiting.contains(&name.as_str()) || self.visiting.len() >= MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        self.visiting.push(name);
        let available = scope(function.params.iter().chain(&function.body.locals));
        let mut cost = Cost::steps(1);
        for parameter in &function.params {
            cost = cost.add(Cost::steps(self.value_type(&parameter.ty, 0)?.cells))?;
        }
        cost = cost.add(Cost::steps(
            self.value_type(&function.return_type, 0)?.cells,
        ))?;
        for binding in &function.body.bindings {
            cost = cost.add(self.expression(&binding.value, &available, 0)?)?;
        }
        cost = cost.add(self.expression(&function.body.result, &available, 0)?)?;
        self.visiting.pop();
        self.cached.insert(name, cost);
        Some(cost)
    }

    fn expression(
        &mut self,
        expr: &'a CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let own = self.value(expr, available, depth)?.evaluation()?;
        let children = match expr {
            CoreExpr::Local { .. } | CoreExpr::Const(_) => Cost::default(),
            CoreExpr::Field { base, .. } => self.expression(base, available, depth + 1)?,
            CoreExpr::Variant { payload, .. } => match payload {
                Some(value) => self.expression(value, available, depth + 1)?,
                None => Cost::default(),
            },
            CoreExpr::Record { fields } => {
                fields.values().try_fold(Cost::default(), |sum, expr| {
                    sum.add(self.expression(expr, available, depth + 1)?)
                })?
            }
            CoreExpr::List { values } => values.iter().try_fold(Cost::default(), |sum, expr| {
                sum.add(self.expression(expr, available, depth + 1)?)
            })?,
            CoreExpr::Fold {
                initial,
                step,
                bound,
                ..
            } => {
                let initial = self.expression(initial, available, depth + 1)?;
                // A terminal/done flag is ordinary state: all N steps are charged.
                let repeated = self.function(step)?.add(Cost::steps(1))?.times(*bound)?;
                initial.add(repeated)?
            }
            CoreExpr::Call { callee, args, .. } => {
                let arguments = args.iter().try_fold(Cost::default(), |sum, arg| {
                    sum.add(self.expression(arg, available, depth + 1)?)
                })?;
                arguments.add(self.call(callee, args, expr, available, depth)?)?
            }
            CoreExpr::If {
                predicate,
                then_value,
                else_value,
            } => {
                let predicate = self.predicate(predicate, available, depth + 1)?;
                let then_cost = self.expression(then_value, available, depth + 1)?;
                let else_cost = self.expression(else_value, available, depth + 1)?;
                predicate.add(then_cost.maximum(else_cost))?
            }
            CoreExpr::Match { scrutinee, arms } => {
                let before = self.expression(scrutinee, available, depth + 1)?;
                let value = self.value(scrutinee, available, depth + 1)?;
                let mut selected = Cost::default();
                for arm in arms {
                    let mut inner = available.clone();
                    let mut storage = Cost::default();
                    if let Some(binder) = &arm.binder {
                        // Payload-bound environments copy the surrounding frame.
                        for local in available.values() {
                            storage = storage.add(self.value_type(&local.ty, 0)?.evaluation()?)?;
                        }
                        storage = storage.add(Cost {
                            steps: value.cells,
                            bytes: value.bytes,
                        })?;
                        inner.insert(&binder.id, binder);
                    }
                    selected = selected.maximum(storage.add(self.expression(
                        &arm.body,
                        &inner,
                        depth + 1,
                    )?)?);
                }
                before.add(selected)?.add(Cost::steps(
                    value.cells.checked_add(u64::try_from(arms.len()).ok()?)?,
                ))?
            }
        };
        own.add(children)
    }

    fn call(
        &mut self,
        callee: &'a str,
        args: &'a [CoreExpr],
        expr: &CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        let matches: Vec<_> = self
            .facts
            .iter()
            .filter(|fact| fact.coordinate == callee)
            .collect();
        if !matches.is_empty() {
            if matches.len() != 1 {
                return None;
            }
            let fact = matches[0];
            if let Some(rule) = &fact.intrinsic_cost {
                if args.len() != rule.per_argument_max.len() {
                    return None;
                }
                let steps = args.iter().zip(&rule.per_argument_max).try_fold(
                    rule.base,
                    |sum, (arg, coefficient)| {
                        if *coefficient == 0 {
                            return Some(sum);
                        }
                        let maximum = self.sequence_max(arg, available, 0)?;
                        sum.checked_add(coefficient.checked_mul(maximum)?)
                    },
                )?;
                return Some(Cost::steps(steps));
            }
            let CoreBudget {
                max_steps,
                max_allocated_bytes,
                ..
            } = fact.helper_cost.as_ref()?;
            return Some(Cost {
                steps: *max_steps,
                bytes: *max_allocated_bytes,
            });
        }
        if is_typed_prelude_operation(callee) || callee == "core.string.concat" {
            let mut steps = 0_u64;
            for arg in args {
                steps = steps.checked_add(self.value(arg, available, depth + 1)?.cells)?;
            }
            // Bounded storage includes scan/copy work for byte/text primitives.
            if !super::is_unsigned_arithmetic(callee) {
                steps = steps.checked_add(self.value(expr, available, depth + 1)?.bytes)?;
                for arg in args {
                    steps = steps.checked_add(self.value(arg, available, depth + 1)?.bytes)?;
                }
            }
            return Some(Cost::steps(steps));
        }
        self.function(callee)
    }

    fn sequence_max(&self, expr: &CoreExpr, available: &Scope<'_>, depth: usize) -> Option<u64> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        match expr {
            CoreExpr::If {
                then_value,
                else_value,
                ..
            } => Some(
                self.sequence_max(then_value, available, depth + 1)?
                    .max(self.sequence_max(else_value, available, depth + 1)?),
            ),
            CoreExpr::Match { arms, .. } => arms.iter().try_fold(0_u64, |maximum, arm| {
                let mut inner = available.clone();
                if let Some(binder) = &arm.binder {
                    inner.insert(&binder.id, binder);
                }
                Some(maximum.max(self.sequence_max(&arm.body, &inner, depth + 1)?))
            }),
            CoreExpr::List { values } => u64::try_from(values.len()).ok(),
            _ => match resolved_core_type(
                self.core,
                &expression_type_coordinate(self.core, self.facts, expr, available)?,
            )? {
                CoreType::Bytes { max, .. }
                | CoreType::String { max, .. }
                | CoreType::List { max, .. } => Some(max),
                _ => None,
            },
        }
    }

    fn predicate(
        &mut self,
        predicate: &'a CorePredicate,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let nested = match predicate {
            CorePredicate::True | CorePredicate::False => Cost::default(),
            CorePredicate::Not(value) => self.predicate(value, available, depth + 1)?,
            CorePredicate::All(items) | CorePredicate::Any(items) => {
                items.iter().try_fold(Cost::default(), |sum, item| {
                    sum.add(self.predicate(item, available, depth + 1)?)
                })?
            }
            CorePredicate::Compare { left, right, .. } => {
                let work = self
                    .value(left, available, depth + 1)?
                    .bytes
                    .max(self.value(right, available, depth + 1)?.bytes);
                self.expression(left, available, depth + 1)?
                    .add(self.expression(right, available, depth + 1)?)?
                    .add(Cost::steps(work))?
            }
        };
        nested.add(Cost::steps(1))
    }

    fn block(&mut self, block: &'a CoreBlock, outer: &Scope<'a>, depth: usize) -> Option<Cost> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let mut available = outer.clone();
        available.extend(scope(block.locals.iter()));
        let mut cost = Cost::default();
        for node in &block.nodes {
            cost = cost.add(self.node(node, &available, depth + 1)?)?;
        }
        cost.add(self.expression(&block.result, &available, depth + 1)?)
    }

    fn node(&mut self, node: &'a CoreNode, available: &Scope<'a>, depth: usize) -> Option<Cost> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let cost = match node {
            CoreNode::Let { value, .. } => self.expression(value, available, depth + 1)?,
            CoreNode::Require { predicate, arm } => {
                let (CoreRequireFailureArm::Terminal { reason }
                | CoreRequireFailureArm::ContinueObstructed { reason }) = arm;
                let mut cost = self.predicate(predicate, available, depth + 1)?;
                for value in reason.payload.values() {
                    cost = cost.add(self.expression(value, available, depth + 1)?)?;
                }
                cost
            }
            CoreNode::Effect {
                input,
                binding,
                obstruction_map,
                ..
            } => {
                let mut cost = self
                    .expression(input, available, depth + 1)?
                    .add(self.value_type(&binding.ty, 0)?.evaluation()?)?;
                for arm in obstruction_map.values() {
                    // Nullary obstruction annotations are not callable helpers.
                    // Match the existing closed-authority rule only here; a
                    // normal expression must still establish callable authority.
                    let annotation = match &arm.value {
                        CoreExpr::Call {
                            callee,
                            type_args,
                            args,
                        } if !callee.is_empty() && type_args.is_empty() && args.is_empty() => {
                            Value {
                                cells: 1,
                                bytes: 64_u64.checked_add(u64::try_from(callee.len()).ok()?)?,
                            }
                            .evaluation()?
                        }
                        value => self.expression(value, available, depth + 1)?,
                    };
                    cost = cost
                        .add(self.value_type(&arm.binder.ty, 0)?.evaluation()?)?
                        .add(annotation)?;
                }
                cost
            }
            CoreNode::Branch {
                predicate,
                then_block,
                else_block,
                ..
            } => {
                let condition = self.predicate(predicate, available, depth + 1)?;
                let then_cost = self.block(then_block, available, depth + 1)?;
                let else_cost = self.block(else_block, available, depth + 1)?;
                condition.add(then_cost.maximum(else_cost))?
            }
            CoreNode::For {
                binder,
                iter,
                bound: CoreBound::Literal(bound),
                body,
            } => {
                let initial = self.expression(iter, available, depth + 1)?;
                let mut inner = available.clone();
                inner.insert(&binder.id, binder);
                let body = self
                    .block(body, &inner, depth + 1)?
                    .add(self.value_type(&binder.ty, 0)?.evaluation()?)?;
                initial.add(body.times(*bound)?)?
            }
            CoreNode::For { .. } | CoreNode::ExternalActionRequest { .. } => return None,
        };
        cost.add(Cost::steps(1))
    }
}
