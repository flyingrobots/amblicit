//! Source-owned function integrity and context-free call-height summaries.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    is_core_field_name, validate_core_expression_types, validate_core_graph_depth,
    validate_core_local_type, validate_core_type_reference, CoreExpr, CoreFunction, CoreModule,
    CorePredicate, CoreTypeIntegrityFailure, CoreTypeIntegrityFailureKind, CoreTypeIntegrityState,
    LocalRef, MAX_CORE_GRAPH_DEPTH,
};

pub(super) fn validate_functions(
    module: &CoreModule,
    state: &mut CoreTypeIntegrityState,
) -> Result<(), CoreTypeIntegrityFailure> {
    for (name, function) in &module.functions {
        let path = format!("functions.{name}");
        if !is_core_field_name(name)
            || module.intents.contains_key(name)
            || module.types.contains_key(name)
        {
            return Err(CoreTypeIntegrityFailure::new(
                CoreTypeIntegrityFailureKind::InvalidTableKey,
                path,
            ));
        }
        validate_core_type_reference(
            module,
            state,
            &function.return_type,
            &format!("{path}.returnType"),
            0,
        )?;
        let mut scope = BTreeMap::new();
        for (index, local) in function.params.iter().enumerate() {
            validate_core_local_type(
                module,
                state,
                local,
                &format!("{path}.params[{index}].type"),
            )?;
            if local.id.is_empty()
                || local.alpha_name.is_empty()
                || scope.insert(local.id.as_str(), local).is_some()
            {
                return Err(invalid(&path));
            }
        }
        let mut declarations = BTreeMap::new();
        for local in &function.body.locals {
            validate_core_local_type(
                module,
                state,
                local,
                &format!("{path}.body.locals.{}", local.id),
            )?;
            if local.id.is_empty()
                || local.alpha_name.is_empty()
                || scope.contains_key(local.id.as_str())
                || declarations.insert(local.id.as_str(), local).is_some()
            {
                return Err(invalid(&path));
            }
        }
        if declarations.len() != function.body.bindings.len() {
            return Err(invalid(&path));
        }
        for (index, binding) in function.body.bindings.iter().enumerate() {
            let binding_path = format!("{path}.body.bindings[{index}]");
            if declarations.remove(binding.binding.id.as_str()) != Some(&binding.binding) {
                return Err(invalid(&binding_path));
            }
            validate_core_expression_types(
                module,
                state,
                &binding.value,
                &format!("{binding_path}.value"),
                0,
            )?;
            validate_scope(&binding.value, &scope, &binding_path)?;
            scope.insert(binding.binding.id.as_str(), &binding.binding);
        }
        validate_core_expression_types(
            module,
            state,
            &function.body.result,
            &format!("{path}.body.result"),
            0,
        )?;
        validate_scope(
            &function.body.result,
            &scope,
            &format!("{path}.body.result"),
        )?;
    }
    validate_function_graph(&module.coordinate, &module.functions)
        .map(|_| ())
        .map_err(|failure| failure.integrity)
}

fn invalid(path: &str) -> CoreTypeIntegrityFailure {
    CoreTypeIntegrityFailure::new(CoreTypeIntegrityFailureKind::InvalidDefinition, path)
}

fn validate_scope(
    expression: &CoreExpr,
    scope: &BTreeMap<&str, &LocalRef>,
    path: &str,
) -> Result<(), CoreTypeIntegrityFailure> {
    walk(expression, 0, &mut |expression| {
        if let CoreExpr::Local { reference } = expression {
            if scope.get(reference.id.as_str()).copied() != Some(reference) {
                return Err(invalid(path));
            }
        }
        Ok(())
    })
}

fn walk(
    expression: &CoreExpr,
    depth: usize,
    check: &mut impl FnMut(&CoreExpr) -> Result<(), CoreTypeIntegrityFailure>,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_graph_depth(depth, "functions.expression")?;
    check(expression)?;
    match expression {
        CoreExpr::Const(_) | CoreExpr::Local { .. } => Ok(()),
        CoreExpr::Field { base, .. } => walk(base, depth + 1, check),
        CoreExpr::Record { fields } => {
            for value in fields.values() {
                walk(value, depth + 1, check)?;
            }
            Ok(())
        }
        CoreExpr::Call { args, .. } => {
            for value in args {
                walk(value, depth + 1, check)?;
            }
            Ok(())
        }
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            walk_predicate(predicate, depth + 1, check)?;
            walk(then_value, depth + 1, check)?;
            walk(else_value, depth + 1, check)
        }
    }
}

fn walk_predicate(
    predicate: &CorePredicate,
    depth: usize,
    check: &mut impl FnMut(&CoreExpr) -> Result<(), CoreTypeIntegrityFailure>,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_graph_depth(depth, "functions.predicate")?;
    match predicate {
        CorePredicate::True | CorePredicate::False => Ok(()),
        CorePredicate::Not(inner) => walk_predicate(inner, depth + 1, check),
        CorePredicate::All(items) | CorePredicate::Any(items) => {
            for item in items {
                walk_predicate(item, depth + 1, check)?;
            }
            Ok(())
        }
        CorePredicate::Compare { left, right, .. } => {
            walk(left, depth + 1, check)?;
            walk(right, depth + 1, check)
        }
    }
}

/// Diagnostic attribution is private to graph validation. The public integrity
/// failure retains its existing kind and path, including module-wide sentinels.
#[derive(Debug)]
pub(crate) struct FunctionGraphFailure {
    integrity: CoreTypeIntegrityFailure,
    function: Option<String>,
}

impl FunctionGraphFailure {
    fn attributed(integrity: CoreTypeIntegrityFailure, function: Option<&str>) -> Self {
        Self {
            integrity,
            function: function.map(str::to_owned),
        }
    }

    pub(crate) fn integrity(&self) -> &CoreTypeIntegrityFailure {
        &self.integrity
    }

    pub(crate) fn function_name(&self) -> Option<&str> {
        self.function.as_deref()
    }
}

/// Check every source-owned definition, including unreachable definitions. Heights
/// count function frames, so a leaf has height one; completed summaries are not
/// permission to omit a later caller's edge. This walk never expands the DAG.
pub(crate) fn validate_function_graph(
    coordinate: &str,
    functions: &BTreeMap<String, CoreFunction>,
) -> Result<BTreeMap<String, usize>, FunctionGraphFailure> {
    let mut graph = BTreeMap::new();
    let prefix = format!("{coordinate}.");
    let mut work = 65_536_usize;
    for (name, function) in functions {
        let edges = function_edges(name, function, functions, &prefix, &mut work)?;
        graph.insert(name.clone(), edges);
    }
    let mut completed = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    for name in graph.keys() {
        height(name, &graph, &mut completed, &mut visiting, 0)?;
    }
    Ok(completed)
}

fn function_edges(
    name: &str,
    function: &CoreFunction,
    functions: &BTreeMap<String, CoreFunction>,
    prefix: &str,
    work: &mut usize,
) -> Result<BTreeSet<String>, FunctionGraphFailure> {
    let mut edges = BTreeSet::new();
    let mut exhausted = false;
    let mut collect = |expression: &CoreExpr| {
        *work = work.checked_sub(1).ok_or_else(|| {
            exhausted = true;
            CoreTypeIntegrityFailure::new(
                CoreTypeIntegrityFailureKind::DepthExceeded,
                "functions.work",
            )
        })?;
        if let CoreExpr::Call {
            callee,
            args,
            type_args,
        } = expression
        {
            // A package prefix can also belong to an imported lawpack.
            // Only declared source members form this graph; the compiler
            // and independent Target checker authenticate external calls.
            let source = callee
                .strip_prefix(prefix)
                .and_then(|target| functions.get(target).map(|called| (target, called)));
            if let Some((target, called)) = source {
                if !type_args.is_empty() || args.len() != called.params.len() {
                    return Err(invalid(&format!("functions.{name}.call.{callee}")));
                }
                edges.insert(target.to_owned());
            }
        }
        Ok(())
    };
    let walked = function
        .body
        .bindings
        .iter()
        .try_for_each(|binding| walk(&binding.value, 0, &mut collect))
        .and_then(|()| walk(&function.body.result, 0, &mut collect));
    walked.map_err(|failure| {
        // The work allowance belongs to the complete module, not whichever
        // function happens to exhaust it or shares its descriptive path.
        FunctionGraphFailure::attributed(failure, (!exhausted).then_some(name))
    })?;
    Ok(edges)
}

fn height(
    name: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    completed: &mut BTreeMap<String, usize>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> Result<usize, FunctionGraphFailure> {
    let failure = |kind| {
        FunctionGraphFailure::attributed(
            CoreTypeIntegrityFailure::new(kind, format!("functions.{name}")),
            Some(name),
        )
    };
    if visiting.contains(name) {
        return Err(failure(CoreTypeIntegrityFailureKind::ReferenceCycle));
    }
    if depth >= MAX_CORE_GRAPH_DEPTH {
        return Err(failure(CoreTypeIntegrityFailureKind::DepthExceeded));
    }
    if let Some(height) = completed.get(name) {
        return Ok(*height);
    }
    visiting.insert(name.to_owned());
    let mut result = 1;
    for child in &graph[name] {
        let child_height = height(child, graph, completed, visiting, depth + 1)?;
        result = result.max(child_height + 1);
        if result > MAX_CORE_GRAPH_DEPTH {
            return Err(failure(CoreTypeIntegrityFailureKind::DepthExceeded));
        }
    }
    visiting.remove(name);
    completed.insert(name.to_owned(), result);
    Ok(result)
}
