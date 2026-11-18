//! This module owns reachable-length bounds for the shared-read meter (#335).
//!
//! The meter evaluates pure code over an abstract environment that maps each
//! local to upper bounds on the lengths of the lists inside its value, keyed by
//! field path (the empty path is the value itself). A path without an entry has
//! no proven bound and is charged at its declared maximum. Bounds only come
//! from list literals, appends and concatenations, and are capped by the
//! declared maximum, so every bound is at least the concrete length.
//!
//! A fold is charged round by round under the abstract state the previous round
//! produced. Once a round's abstract result equals its input, every remaining
//! round has that same cost. Every round of the declared bound is still
//! charged, and branches still take their largest arm.
use super::{
    expression_type_coordinate, resolved_core_type, scope, CoreExpr, CoreType, Cost, Meter, Scope,
    MAX_CORE_GRAPH_DEPTH,
};
use std::collections::BTreeMap;

/// A list's length bound: proven by the meter, or only its declared maximum.
#[derive(Clone, Copy)]
pub(super) enum Length {
    Proven(u64),
    Declared,
}

/// Upper bounds on list lengths within one value, by field path.
pub(super) type Reach = BTreeMap<Vec<String>, u64>;

/// The bound two branches share: a path keeps a bound only if both arms prove one.
fn join(left: &Reach, right: &Reach) -> Reach {
    left.iter()
        .filter_map(|(path, bound)| {
            right
                .get(path)
                .map(|other| (path.clone(), (*bound).max(*other)))
        })
        .collect()
}

fn prefixed(field: &str, reach: Reach) -> impl Iterator<Item = (Vec<String>, u64)> + '_ {
    reach.into_iter().map(move |(mut path, bound)| {
        path.insert(0, field.to_owned());
        (path, bound)
    })
}

impl<'a> Meter<'a> {
    /// The declared maximum of a list-typed expression.
    pub(super) fn declared_list_max(&self, expr: &CoreExpr, available: &Scope<'_>) -> Option<u64> {
        let coordinate = expression_type_coordinate(self.core, self.facts, expr, available)?;
        match resolved_core_type(self.core, &coordinate)? {
            CoreType::List { max, .. } => Some(max),
            _ => None,
        }
    }

    /// The length bound of the list `expr` evaluates to, or `None` when the
    /// meter cannot establish the expression at all.
    pub(super) fn reach_length(
        &mut self,
        expr: &'a CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Length> {
        Some(
            match self
                .reach(expr, available, depth)?
                .get(&Vec::new())
                .copied()
            {
                Some(bound) => Length::Proven(bound),
                None => Length::Declared,
            },
        )
    }

    /// Length bounds for the value `expr` evaluates to.
    pub(super) fn reach(
        &mut self,
        expr: &'a CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Reach> {
        if depth > MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        Some(match expr {
            CoreExpr::Local { reference } => self
                .reach_env
                .get(&reference.id)
                .cloned()
                .unwrap_or_default(),
            CoreExpr::Field { base, field } => self
                .reach(base, available, depth + 1)?
                .into_iter()
                .filter_map(|(path, bound)| match path.split_first() {
                    Some((head, rest)) if head == field => Some((rest.to_vec(), bound)),
                    _ => None,
                })
                .collect(),
            CoreExpr::Record { fields } => {
                let mut reach = Reach::new();
                for (key, value) in fields {
                    let child = self.reach(value, available, depth + 1)?;
                    reach.extend(prefixed(key, child));
                }
                reach
            }
            CoreExpr::OrderedRecord { fields } => {
                let mut reach = Reach::new();
                for (key, value) in fields {
                    let child = self.reach(value, available, depth + 1)?;
                    reach.extend(prefixed(key, child));
                }
                reach
            }
            CoreExpr::List { values } => {
                Reach::from([(Vec::new(), u64::try_from(values.len()).ok()?)])
            }
            CoreExpr::If {
                then_value,
                else_value,
                ..
            } => join(
                &self.reach(then_value, available, depth + 1)?,
                &self.reach(else_value, available, depth + 1)?,
            ),
            CoreExpr::Match { arms, .. } => {
                let mut joined: Option<Reach> = None;
                for arm in arms {
                    let mut inner = available.clone();
                    if let Some(binder) = &arm.binder {
                        inner.insert(&binder.id, binder);
                    }
                    let arm_reach = self.reach(&arm.body, &inner, depth + 1)?;
                    joined = Some(match joined {
                        Some(previous) => join(&previous, &arm_reach),
                        None => arm_reach,
                    });
                }
                joined.unwrap_or_default()
            }
            CoreExpr::Fold {
                initial,
                step,
                bound,
                ..
            } => {
                let start = self.reach(initial, available, depth + 1)?;
                self.fold_reach(start, step, *bound)?.1
            }
            CoreExpr::Call { callee, args, .. } => {
                self.call_reach(callee, args, expr, available, depth)?
            }
            CoreExpr::Const(_) | CoreExpr::Variant { .. } | CoreExpr::Fail { .. } => Reach::new(),
        })
    }

    fn call_reach(
        &mut self,
        callee: &'a str,
        args: &'a [CoreExpr],
        expr: &'a CoreExpr,
        available: &Scope<'a>,
        depth: usize,
    ) -> Option<Reach> {
        let grown = |this: &mut Self, extra: Length| -> Option<Reach> {
            let max = this.declared_list_max(expr, available)?;
            let base = this.reach_length(&args[0], available, depth + 1)?;
            Some(match (base, extra) {
                (Length::Proven(base), Length::Proven(extra)) => {
                    Reach::from([(Vec::new(), base.saturating_add(extra).min(max))])
                }
                _ => Reach::new(),
            })
        };
        match callee {
            "core.list.append" if args.len() == 2 => grown(self, Length::Proven(1)),
            "core.list.concat" if args.len() == 2 => {
                let right = self.reach_length(&args[1], available, depth + 1)?;
                grown(self, right)
            }
            _ if self.is_source_function(callee) => {
                let mut arguments = Vec::with_capacity(args.len());
                for arg in args {
                    arguments.push(self.reach(arg, available, depth + 1)?);
                }
                Some(self.function_reach(callee, arguments)?.1)
            }
            _ => Some(Reach::new()),
        }
    }

    fn is_source_function(&self, callee: &str) -> bool {
        callee
            .strip_prefix(self.core.coordinate.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|name| self.core.functions.contains_key(name))
    }

    /// The shared-read cost and result bounds of one call under argument bounds.
    pub(super) fn function_reach(
        &mut self,
        callee: &'a str,
        arguments: Vec<Reach>,
    ) -> Option<(Cost, Reach)> {
        let name = callee
            .strip_prefix(self.core.coordinate.as_str())?
            .strip_prefix('.')?;
        let (name, function) = self.core.functions.get_key_value(name)?;
        let key = (name.as_str(), arguments);
        if let Some(cached) = self.reach_cache.get(&key) {
            return Some(cached.clone());
        }
        if self.visiting.contains(&name.as_str()) || self.visiting.len() >= MAX_CORE_GRAPH_DEPTH {
            return None;
        }
        if function.params.len() != key.1.len() {
            return None;
        }
        self.visiting.push(name);
        let frame: BTreeMap<String, Reach> = function
            .params
            .iter()
            .map(|param| param.id.clone())
            .zip(key.1.iter().cloned())
            .collect();
        let saved = std::mem::replace(&mut self.reach_env, frame);
        let available = scope(function.params.iter().chain(&function.body.locals));
        let evaluated = (|| {
            // Arguments and the result are passed by reference.
            let params = u64::try_from(function.params.len()).ok()?;
            let mut cost = Cost::steps(params.checked_add(2)?);
            for binding in &function.body.bindings {
                cost = cost.add(self.expression(&binding.value, &available, 0)?)?;
                let reach = self.reach(&binding.value, &available, 0)?;
                self.reach_env.insert(binding.binding.id.clone(), reach);
            }
            cost = cost.add(self.expression(&function.body.result, &available, 0)?)?;
            let result = self.reach(&function.body.result, &available, 0)?;
            Some((cost, result))
        })();
        self.reach_env = saved;
        self.visiting.pop();
        let evaluated = evaluated?;
        self.reach_cache.insert(key, evaluated.clone());
        Some(evaluated)
    }

    /// Cost of the fold's rounds (one step plus the step call each) and the
    /// bounds of its final state.
    pub(super) fn fold_reach(
        &mut self,
        mut state: Reach,
        step: &'a str,
        bound: u64,
    ) -> Option<(Cost, Reach)> {
        let mut total = Cost::default();
        let mut round = 0_u64;
        while round < bound {
            let (cost, next) = self.function_reach(step, vec![state.clone()])?;
            let per_round = cost.add(Cost::steps(1))?;
            if next == state {
                // A fixed point: every remaining round costs the same.
                total = total.add(per_round.times(bound - round)?)?;
                return Some((total, state));
            }
            total = total.add(per_round)?;
            state = next;
            round += 1;
        }
        Some((total, state))
    }
}
