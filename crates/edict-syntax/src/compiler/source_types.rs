//! Dependency-first source types with transparent aliases and bounded expansion.
use super::{
    classify_core_type_reference, error, CompilerError, CompilerErrorKind, CompilerStage,
    CoreTypeReference, ResolvedModule, Span, TypeChecker, TypeDecl, TypeExpr, TypeKind, TypeRef,
    TypeShape, MAX_CORE_TYPE_DEPTH,
};
use std::collections::{BTreeMap, BTreeSet};

const SOURCE_TYPE_WORK: u64 = 65_536;
type Graph = BTreeMap<String, (usize, BTreeSet<String>)>;

impl TypeChecker<'_> {
    pub(super) fn check_types(&mut self) {
        let order = match declaration_order(self.resolved) {
            Ok(order) => order,
            Err(failure) => {
                self.errors.push(*failure);
                return;
            }
        };
        let mut weights = BTreeMap::new();
        let mut remaining = SOURCE_TYPE_WORK;
        for index in order {
            let decl = &self.resolved.types[index];
            let Some(weight) =
                declaration_weight(&decl.source, &weights).filter(|weight| *weight <= remaining)
            else {
                self.errors.push(*bound_failure(
                    decl.source.span,
                    "source type declaration expansion work exceeded",
                ));
                return;
            };
            remaining -= weight;
            let previous_errors = self.errors.len();
            let Some(shape) = self.type_decl_shape(&decl.source) else {
                if self.errors.len() == previous_errors {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        "source type declaration has no lowerable representation",
                        decl.source.span,
                    ));
                }
                continue;
            };
            if !shape_fits_depth(&shape) {
                self.errors.push(*bound_failure(
                    decl.source.span,
                    "expanded source structural type depth exceeded",
                ));
                continue;
            }
            // A source alias is a lexical spelling for its target, not another
            // nominal contract or a duplicate Core type definition.
            if !matches!(decl.source.body, TypeExpr::Ref(_)) {
                self.core_types.insert(decl.name.clone(), shape.core_type());
            }
            weights.insert(decl.name.clone(), weight);
            self.named_types.insert(decl.name.clone(), shape);
        }
    }
}

fn declaration_order(resolved: &ResolvedModule) -> Result<Vec<usize>, Box<CompilerError>> {
    let mut work = SOURCE_TYPE_WORK;
    if resolved.types.len() > usize::try_from(work).unwrap_or(usize::MAX) {
        return Err(bound_failure(
            Span::new(0, 0),
            "source type declaration count exceeded",
        ));
    }
    let mut graph = Graph::new();
    for (index, decl) in resolved.types.iter().enumerate() {
        if !matches!(
            classify_core_type_reference(&decl.name),
            Some(CoreTypeReference::Named)
        ) {
            return Err(Box::new(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::ReservedTypeIdentity,
                format!("type `{}` cannot become a Core named identity", decl.name),
                decl.source.span,
            )));
        }
        if !decl.source.params.is_empty() {
            return Err(Box::new(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "generic source type declarations are not supported",
                decl.source.span,
            )));
        }
        if graph
            .insert(
                decl.name.clone(),
                (index, references(&decl.source, &mut work)?),
            )
            .is_some()
        {
            return Err(Box::new(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::SurfaceValidation,
                "duplicate source type declaration",
                decl.source.span,
            )));
        }
    }
    let mut visiting = BTreeSet::new();
    let mut heights = BTreeMap::new();
    let mut order = Vec::new();
    for name in graph.keys() {
        visit(
            name,
            &graph,
            resolved,
            &mut visiting,
            &mut heights,
            &mut order,
        )?;
    }
    Ok(order)
}

fn references(decl: &TypeDecl, work: &mut u64) -> Result<BTreeSet<String>, Box<CompilerError>> {
    let root_depth = usize::from(!matches!(decl.body, TypeExpr::Ref(_)));
    let mut pending = roots(&decl.body)
        .into_iter()
        .map(|ty| (ty, root_depth))
        .collect::<Vec<_>>();
    let mut references = BTreeSet::new();
    while let Some((ty, depth)) = pending.pop() {
        *work = work
            .checked_sub(1)
            .ok_or_else(|| bound_failure(decl.span, "source type reference work exceeded"))?;
        if depth > MAX_CORE_TYPE_DEPTH {
            return Err(bound_failure(
                decl.span,
                "source structural type depth exceeded",
            ));
        }
        match ty {
            TypeRef::Named { path, args } => {
                if path.len() == 1 {
                    references.insert(path[0].clone());
                }
                pending.extend(args.iter().map(|ty| (ty, depth + 1)));
            }
            TypeRef::Option(inner)
            | TypeRef::CapabilityRef(inner)
            | TypeRef::List { elem: inner, .. } => pending.push((inner, depth + 1)),
            TypeRef::Map { key, value, .. } => {
                pending.push((key, depth + 1));
                pending.push((value, depth + 1));
            }
            TypeRef::StringTy(_) | TypeRef::BytesTy(_) => {}
        }
    }
    Ok(references)
}
fn roots(body: &TypeExpr) -> Vec<&TypeRef> {
    match body {
        TypeExpr::Ref(ty) => vec![ty],
        TypeExpr::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
        TypeExpr::Variant(cases) => cases
            .iter()
            .filter_map(|case| case.payload.as_ref())
            .collect(),
    }
}
fn visit(
    name: &str,
    graph: &Graph,
    resolved: &ResolvedModule,
    visiting: &mut BTreeSet<String>,
    heights: &mut BTreeMap<String, usize>,
    order: &mut Vec<usize>,
) -> Result<usize, Box<CompilerError>> {
    if let Some(height) = heights.get(name) {
        return Ok(*height);
    }
    let (index, dependencies) = &graph[name];
    let span = resolved.types[*index].source.span;
    if visiting.contains(name) {
        return Err(Box::new(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::InvalidCoreTypeIntegrity,
            "source type dependency cycle",
            span,
        )));
    }
    if visiting.len() >= MAX_CORE_TYPE_DEPTH {
        return Err(bound_failure(span, "source type dependency depth exceeded"));
    }
    visiting.insert(name.to_owned());
    let mut height = 1;
    for dependency in dependencies
        .iter()
        .filter(|dependency| graph.contains_key(*dependency))
    {
        height = height.max(1 + visit(dependency, graph, resolved, visiting, heights, order)?);
    }
    if height > MAX_CORE_TYPE_DEPTH {
        return Err(bound_failure(span, "source type dependency depth exceeded"));
    }
    visiting.remove(name);
    heights.insert(name.to_owned(), height);
    order.push(*index);
    Ok(height)
}
fn declaration_weight(decl: &TypeDecl, weights: &BTreeMap<String, u64>) -> Option<u64> {
    let mut total = u64::from(!matches!(decl.body, TypeExpr::Ref(_)));
    for ty in roots(&decl.body) {
        total = total.checked_add(reference_weight(ty, weights)?)?;
    }
    Some(total)
}
fn reference_weight(ty: &TypeRef, weights: &BTreeMap<String, u64>) -> Option<u64> {
    match ty {
        TypeRef::Named { path, args } if path.len() == 1 && args.is_empty() => {
            Some(weights.get(&path[0]).copied().unwrap_or(1))
        }
        TypeRef::Named { args, .. } => args.iter().try_fold(1_u64, |total, ty| {
            total.checked_add(reference_weight(ty, weights)?)
        }),
        TypeRef::Option(inner)
        | TypeRef::CapabilityRef(inner)
        | TypeRef::List { elem: inner, .. } => 1_u64.checked_add(reference_weight(inner, weights)?),
        TypeRef::Map { key, value, .. } => 1_u64
            .checked_add(reference_weight(key, weights)?)?
            .checked_add(reference_weight(value, weights)?),
        TypeRef::StringTy(_) | TypeRef::BytesTy(_) => Some(1),
    }
}
fn bound_failure(span: Span, message: &str) -> Box<CompilerError> {
    Box::new(error(
        CompilerStage::TypeCheck,
        CompilerErrorKind::InvalidBound,
        message,
        span,
    ))
}

// Named edges preserve authority but do not reset structural height. Inspect the
// expanded shape, including cached alias targets, before caching this declaration.
fn shape_fits_depth(shape: &TypeShape) -> bool {
    let mut pending = vec![(shape, 0_usize)];
    while let Some((shape, depth)) = pending.pop() {
        if depth > MAX_CORE_TYPE_DEPTH {
            return false;
        }
        match &shape.kind {
            TypeKind::Nominal { representation, .. } => pending.push((representation, depth + 1)),
            TypeKind::List { item, .. } => pending.push((item, depth + 1)),
            TypeKind::ExternalActionRequest { settlement } => pending.push((settlement, depth + 1)),
            TypeKind::Record(fields) => {
                pending.extend(fields.values().map(|field| (field, depth + 1)));
            }
            TypeKind::Bool
            | TypeKind::Int { .. }
            | TypeKind::String { .. }
            | TypeKind::Bytes { .. } => {}
        }
    }
    true
}
