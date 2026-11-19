//! A test-only Core evaluator metered by the shared-read schedule.
//!
//! Oracle: the metering rules are written from the shared-read schedule's
//! specification (`docs/topics/core-cost-schedules/README.md`), not from the
//! static meter. Values are shared; each evaluated node costs one step; a node
//! that builds a value also pays for the value's shell, measured on the actual
//! value; a comparison scans both operands in full; intent input and output
//! are charged at their full actual size.
use amblicit_syntax::{
    CompareOp, CoreExpr, CoreModule, CoreNode, CorePredicate, CorePureBlock, CoreValue,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V {
    Null,
    Bool(bool),
    Int(u128),
    Rec(BTreeMap<String, V>),
    List(Vec<V>),
    Var(String, String, Option<Box<V>>),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Meter {
    pub steps: u64,
    pub bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    /// A declared failure ended evaluation; the work so far still counts.
    Failure,
}

/// Full canonical size in (cells, bytes), as a comparison or input scan sees it.
pub fn full(value: &V) -> (u64, u64) {
    match value {
        V::Null | V::Bool(_) | V::Int(_) => (1, 64),
        V::Rec(fields) => fields.iter().fold((1, 64), |(c, b), (key, child)| {
            let (cc, cb) = full(child);
            (c + 1 + cc, b + 64 + key.len() as u64 + cb)
        }),
        V::List(items) => items.iter().fold((1, 64), |(c, b), child| {
            let (cc, cb) = full(child);
            (c + cc, b + cb)
        }),
        V::Var(ty, case, payload) => {
            let (pc, pb) = payload.as_deref().map_or((1, 64), full);
            (6 + pc, 399 + ty.len() as u64 + case.len() as u64 + pb)
        }
    }
}

/// The newly built part of a value; its children are shared references.
fn shell(value: &V) -> (u64, u64) {
    match value {
        V::Null | V::Bool(_) | V::Int(_) => (1, 64),
        V::Rec(fields) => fields
            .keys()
            .fold((1, 64), |(c, b), key| (c + 1, b + 64 + key.len() as u64)),
        V::List(items) => (1 + items.len() as u64, 64 * (1 + items.len() as u64)),
        V::Var(ty, case, _) => (7, 64 + 399 + ty.len() as u64 + case.len() as u64),
    }
}

/// A target intrinsic or lawpack helper: its result and its declared work on
/// the actual arguments (the template evaluated at actual operand lengths).
pub struct Foreign {
    pub result: fn(&[V]) -> V,
    pub work: fn(&[V]) -> u64,
}

pub struct Evaluator<'a> {
    pub core: &'a CoreModule,
    pub meter: Meter,
    /// Foreign calls by Core callee; each copies its arguments in and its
    /// result out in full, then performs its declared work.
    pub foreign: BTreeMap<String, Foreign>,
}

type Env = BTreeMap<String, V>;

impl Evaluator<'_> {
    fn step(&mut self, n: u64) {
        self.meter.steps += n;
    }

    fn build(&mut self, value: V) -> V {
        let (cells, bytes) = shell(&value);
        self.meter.steps += cells;
        self.meter.bytes += bytes;
        value
    }

    /// Evaluate one intent on `input`; returns the result or the stop.
    pub fn intent(&mut self, name: &str, input: V, output: impl Fn(&V) -> V) -> Result<V, Stop> {
        let intent = &self.core.intents[name];
        let mut env = Env::new();
        let (ic, ib) = full(&input);
        env.insert("arg.0".into(), input);
        self.meter.steps += 2 * ic;
        self.meter.bytes += ib;
        self.nodes(&intent.body.nodes, &mut env)?;
        let result = self.expr(&intent.body.result, &env)?;
        let (oc, ob) = full(&output(&result));
        self.meter.steps += 2 * oc;
        self.meter.bytes += ob;
        Ok(result)
    }

    /// Bindings and bounded loops; each loop item is one step plus a shared
    /// binder reference, then the body's nodes and result.
    fn nodes(&mut self, nodes: &[CoreNode], env: &mut Env) -> Result<(), Stop> {
        for node in nodes {
            match node {
                CoreNode::Let { binding, value } | CoreNode::Attempt { binding, value, .. } => {
                    let value = self.expr(value, env)?;
                    env.insert(binding.id.clone(), value);
                }
                CoreNode::For {
                    binder, iter, body, ..
                } => {
                    let V::List(items) = self.expr(iter, env)? else {
                        panic!("loop over a non-list")
                    };
                    for item in items {
                        self.step(2);
                        let mut inner = env.clone();
                        inner.insert(binder.id.clone(), item);
                        self.nodes(&body.nodes, &mut inner)?;
                        self.expr(&body.result, &inner)?;
                    }
                }
                other => panic!("reference evaluator does not model {other:?}"),
            }
        }
        Ok(())
    }

    fn function(&mut self, callee: &str, args: Vec<V>) -> Result<V, Stop> {
        let name = callee
            .strip_prefix(&format!("{}.", self.core.coordinate))
            .unwrap_or_else(|| panic!("unmodelled callee {callee}"));
        let function = &self.core.functions[name];
        self.step(2 + function.params.len() as u64);
        let mut env = Env::new();
        for (param, arg) in function.params.iter().zip(args) {
            env.insert(param.id.clone(), arg);
        }
        self.block(&function.body, env)
    }

    fn block(&mut self, block: &CorePureBlock, mut env: Env) -> Result<V, Stop> {
        for binding in &block.bindings {
            let value = self.expr(&binding.value, &env)?;
            env.insert(binding.binding.id.clone(), value);
        }
        self.expr(&block.result, &env)
    }

    fn expr(&mut self, expr: &CoreExpr, env: &Env) -> Result<V, Stop> {
        self.step(1);
        Ok(match expr {
            CoreExpr::Local { reference } => env[&reference.id].clone(),
            CoreExpr::Const(CoreValue::Null) => self.build(V::Null),
            CoreExpr::Const(CoreValue::Bool(b)) => self.build(V::Bool(*b)),
            CoreExpr::Const(CoreValue::Int { value, .. }) => {
                self.build(V::Int(value.parse().expect("integer literal")))
            }
            CoreExpr::Const(other) => panic!("unmodelled constant {other:?}"),
            CoreExpr::Record { fields } => {
                let mut out = BTreeMap::new();
                for (key, value) in fields {
                    out.insert(key.clone(), self.expr(value, env)?);
                }
                self.build(V::Rec(out))
            }
            CoreExpr::OrderedRecord { fields } => {
                let mut out = BTreeMap::new();
                for (key, value) in fields {
                    out.insert(key.clone(), self.expr(value, env)?);
                }
                self.build(V::Rec(out))
            }
            CoreExpr::List { values } => {
                let mut out = Vec::new();
                for value in values {
                    out.push(self.expr(value, env)?);
                }
                self.build(V::List(out))
            }
            CoreExpr::Variant { ty, case, payload } => {
                let payload = match payload {
                    Some(p) => Some(Box::new(self.expr(p, env)?)),
                    None => None,
                };
                self.build(V::Var(ty.clone(), case.clone(), payload))
            }
            CoreExpr::Field { base, field } => match self.expr(base, env)? {
                V::Rec(mut fields) => fields.remove(field).expect("field"),
                other => panic!("field of {other:?}"),
            },
            CoreExpr::If {
                predicate,
                then_value,
                else_value,
            } => {
                if self.predicate(predicate, env)? {
                    self.expr(then_value, env)?
                } else {
                    self.expr(else_value, env)?
                }
            }
            CoreExpr::Fail { .. } => return Err(Stop::Failure),
            CoreExpr::Fold {
                initial,
                step,
                bound,
                ..
            } => {
                let mut state = self.expr(initial, env)?;
                for _ in 0..*bound {
                    self.step(1);
                    state = self.function(step, vec![state])?;
                }
                state
            }
            CoreExpr::Match { scrutinee, arms } => {
                let V::Var(_, case, payload) = self.expr(scrutinee, env)? else {
                    panic!("match on non-variant");
                };
                self.step(1);
                let arm = arms.iter().find(|arm| arm.case == case).expect("arm");
                let mut inner = env.clone();
                if let Some(binder) = &arm.binder {
                    self.step(1);
                    inner.insert(binder.id.clone(), *payload.expect("payload"));
                }
                self.expr(&arm.body, &inner)?
            }
            CoreExpr::Call { callee, args, .. } => {
                let mut values = Vec::new();
                for arg in args {
                    values.push(self.expr(arg, env)?);
                }
                self.call(callee, values)?
            }
        })
    }

    fn call(&mut self, callee: &str, mut args: Vec<V>) -> Result<V, Stop> {
        let int = |v: &V| match v {
            V::Int(i) => *i,
            other => panic!("integer expected, got {other:?}"),
        };
        Ok(match callee {
            "core.list.length" => match &args[0] {
                V::List(items) => self.build(V::Int(items.len() as u128)),
                other => panic!("list expected, got {other:?}"),
            },
            "core.list.at" => match (&args[0], int(&args[1])) {
                (V::List(items), index) => items[usize::try_from(index).unwrap()].clone(),
                other => panic!("list expected, got {other:?}"),
            },
            "core.list.append" => {
                let item = args.pop().unwrap();
                let V::List(mut items) = args.pop().unwrap() else {
                    panic!("list")
                };
                items.push(item);
                self.build(V::List(items))
            }
            "core.list.concat" => {
                let V::List(right) = args.pop().unwrap() else {
                    panic!("list")
                };
                let V::List(mut left) = args.pop().unwrap() else {
                    panic!("list")
                };
                left.extend(right);
                self.build(V::List(left))
            }
            "core.integer.add" => {
                let sum = int(&args[0]) + int(&args[1]);
                self.build(V::Int(sum))
            }
            "core.integer.subtract" => {
                let difference = int(&args[0]) - int(&args[1]);
                self.build(V::Int(difference))
            }
            _ if self.foreign.contains_key(callee) => {
                let foreign = &self.foreign[callee];
                let (result_of, work_of) = (foreign.result, foreign.work);
                for arg in &args {
                    let (cells, bytes) = full(arg);
                    self.meter.steps += cells + 1;
                    self.meter.bytes += bytes;
                }
                self.meter.steps += work_of(&args);
                let result = result_of(&args);
                let (cells, bytes) = full(&result);
                self.meter.steps += cells;
                self.meter.bytes += bytes;
                result
            }
            _ => self.function(callee, args)?,
        })
    }

    fn predicate(&mut self, predicate: &CorePredicate, env: &Env) -> Result<bool, Stop> {
        self.step(1);
        Ok(match predicate {
            CorePredicate::True => true,
            CorePredicate::False => false,
            CorePredicate::Not(inner) => !self.predicate(inner, env)?,
            CorePredicate::All(items) => {
                // Ordered short-circuit: later conjuncts may rely on earlier ones.
                let mut all = true;
                for item in items {
                    if !self.predicate(item, env)? {
                        all = false;
                        break;
                    }
                }
                all
            }
            CorePredicate::Any(items) => {
                let mut any = false;
                for item in items {
                    if self.predicate(item, env)? {
                        any = true;
                        break;
                    }
                }
                any
            }
            CorePredicate::Compare { op, left, right } => {
                let left = self.expr(left, env)?;
                let right = self.expr(right, env)?;
                self.step(full(&left).1.max(full(&right).1));
                match (op, &left, &right) {
                    (CompareOp::Eq, l, r) => l == r,
                    (CompareOp::Ne, l, r) => l != r,
                    (op, V::Int(l), V::Int(r)) => match op {
                        CompareOp::Lt => l < r,
                        CompareOp::Le => l <= r,
                        CompareOp::Gt => l > r,
                        CompareOp::Ge => l >= r,
                        _ => unreachable!(),
                    },
                    other => panic!("ordered comparison of {other:?}"),
                }
            }
        })
    }
}
