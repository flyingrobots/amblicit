//! Independent conservative Core work accounting for fold-capable modules.
//!
//! No compiler cost table or artifact-supplied estimate is trusted. Values are
//! charged in portable 64-byte cells; imported work comes only from sealed facts.
use super::effect_costs::{validations, Charge, TargetEffectCostFact};
use super::{
    expression_type_coordinate, is_typed_prelude_operation, resolved_core_type, CoreBlock,
    CoreBound, CoreBudget, CoreExpr, CoreIntent, CoreModule, CoreNode, CoreObstructionArm,
    CorePredicate, CoreRequireFailureArm, CoreType, CoreValue, LocalRef, TargetPureFunctionFact,
    MAX_CORE_GRAPH_DEPTH, MAX_CORE_TYPE_DEPTH,
};
use std::collections::BTreeMap;

mod reachable;
mod shared_reads;
pub(crate) use shared_reads::SHARED_READS_CAPABILITY;

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

pub(crate) fn intent_cost_fits(
    core: &CoreModule,
    facts: &[TargetPureFunctionFact],
    effect_costs: &[TargetEffectCostFact],
    intent: &CoreIntent,
) -> bool {
    if !core
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.fold/v1" || c == "core.list/v1" || c == SHARED_READS_CAPABILITY)
    {
        return true;
    }
    intent_work(core, facts, effect_costs, intent).is_some_and(|(steps, bytes)| {
        steps <= intent.core_evaluation_budget.max_steps
            && bytes <= intent.core_evaluation_budget.max_allocated_bytes
    })
}

/// The intent's static (steps, allocated bytes), or `None` when unestablished.
pub(crate) fn intent_work(
    core: &CoreModule,
    facts: &[TargetPureFunctionFact],
    effect_costs: &[TargetEffectCostFact],
    intent: &CoreIntent,
) -> Option<(u64, u64)> {
    let mut meter = Meter {
        effect_costs,
        costliest: None,
        costliest_known: false,
        shared: core
            .required_core_capabilities
            .iter()
            .any(|c| c == SHARED_READS_CAPABILITY),
        effect_body: false,
        core,
        facts,
        visiting: Vec::new(),
        cached: BTreeMap::new(),
        reach_env: BTreeMap::new(),
        reach_cache: BTreeMap::new(),
    };
    meter.intent(intent).map(|cost| (cost.steps, cost.bytes))
}

struct Meter<'a> {
    /// Provider effect cost templates, by semantic effect coordinate.
    effect_costs: &'a [TargetEffectCostFact],
    /// The costliest source helper, computed once when a template needs it.
    costliest: Option<(u64, u64)>,
    costliest_known: bool,
    /// `edict.core-cost/v2`: reads share values; only construction is sized.
    shared: bool,
    /// The intent being metered is an effect program: its body holds an
    /// effect node. Under the shared-read schedule Echo's effect-body judge
    /// charges each `let` its binding type's validations; its pure-intent
    /// judge does not.
    effect_body: bool,
    core: &'a CoreModule,
    facts: &'a [TargetPureFunctionFact],
    visiting: Vec<&'a str>,
    cached: BTreeMap<&'a str, Cost>,
    /// Shared-read schedule: proven list-length bounds of the current frame.
    reach_env: BTreeMap<String, reachable::Reach>,
    /// Shared-read schedule: call cost and result bounds by argument bounds.
    reach_cache: BTreeMap<(&'a str, Vec<reachable::Reach>), (Cost, reachable::Reach)>,
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
            // A presence cell plus the largest present item; absence is one cell.
            CoreType::Option { item } => Value::CELL.add(child(&item)?),
            CoreType::Map { .. }
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
            Shape::Never => Some(Value { cells: 0, bytes: 0 }),
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
            // A terminal failure returns no value. Its evaluation still costs
            // one step through Value::evaluation; enclosing work stays charged.
            CoreExpr::Fail { .. } => Some(Value { cells: 0, bytes: 0 }),
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
            CoreExpr::OrderedRecord { fields } => {
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
        self.effect_body = has_effect(&intent.body, 0);
        let available = scope(intent.body.locals.iter());
        for constraint in &intent.input_constraints {
            cost = cost.add(self.predicate(&constraint.predicate, &available, 0)?)?;
        }
        if let Some(basis) = &intent.basis {
            cost = cost.add(self.expression(basis, &available, 0)?)?;
        }
        cost.add(self.block(&intent.body, &available, 0)?)
    }

    fn function(&mut self, callee: &str) -> Option<Cost> {
        let name = callee
            .strip_prefix(self.core.coordinate.as_str())?
            .strip_prefix('.')?;
        self.helper(name)
    }

    /// The copying cost of one call to the source helper `name`.
    fn helper(&mut self, name: &str) -> Option<Cost> {
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
        if self.shared {
            // Arguments and the result are passed by reference.
            let params = u64::try_from(function.params.len()).ok()?;
            cost = cost.add(Cost::steps(params.checked_add(1)?))?;
        } else {
            for parameter in &function.params {
                cost = cost.add(Cost::steps(self.value_type(&parameter.ty, 0)?.cells))?;
            }
            cost = cost.add(Cost::steps(
                self.value_type(&function.return_type, 0)?.cells,
            ))?;
        }
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
        let own = if self.shared {
            self.shared_own(expr, available, depth)?
        } else {
            self.value(expr, available, depth)?.evaluation()?
        };
        let children = match expr {
            CoreExpr::Local { .. } | CoreExpr::Const(_) | CoreExpr::Fail { .. } => Cost::default(),
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
            CoreExpr::OrderedRecord { fields } => {
                fields.iter().try_fold(Cost::default(), |sum, (_, expr)| {
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
                let initial_cost = self.expression(initial, available, depth + 1)?;
                // A terminal/done flag is ordinary state: all N steps are charged.
                let repeated = if self.shared {
                    let start = self.reach(initial, available, depth + 1)?;
                    self.fold_reach(start, step, *bound)?.0
                } else {
                    self.function(step)?.add(Cost::steps(1))?.times(*bound)?
                };
                initial_cost.add(repeated)?
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
                        if self.shared {
                            // The payload binder is one shared reference.
                            storage = Cost::steps(1);
                            inner.insert(&binder.id, binder);
                            selected = selected.maximum(storage.add(self.expression(
                                &arm.body,
                                &inner,
                                depth + 1,
                            )?)?);
                            continue;
                        }
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
                let dispatch = if self.shared { 1 } else { value.cells };
                before.add(selected)?.add(Cost::steps(
                    dispatch.checked_add(u64::try_from(arms.len()).ok()?)?,
                ))?
            }
        };
        own.add(children)
    }

    fn call(
        &mut self,
        callee: &'a str,
        args: &'a [CoreExpr],
        expr: &'a CoreExpr,
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
            let template = self.foreign_template(matches[0], args, available)?;
            if self.shared {
                return template.add(self.shared_foreign_copies(args, expr, available, depth)?);
            }
            return Some(template);
        }
        if self.shared && Self::has_shared_rule(callee) {
            return self.shared_prelude(callee, expr, available, depth);
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
        if self.shared {
            let mut arguments = Vec::with_capacity(args.len());
            for arg in args {
                arguments.push(self.reach(arg, available, depth + 1)?);
            }
            return Some(self.function_reach(callee, arguments)?.0);
        }
        self.function(callee)
    }

    /// The declared cost template of one lawpack helper or target intrinsic.
    fn foreign_template(
        &self,
        fact: &TargetPureFunctionFact,
        args: &[CoreExpr],
        available: &Scope<'_>,
    ) -> Option<Cost> {
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
        Some(Cost {
            steps: *max_steps,
            bytes: *max_allocated_bytes,
        })
    }

    fn sequence_max(&self, expr: &CoreExpr, available: &Scope<'_>, depth: usize) -> Option<u64> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        match expr {
            // Size charging sees no returned sequence; expression() separately
            // retains the terminal failure's evaluation step.
            CoreExpr::Fail { .. } => Some(0),
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
            CoreNode::Let { binding, value } => self.let_node(binding, value, available, depth)?,
            CoreNode::Attempt {
                value, failure_map, ..
            } => {
                let cost = self.expression(value, available, depth + 1)?;
                let mut selected = Cost::default();
                for reason in failure_map.values() {
                    let mut arm = Cost::default();
                    for payload in reason.payload.values() {
                        arm = arm.add(self.expression(payload, available, depth + 1)?)?;
                    }
                    if self.shared && self.effect_body {
                        arm = arm.add(self.obstruction_payload(&reason.payload, available)?)?;
                    }
                    selected = selected.maximum(arm);
                }
                cost.add(selected)?
            }
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
                effect,
            } => {
                let input_cost = self.expression(input, available, depth + 1)?;
                let template = self.effect_template(effect, input, &binding.ty, available)?;
                if self.shared && self.has_effect_template(effect) {
                    // The node's own step stays outside the template, as in
                    // Echo's effect-body judge: every node is one step.
                    return self
                        .replaced_effect(
                            input_cost.add(template)?,
                            obstruction_map,
                            available,
                            depth,
                        )?
                        .add(Cost::steps(1));
                }
                let mut cost = input_cost
                    .add(self.value_type(&binding.ty, 0)?.evaluation()?)?
                    .add(template)?;
                for arm in obstruction_map.values() {
                    // Nullary obstruction annotations are not callable helpers.
                    // Match the existing closed-authority rule only here; a
                    // normal expression must still establish callable authority.
                    let annotation = match &arm.value {
                        CoreExpr::Call { callee, .. } if is_nullary_annotation(&arm.value) => {
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

impl<'a> Meter<'a> {
    /// A `let` node's charge beyond its node step.
    fn let_node(
        &mut self,
        binding: &LocalRef,
        value: &'a CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        let cost = self.expression(value, available, depth + 1)?;
        if !self.shared {
            return Some(cost);
        }
        let reach = self.reach(value, available, depth + 1)?;
        self.reach_env.insert(binding.id.clone(), reach);
        if !self.effect_body {
            return Some(cost);
        }
        // Echo's effect-body judge validates the bound value against its
        // declared type: `validations(type)` of the provider shape.
        cost.add(Cost::steps(validations(self.core, &binding.ty)?))
    }

    /// Under the shared-read schedule, what an effect body's failure arm adds
    /// beyond its payload fields' own work, as warp-core's shared executor
    /// charges it: the payload record's expression step and shell (one cell,
    /// plus a keyed cell per field), then encoding the record at its type's
    /// copying size, Echo's `shape(record)` scratch. An empty payload costs
    /// (2, 64) + (1, 64).
    fn obstruction_payload(
        &self,
        payload: &'a BTreeMap<String, CoreExpr>,
        available: &Scope<'a>,
    ) -> Option<Cost> {
        let mut shell = Value::CELL;
        let mut encoded = Value::CELL;
        for (name, field) in payload {
            shell = shell.add(Self::field(name, Value { cells: 0, bytes: 0 })?)?;
            let ty = expression_type_coordinate(self.core, self.facts, field, available)?;
            encoded = encoded.add(Self::field(name, self.value_type(&ty, 0)?)?)?;
        }
        shell.evaluation()?.add(Cost {
            steps: encoded.cells,
            bytes: encoded.bytes,
        })
    }

    /// Under the shared-read schedule the provider's template replaces the
    /// generic charge of an effect step beyond its input expression: the
    /// result binding and each obstruction arm's scratch. The node's own step
    /// is added by the caller.
    /// `charged` is the input expression plus the template.
    fn replaced_effect(
        &mut self,
        charged: Cost,
        obstruction_map: &'a BTreeMap<String, CoreObstructionArm>,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        let mut cost = charged;
        if !obstruction_map.is_empty() {
            // Echo builds one empty obstruction payload, `shape(record {})`,
            // when a step maps any arm; a template cannot see the arms.
            cost = cost.add(Cost {
                steps: 1,
                bytes: 64,
            })?;
        }
        for arm in obstruction_map.values() {
            if !is_nullary_annotation(&arm.value) {
                cost = cost.add(self.expression(&arm.value, available, depth + 1)?)?;
            }
        }
        Some(cost)
    }
}

/// Whether `block`, or a block nested in it, holds an effect node.
fn has_effect(block: &CoreBlock, depth: usize) -> bool {
    depth <= MAX_CORE_GRAPH_DEPTH
        && block.nodes.iter().any(|node| match node {
            CoreNode::Effect { .. } => true,
            CoreNode::Branch {
                then_block,
                else_block,
                ..
            } => has_effect(then_block, depth + 1) || has_effect(else_block, depth + 1),
            CoreNode::For { body, .. } => has_effect(body, depth + 1),
            _ => false,
        })
}

/// A nullary obstruction annotation: a bare failure name, not a callable helper.
fn is_nullary_annotation(value: &CoreExpr) -> bool {
    matches!(
        value,
        CoreExpr::Call { callee, type_args, args }
            if !callee.is_empty() && type_args.is_empty() && args.is_empty()
    )
}

impl Meter<'_> {
    /// Whether the provider publishes a template for `effect`.
    fn has_effect_template(&self, effect: &str) -> bool {
        self.effect_costs.iter().any(|fact| fact.effect == effect)
    }

    /// The provider template charge of one effect node; nothing without one.
    /// `None` when the template is unestablished.
    fn effect_template(
        &mut self,
        effect: &str,
        input: &CoreExpr,
        output: &str,
        available: &Scope<'_>,
    ) -> Option<Cost> {
        let facts = self.effect_costs;
        let Some(fact) = facts.iter().find(|fact| fact.effect == effect) else {
            return Some(Cost::default());
        };
        let input = expression_type_coordinate(self.core, self.facts, input, available)?;
        if !self.costliest_known {
            self.costliest = self.costliest_helper();
            self.costliest_known = true;
        }
        let costliest = self.costliest;
        let mut helpers = BTreeMap::new();
        for name in fact.helpers() {
            if let Some(cost) = self.named_helper(name) {
                helpers.insert(name.to_owned(), cost);
            }
        }
        let charge = Charge {
            core: self.core,
            input: &input,
            output,
            parameters: &fact.parameters,
            costliest_helper: costliest,
            helpers: &helpers,
        };
        let (steps, bytes) = charge.evaluate(&fact.rule)?;
        Some(Cost { steps, bytes })
    }

    /// One call to the source helper `name` under the module's schedule.
    fn helper_call(&mut self, name: &str) -> Option<Cost> {
        if self.shared {
            let parameters = self.core.functions.get(name)?.params.len();
            let arguments = vec![reachable::Reach::new(); parameters];
            Some(self.helper_reach(name, arguments)?.0)
        } else {
            self.helper(name)
        }
    }

    /// The charge for the helper a `helper` parameter names. The shared-read
    /// schedule prices exactly that helper. The copying schedule keeps the
    /// provider's v1 rule: any helper may be selected, so it charges the
    /// costliest, once the named helper is known to exist.
    fn named_helper(&mut self, name: &str) -> Option<(u64, u64)> {
        if !self.core.functions.contains_key(name) {
            return None;
        }
        if self.shared {
            let cost = self.helper_call(name)?;
            Some((cost.steps, cost.bytes))
        } else {
            self.costliest
        }
    }

    /// The largest (steps, bytes) of any one call to a source helper, under
    /// the module's schedule, or `None` when some helper is unestablished.
    fn costliest_helper(&mut self) -> Option<(u64, u64)> {
        let core = self.core;
        let mut worst = Cost::default();
        for name in core.functions.keys() {
            worst = worst.maximum(self.helper_call(name)?);
        }
        Some((worst.steps, worst.bytes))
    }
}

/// The provider template work of an intent's effect nodes alone: each node
/// once, a branch at its larger arm, a literal-bound loop body per iteration.
pub(crate) fn effect_template_work(
    core: &CoreModule,
    facts: &[TargetPureFunctionFact],
    effect_costs: &[TargetEffectCostFact],
    intent: &CoreIntent,
) -> Option<(u64, u64)> {
    let mut meter = Meter {
        effect_costs,
        costliest: None,
        costliest_known: false,
        shared: core
            .required_core_capabilities
            .iter()
            .any(|c| c == SHARED_READS_CAPABILITY),
        effect_body: false,
        core,
        facts,
        visiting: Vec::new(),
        cached: BTreeMap::new(),
        reach_env: BTreeMap::new(),
        reach_cache: BTreeMap::new(),
    };
    let available = scope(intent.body.locals.iter());
    meter
        .template_block(&intent.body, &available, 0)
        .map(|cost| (cost.steps, cost.bytes))
}

impl<'a> Meter<'a> {
    fn template_block(
        &mut self,
        block: &'a CoreBlock,
        outer: &Scope<'a>,
        depth: usize,
    ) -> Option<Cost> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        let mut available = outer.clone();
        available.extend(scope(block.locals.iter()));
        let mut total = Cost::default();
        for node in &block.nodes {
            let cost = match node {
                CoreNode::Effect {
                    effect,
                    input,
                    binding,
                    ..
                } => self.effect_template(effect, input, &binding.ty, &available)?,
                CoreNode::Branch {
                    then_block,
                    else_block,
                    ..
                } => self
                    .template_block(then_block, &available, depth + 1)?
                    .maximum(self.template_block(else_block, &available, depth + 1)?),
                CoreNode::For {
                    binder,
                    bound: CoreBound::Literal(bound),
                    body,
                    ..
                } => {
                    let mut inner = available.clone();
                    inner.insert(&binder.id, binder);
                    self.template_block(body, &inner, depth + 1)?
                        .times(*bound)?
                }
                CoreNode::For { .. } => return None,
                _ => Cost::default(),
            };
            total = total.add(cost)?;
        }
        Some(total)
    }
}
