//! Structured control flow in explicitly selected `echo.span-ir/v3`.
//!
//! These tests inspect public Target IR values, canonical bytes and stable
//! failure kinds/details. They make no runtime claim.

use super::*;
use edict_syntax::{
    TargetIrBranchEntry, TargetIrForEntry, TargetIrOrderEntry, ECHO_ORDERED_SPAN_IR_DOMAIN,
    ECHO_STRUCTURED_SPAN_IR_DOMAIN,
};

const LOOP_PRELUDE: &str = "package a.b@1;\n\
    type Leaf = { id: String<max=16>, };\n\
    type Group = { leaves: List<Leaf, max=2>, };\n\
    type Input = { id: String<max=16>, leaves: List<Leaf, max=4>, groups: List<Group, max=2>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.loop {\n";

const ENSURE_EACH_LEAF: &str = "  for leaf in input.leaves bounded 4 {\n\
        let receipt: Receipt = target.replace(leaf.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      }\n\
      return { id: input.id };\n\
    }";

const NESTED_LOOPS: &str = "  for group in input.groups bounded 2 {\n\
        for leaf in group.leaves bounded 2 {\n\
          let receipt: Receipt = target.replace(leaf.id)\n\
            else { rejected(reason) => domain.WriteRejected };\n\
        }\n\
      }\n\
      return { id: input.id };\n\
    }";

const BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH: &str = "  for leaf in input.leaves bounded 4 {\n\
        if leaf.id != \"\" {\n\
          let receipt: Receipt = target.replace(leaf.id)\n\
            else { rejected(reason) => domain.WriteRejected };\n\
        } else {\n\
          require leaf.id == \"\" else domain.WriteRejected;\n\
        }\n\
      }\n\
      if input.id != \"\" {\n\
        for leaf in input.leaves bounded 4 {\n\
          let observed: String<max=16> = leaf.id;\n\
        }\n\
      }\n\
      return { id: input.id };\n\
    }";

const REQUIREMENT_IN_LOOP: &str = "  let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      for leaf in input.leaves bounded 4 {\n\
        require leaf.id != receipt.id else continue obstructed {\n\
          reason: jim.EditObstruction.StaleBase,\n\
          provided: leaf.id,\n\
        };\n\
      }\n\
      return { id: receipt.id };\n\
    }";

const INDEPENDENT_LOOP_EFFECT: &str = "  for leaf in input.leaves bounded 4 {\n\
        let receipt: Receipt = target.replace(input.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      }\n\
      return { id: input.id };\n\
    }";

const SIBLING_LOOPS: &str = "  for leaf in input.leaves bounded 4 {\n\
        let first: Receipt = target.replace(leaf.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      }\n\
      for leaf in input.leaves bounded 4 {\n\
        let second: Receipt = target.replace(leaf.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      }\n\
      return { id: input.id };\n\
    }";

const SHADOWING_LOOP: &str = "  for leaf in input.leaves bounded 4 {\n\
        for leaf in input.leaves bounded 4 {\n\
          let receipt: Receipt = target.replace(leaf.id)\n\
            else { rejected(reason) => domain.WriteRejected };\n\
        }\n\
      }\n\
      return { id: input.id };\n\
    }";

const VALUE_BRANCH: &str =
    "  let chosen = if input.id != \"\" { yield input.id; } else { yield input.id; };\n\
      let receipt: Receipt = target.replace(chosen)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      return { id: input.id };\n\
    }";

fn source(body: &str) -> String {
    format!("{LOOP_PRELUDE}{body}")
}

fn loop_context() -> CompilerContext {
    effectful_context().with_budget(
        "p.loop",
        CoreBudget {
            max_steps: 64,
            max_allocated_bytes: 4096,
            max_output_bytes: 256,
        },
    )
}

fn facts_for(domain: &str) -> TargetIrLoweringFacts {
    let mut facts = echo_facts();
    domain.clone_into(&mut facts.target_ir_domain);
    facts
}

fn structured_facts() -> TargetIrLoweringFacts {
    facts_for(ECHO_STRUCTURED_SPAN_IR_DOMAIN)
}

fn loop_core(body: &str) -> CoreModule {
    let module = edict_syntax::parse_module(&source(body)).expect("loop source parses");
    compile_to_core(&module, &loop_context()).expect("loop source compiles")
}

fn structured_artifact(body: &str) -> TargetIrArtifact {
    let report = lower_to_target_ir(&loop_core(body), &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    report.artifact.expect("structured artifact")
}

fn order(artifact: &TargetIrArtifact) -> &[TargetIrOrderEntry] {
    artifact.intents["t"]
        .execution_order
        .as_deref()
        .expect("structured order")
}

fn order_mut(artifact: &mut TargetIrArtifact) -> &mut Vec<TargetIrOrderEntry> {
    artifact
        .intents
        .get_mut("t")
        .expect("intent")
        .execution_order
        .as_mut()
        .expect("structured order")
}

fn instruction(id: &str) -> TargetIrOrderEntry {
    TargetIrOrderEntry::Instruction(id.to_owned())
}

/// A compact rendering of the order tree: ids in pre-order with nesting.
fn shape(entries: &[TargetIrOrderEntry]) -> String {
    entries
        .iter()
        .map(|entry| match entry {
            TargetIrOrderEntry::Instruction(id) => id.clone(),
            TargetIrOrderEntry::For(entry) => format!("{}[{}]", entry.id, shape(&entry.body)),
            TargetIrOrderEntry::Branch(entry) => format!(
                "{}[{}|{}]",
                entry.id,
                shape(&entry.then_entries),
                shape(&entry.else_entries)
            ),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn for_entry(entry: &TargetIrOrderEntry) -> &TargetIrForEntry {
    let TargetIrOrderEntry::For(entry) = entry else {
        panic!("for entry expected");
    };
    entry
}

fn for_entry_mut(entry: &mut TargetIrOrderEntry) -> &mut TargetIrForEntry {
    let TargetIrOrderEntry::For(entry) = entry else {
        panic!("for entry expected");
    };
    entry
}

fn branch_entry_mut(entry: &mut TargetIrOrderEntry) -> &mut TargetIrBranchEntry {
    let TargetIrOrderEntry::Branch(entry) = entry else {
        panic!("branch entry expected");
    };
    entry
}

fn assert_unsupported_value(artifact: &TargetIrArtifact, case: &str) {
    assert_eq!(
        encode_target_ir_artifact(artifact).expect_err(case).kind(),
        CanonicalErrorKind::UnsupportedValue,
        "{case}"
    );
}

fn assert_refused(
    report: &edict_syntax::TargetLoweringReport,
    kind: TargetLoweringFailureKind,
    detail: Option<&str>,
    case: &str,
) {
    assert_eq!(report.status, TargetLoweringStatus::Unsupported, "{case}");
    assert!(report.artifact.is_none(), "{case}");
    assert_eq!(
        report.failures[0].kind, kind,
        "{case}: {:?}",
        report.failures
    );
    if let Some(detail) = detail {
        assert_eq!(report.failures[0].detail, detail, "{case}");
    }
}

fn encoded_intent(artifact: &TargetIrArtifact) -> CanonicalValue {
    let bytes = encode_target_ir_artifact(artifact).expect("structured encoding");
    let value = decode_canonical_cbor(&bytes).expect("canonical decoding");
    let CanonicalValue::Map(entries) = &value else {
        panic!("artifact map");
    };
    let kind = entries
        .iter()
        .find_map(|(key, value)| (key == &CanonicalValue::Text("kind".to_owned())).then_some(value))
        .expect("kind");
    assert_eq!(
        kind,
        &CanonicalValue::Text("structuredTargetIrArtifact".to_owned())
    );
    let intents = entries
        .iter()
        .find_map(|(key, value)| {
            (key == &CanonicalValue::Text("intents".to_owned())).then_some(value)
        })
        .expect("intents");
    let CanonicalValue::Map(intents) = intents else {
        panic!("intent map");
    };
    intents[0].1.clone()
}

fn field<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("map expected");
    };
    entries
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.to_owned())).then_some(value))
        .unwrap_or_else(|| panic!("map key `{key}` exists"))
}

fn rename_expr(expr: &mut CoreExpr, from: &str, to: &LocalRef) {
    match expr {
        CoreExpr::Local { reference } if reference.id == from => *reference = to.clone(),
        CoreExpr::Local { .. } | CoreExpr::Const(_) => {}
        CoreExpr::Record { fields } => fields
            .values_mut()
            .for_each(|value| rename_expr(value, from, to)),
        CoreExpr::List { values } => values
            .iter_mut()
            .for_each(|value| rename_expr(value, from, to)),
        CoreExpr::Field { base, .. } => rename_expr(base, from, to),
        CoreExpr::Call { args, .. } => args
            .iter_mut()
            .for_each(|value| rename_expr(value, from, to)),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            rename_predicate(predicate, from, to);
            rename_expr(then_value, from, to);
            rename_expr(else_value, from, to);
        }
    }
}

fn rename_predicate(predicate: &mut CorePredicate, from: &str, to: &LocalRef) {
    match predicate {
        CorePredicate::True | CorePredicate::False => {}
        CorePredicate::Not(value) => rename_predicate(value, from, to),
        CorePredicate::All(values) | CorePredicate::Any(values) => values
            .iter_mut()
            .for_each(|value| rename_predicate(value, from, to)),
        CorePredicate::Compare { left, right, .. } => {
            rename_expr(left, from, to);
            rename_expr(right, from, to);
        }
    }
}

fn rename_block(block: &mut CoreBlock, from: &str, to: &LocalRef) {
    for local in &mut block.locals {
        if local.id == from {
            *local = to.clone();
        }
    }
    for node in &mut block.nodes {
        match node {
            CoreNode::Let { value, .. } => rename_expr(value, from, to),
            CoreNode::Effect { input, .. } => rename_expr(input, from, to),
            CoreNode::Require { predicate, .. } => rename_predicate(predicate, from, to),
            CoreNode::For {
                binder, iter, body, ..
            } => {
                if binder.id == from {
                    *binder = to.clone();
                }
                rename_expr(iter, from, to);
                rename_block(body, from, to);
            }
            CoreNode::Branch {
                predicate,
                then_block,
                else_block,
                ..
            } => {
                rename_predicate(predicate, from, to);
                rename_block(then_block, from, to);
                rename_block(else_block, from, to);
            }
            CoreNode::ExternalActionRequest { .. } => {}
        }
    }
    rename_expr(&mut block.result, from, to);
}

fn core_loop(
    core: &mut CoreModule,
    index: usize,
) -> (&mut LocalRef, &mut CoreExpr, &mut CoreBlock) {
    let CoreNode::For {
        binder, iter, body, ..
    } = &mut core.intents.get_mut("t").expect("intent").body.nodes[index]
    else {
        panic!("Core for expected");
    };
    (binder, iter, body)
}

#[test]
fn structured_loop_emits_one_effect_step_per_element() {
    let core = loop_core(ENSURE_EACH_LEAF);
    let artifact = structured_artifact(ENSURE_EACH_LEAF);
    assert_eq!(artifact.domain, ECHO_STRUCTURED_SPAN_IR_DOMAIN);
    assert_eq!(shape(order(&artifact)), "t.for.0[t.step.0]");
    let intent = &artifact.intents["t"];
    assert_eq!(intent.steps.len(), 1);
    assert_eq!(intent.steps[0].target_intrinsic, "echo.dpo@1.replace");
    assert_eq!(
        intent.core_evaluation_budget,
        core.intents["t"].core_evaluation_budget
    );

    let CoreNode::For {
        binder,
        iter,
        bound,
        ..
    } = &core.intents["t"].body.nodes[0]
    else {
        panic!("Core for");
    };
    let entry = for_entry(&order(&artifact)[0]);
    assert_eq!(&entry.binder, binder);
    assert_eq!(&entry.iterable, iter);
    assert_eq!(&entry.bound, bound);
    assert_eq!(entry.bound, CoreBound::Literal(4));

    let encoded = encoded_intent(&artifact);
    let CanonicalValue::Array(entries) = field(&encoded, "executionOrder") else {
        panic!("order array");
    };
    assert_eq!(
        field(&entries[0], "kind"),
        &CanonicalValue::Text("for".to_owned())
    );
    assert_eq!(
        field(&entries[0], "id"),
        &CanonicalValue::Text("t.for.0".to_owned())
    );
    assert_eq!(
        field(&entries[0], "body"),
        &CanonicalValue::Array(vec![CanonicalValue::Text("t.step.0".to_owned())])
    );
    field(&entries[0], "iterable");
    field(&entries[0], "binder");
    field(&entries[0], "bound");
}

#[test]
fn nested_loops_lower_as_nested_entries() {
    let artifact = structured_artifact(NESTED_LOOPS);
    assert_eq!(shape(order(&artifact)), "t.for.0[t.for.1[t.step.0]]");
    let outer = for_entry(&order(&artifact)[0]);
    let inner = for_entry(&outer.body[0]);
    assert_eq!(inner.bound, CoreBound::Literal(2));
    assert!(matches!(
        &inner.iterable,
        CoreExpr::Field { base, .. }
            if matches!(base.as_ref(), CoreExpr::Local { reference } if reference == &outer.binder)
    ));
}

#[test]
fn branch_inside_loop_and_loop_inside_branch_keep_structure() {
    let artifact = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    assert_eq!(
        shape(order(&artifact)),
        "t.for.0[t.branch.0[t.step.0|t.require.0]],t.branch.1[t.for.1[t.binding.0]|]"
    );
    let intent = &artifact.intents["t"];
    assert_eq!(intent.pure_bindings.len(), 1);
    assert!(artifact.semantic_closure.is_some());
    encode_target_ir_artifact(&artifact).expect("nested structure encodes");
}

/// Branch predicates that each contain an empty `all` or `any` somewhere the
/// Core schema requires one or more values, built around a valid predicate.
fn empty_aggregate_predicates(valid: &CorePredicate) -> Vec<(&'static str, CorePredicate)> {
    let CorePredicate::Compare { left, right, .. } = valid else {
        panic!("comparison predicate expected");
    };
    vec![
        ("empty all", CorePredicate::All(Vec::new())),
        ("empty any", CorePredicate::Any(Vec::new())),
        (
            "empty all inside not",
            CorePredicate::Not(Box::new(CorePredicate::All(Vec::new()))),
        ),
        (
            "empty any inside all",
            CorePredicate::All(vec![valid.clone(), CorePredicate::Any(Vec::new())]),
        ),
        (
            "empty all inside not inside any",
            CorePredicate::Any(vec![
                valid.clone(),
                CorePredicate::Not(Box::new(CorePredicate::All(Vec::new()))),
            ]),
        ),
        (
            "empty any inside an if operand",
            CorePredicate::Compare {
                op: CompareOp::Ne,
                left: CoreExpr::If {
                    predicate: Box::new(CorePredicate::Any(Vec::new())),
                    then_value: Box::new(left.clone()),
                    else_value: Box::new(left.clone()),
                },
                right: right.clone(),
            },
        ),
    ]
}

fn nonempty_aggregate_predicates(valid: &CorePredicate) -> Vec<(&'static str, CorePredicate)> {
    vec![
        ("singleton all", CorePredicate::All(vec![valid.clone()])),
        (
            "nested any",
            CorePredicate::Any(vec![
                valid.clone(),
                CorePredicate::Not(Box::new(CorePredicate::All(vec![valid.clone()]))),
            ]),
        ),
    ]
}

fn core_branch_predicate(core: &mut CoreModule, in_loop: bool) -> &mut CorePredicate {
    let nodes = &mut core.intents.get_mut("t").expect("intent").body.nodes;
    let node = if in_loop {
        let CoreNode::For { body, .. } = &mut nodes[0] else {
            panic!("Core for expected");
        };
        &mut body.nodes[0]
    } else {
        &mut nodes[1]
    };
    let CoreNode::Branch { predicate, .. } = node else {
        panic!("Core branch expected");
    };
    predicate
}

#[test]
fn branch_predicates_with_empty_aggregates_refuse_in_lowering() {
    let baseline = loop_core(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    for in_loop in [false, true] {
        let valid = core_branch_predicate(&mut baseline.clone(), in_loop).clone();
        for (case, predicate) in empty_aggregate_predicates(&valid) {
            let mut core = baseline.clone();
            *core_branch_predicate(&mut core, in_loop) = predicate;
            let report = lower_to_target_ir(&core, &structured_facts());
            assert_refused(
                &report,
                TargetLoweringFailureKind::InvalidCoreIdentity,
                Some("empty_predicate_aggregate"),
                &format!("{case}, in_loop={in_loop}"),
            );
        }
        for (case, predicate) in nonempty_aggregate_predicates(&valid) {
            let mut core = baseline.clone();
            *core_branch_predicate(&mut core, in_loop) = predicate;
            let report = lower_to_target_ir(&core, &structured_facts());
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "{case}, in_loop={in_loop}: {report:?}"
            );
            encode_target_ir_artifact(&report.artifact.expect("artifact"))
                .unwrap_or_else(|error| panic!("{case}, in_loop={in_loop}: {error:?}"));
        }
    }
}

#[test]
fn branch_predicates_with_empty_aggregates_refuse_in_the_encoder() {
    let baseline = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    for in_loop in [false, true] {
        let valid = branch_predicate_mut(&mut baseline.clone(), in_loop).clone();
        for (case, predicate) in empty_aggregate_predicates(&valid) {
            let mut artifact = baseline.clone();
            *branch_predicate_mut(&mut artifact, in_loop) = predicate;
            assert_unsupported_value(&artifact, &format!("{case}, in_loop={in_loop}"));
        }
        for (case, predicate) in nonempty_aggregate_predicates(&valid) {
            let mut artifact = baseline.clone();
            *branch_predicate_mut(&mut artifact, in_loop) = predicate;
            encode_target_ir_artifact(&artifact)
                .unwrap_or_else(|error| panic!("{case}, in_loop={in_loop}: {error:?}"));
        }
    }
}

fn branch_predicate_mut(artifact: &mut TargetIrArtifact, in_loop: bool) -> &mut CorePredicate {
    let order = order_mut(artifact);
    let entry = if in_loop {
        &mut for_entry_mut(&mut order[0]).body[0]
    } else {
        &mut order[1]
    };
    &mut branch_entry_mut(entry).predicate
}

/// Replace the loop in `REQUIREMENT_IN_LOOP` (after its top-level effect) with
/// `levels` nested statement branches or loops whose innermost block is empty.
fn nested_control_core(levels: usize, loops: bool) -> CoreModule {
    let mut core = loop_core(REQUIREMENT_IN_LOOP);
    let nodes = &mut core.intents.get_mut("t").expect("intent").body.nodes;
    let CoreNode::For {
        binder,
        iter,
        bound,
        ..
    } = nodes[1].clone()
    else {
        panic!("Core for expected");
    };
    let empty = || CoreBlock {
        locals: Vec::new(),
        nodes: Vec::new(),
        result: CoreExpr::Const(CoreValue::Null),
    };
    let mut block = empty();
    for level in (0..levels).rev() {
        let node = if loops {
            let binder = LocalRef {
                id: format!("local.nest.{level}"),
                alpha_name: format!("$nest{level}"),
                ty: binder.ty.clone(),
            };
            block.locals.push(binder.clone());
            CoreNode::For {
                binder,
                iter: iter.clone(),
                bound: bound.clone(),
                body: block,
            }
        } else {
            CoreNode::Branch {
                binding: None,
                predicate: CorePredicate::True,
                then_block: block,
                else_block: empty(),
            }
        };
        block = empty();
        block.nodes.push(node);
    }
    nodes[1] = block.nodes.pop().expect("outermost control");
    core
}

#[test]
fn structured_nesting_refuses_at_lowering_beyond_the_canonical_depth() {
    for (kind, loops, deepest) in [("branches", false, 62), ("loops", true, 61)] {
        let report = lower_to_target_ir(&nested_control_core(deepest, loops), &structured_facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{deepest} {kind}: {report:?}"
        );
        let artifact = report.artifact.expect("deepest artifact");
        assert!(
            artifact.semantic_closure.is_none(),
            "{kind} stay closure-free"
        );
        encode_target_ir_artifact(&artifact)
            .unwrap_or_else(|error| panic!("{deepest} {kind} encode: {error:?}"));

        let report = lower_to_target_ir(
            &nested_control_core(deepest + 1, loops),
            &structured_facts(),
        );
        assert_refused(
            &report,
            TargetLoweringFailureKind::UnsupportedCoreNode,
            Some("canonical_nesting_depth"),
            &format!("{} {kind}", deepest + 1),
        );
        assert_eq!(report.failures[0].intent.as_deref(), Some("t"), "{kind}");
    }
}

const TOP_LEVEL_REQUIREMENT: &str = "  let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      require receipt.id != \"\" else domain.WriteRejected;\n\
      return { id: receipt.id };\n\
    }";

/// The Core requirement predicate: top-level (node 1) or inside the loop body.
fn core_require_predicate(core: &mut CoreModule, in_loop: bool) -> &mut CorePredicate {
    let nodes = &mut core.intents.get_mut("t").expect("intent").body.nodes;
    let node = if in_loop {
        let CoreNode::For { body, .. } = &mut nodes[1] else {
            panic!("Core for expected");
        };
        &mut body.nodes[0]
    } else {
        &mut nodes[1]
    };
    let CoreNode::Require { predicate, .. } = node else {
        panic!("Core require expected");
    };
    predicate
}

#[test]
fn requirement_predicates_with_empty_aggregates_refuse_under_v3() {
    for (source, in_loop) in [(TOP_LEVEL_REQUIREMENT, false), (REQUIREMENT_IN_LOOP, true)] {
        let baseline = loop_core(source);
        let valid = core_require_predicate(&mut baseline.clone(), in_loop).clone();
        let artifact = structured_artifact(source);
        for (case, predicate) in empty_aggregate_predicates(&valid) {
            let case = format!("{case}, in_loop={in_loop}");
            let mut core = baseline.clone();
            *core_require_predicate(&mut core, in_loop) = predicate.clone();
            let report = lower_to_target_ir(&core, &structured_facts());
            assert_refused(
                &report,
                TargetLoweringFailureKind::InvalidCoreIdentity,
                Some("empty_predicate_aggregate"),
                &case,
            );
            let mut encoded = artifact.clone();
            encoded.intents.get_mut("t").expect("intent").requirements[0].predicate = predicate;
            assert_unsupported_value(&encoded, &case);
        }
        for (case, predicate) in nonempty_aggregate_predicates(&valid) {
            let mut core = baseline.clone();
            *core_require_predicate(&mut core, in_loop) = predicate;
            let report = lower_to_target_ir(&core, &structured_facts());
            assert_eq!(
                report.status,
                TargetLoweringStatus::Lowered,
                "{case}: {report:?}"
            );
            encode_target_ir_artifact(&report.artifact.expect("artifact"))
                .unwrap_or_else(|error| panic!("{case}: {error:?}"));
        }
    }

    // v2 requirement encoding is unchanged: the same predicate still encodes.
    let report = lower_to_target_ir(
        &loop_core(TOP_LEVEL_REQUIREMENT),
        &facts_for(ECHO_ORDERED_SPAN_IR_DOMAIN),
    );
    let mut ordered = report.artifact.expect("v2 artifact");
    ordered.intents.get_mut("t").expect("intent").requirements[0].predicate =
        CorePredicate::All(Vec::new());
    encode_target_ir_artifact(&ordered).expect("v2 requirement encoding is unchanged");
}

fn conditional_iterable(predicate: CorePredicate, iterable: &CoreExpr) -> CoreExpr {
    CoreExpr::If {
        predicate: Box::new(predicate),
        then_value: Box::new(iterable.clone()),
        else_value: Box::new(iterable.clone()),
    }
}

/// The Core loop at the top level (node 0) or inside the top-level branch.
fn core_loop_iterable(core: &mut CoreModule, in_branch: bool) -> &mut CoreExpr {
    let nodes = &mut core.intents.get_mut("t").expect("intent").body.nodes;
    let node = if in_branch {
        let CoreNode::Branch { then_block, .. } = &mut nodes[1] else {
            panic!("Core branch expected");
        };
        &mut then_block.nodes[0]
    } else {
        &mut nodes[0]
    };
    let CoreNode::For { iter, .. } = node else {
        panic!("Core for expected");
    };
    iter
}

#[test]
fn loop_iterables_with_empty_aggregates_refuse_under_v3() {
    let baseline = loop_core(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    let artifact = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    for in_branch in [false, true] {
        let iterable = core_loop_iterable(&mut baseline.clone(), in_branch).clone();
        for (case, predicate) in [
            ("empty all", CorePredicate::All(Vec::new())),
            ("empty any", CorePredicate::Any(Vec::new())),
            (
                "empty any inside not",
                CorePredicate::Not(Box::new(CorePredicate::Any(Vec::new()))),
            ),
        ] {
            let case = format!("{case} iterable, in_branch={in_branch}");
            let conditional = conditional_iterable(predicate, &iterable);
            let mut core = baseline.clone();
            *core_loop_iterable(&mut core, in_branch) = conditional.clone();
            let report = lower_to_target_ir(&core, &structured_facts());
            assert_refused(
                &report,
                TargetLoweringFailureKind::InvalidCoreIdentity,
                Some("empty_predicate_aggregate"),
                &case,
            );

            let mut encoded = artifact.clone();
            let order = order_mut(&mut encoded);
            let entry = if in_branch {
                &mut branch_entry_mut(&mut order[1]).then_entries[0]
            } else {
                &mut order[0]
            };
            for_entry_mut(entry).iterable = conditional;
            assert_unsupported_value(&encoded, &case);
        }

        // Control: this fixture's `Leaf` record iterable does not lower when
        // wrapped in a conditional, so the encoder is the control here (a
        // nonempty aggregate encodes); a conditional scalar-list iterable that
        // does lower is covered by `conditional_scalar_list_iterable_lowers_under_v3`.
        let mut encoded = artifact.clone();
        let order = order_mut(&mut encoded);
        let entry = if in_branch {
            &mut branch_entry_mut(&mut order[1]).then_entries[0]
        } else {
            &mut order[0]
        };
        for_entry_mut(entry).iterable =
            conditional_iterable(CorePredicate::All(vec![CorePredicate::True]), &iterable);
        encode_target_ir_artifact(&encoded)
            .unwrap_or_else(|error| panic!("nonempty iterable, in_branch={in_branch}: {error:?}"));
    }
}

/// `ENSURE_EACH_LEAF` rewritten as a raw-Core loop over `[null]` whose binder is
/// `binder`; the effect reads `input.id` instead of the loop item.
fn loop_with_binder(binder: impl FnOnce(&LocalRef) -> LocalRef) -> CoreModule {
    let mut core = loop_core(ENSURE_EACH_LEAF);
    let (old_binder, iter, body) = core_loop(&mut core, 0);
    let CoreExpr::Field { base: input, .. } = iter.clone() else {
        panic!("field iterable expected");
    };
    *iter = CoreExpr::List {
        values: vec![CoreExpr::Const(CoreValue::Null)],
    };
    let CoreNode::Effect {
        input: effect_input,
        obstruction_map,
        ..
    } = &mut body.nodes[0]
    else {
        panic!("Core effect expected");
    };
    *effect_input = CoreExpr::Field {
        base: input,
        field: "id".to_owned(),
    };
    let arm_binder = obstruction_map
        .values()
        .next()
        .expect("obstruction arm")
        .binder
        .clone();
    assert_eq!(arm_binder.ty, "Unit", "the arm binder is a Unit local");
    let new_binder = binder(&arm_binder);
    let old_id = old_binder.id.clone();
    body.locals.retain(|local| local.id != old_id);
    if !body.locals.contains(&new_binder) {
        body.locals.push(new_binder.clone());
    }
    *old_binder = new_binder;
    core
}

#[test]
fn obstruction_binders_must_be_fresh_under_v3() {
    // Control: a fresh Unit binder over `[null]` lowers and encodes.
    let fresh = loop_with_binder(|arm| LocalRef {
        id: "local.fresh".to_owned(),
        alpha_name: "$fresh".to_owned(),
        ty: arm.ty.clone(),
    });
    let report = lower_to_target_ir(&fresh, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    encode_target_ir_artifact(&report.artifact.expect("artifact")).expect("fresh binder encodes");

    // The loop binder is also the nested effect's obstruction binder.
    let shared = loop_with_binder(Clone::clone);
    let report = lower_to_target_ir(&shared, &structured_facts());
    assert_refused(
        &report,
        TargetLoweringFailureKind::InvalidCoreIdentity,
        Some("obstruction_binder_not_fresh"),
        "loop binder reused as an obstruction binder",
    );
}

#[test]
fn obstruction_binder_reusing_the_application_input_refuses_under_v3() {
    // Type-correct application-input reuse: the input is `Unit`, like the
    // obstruction binder, so only the binder's identity differs.
    let control = unit_input_core(None);
    let report = lower_to_target_ir(&control, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    encode_target_ir_artifact(&report.artifact.expect("artifact"))
        .expect("Unit-input control encodes");
    let report = lower_to_target_ir(&unit_input_core(Some("arg.0")), &structured_facts());
    assert_refused(
        &report,
        TargetLoweringFailureKind::InvalidCoreIdentity,
        Some("obstruction_binder_not_fresh"),
        "obstruction binder reusing the application input",
    );
}

/// The `[null]` loop with a `Unit` application input, a constant effect input
/// and result, and a `Unit` obstruction binder whose identity is `arm_id`
/// when given (otherwise its own fresh identity).
fn unit_input_core(arm_id: Option<&str>) -> CoreModule {
    let mut core = loop_with_binder(|arm| LocalRef {
        id: "local.fresh".to_owned(),
        alpha_name: "$fresh".to_owned(),
        ty: arm.ty.clone(),
    });
    let intent = core.intents.get_mut("t").expect("intent");
    "Unit".clone_into(&mut intent.input);
    let input = intent
        .body
        .locals
        .iter_mut()
        .find(|local| local.id == "arg.0")
        .expect("input local");
    "Unit".clone_into(&mut input.ty);
    let input = input.clone();
    intent.body.result = CoreExpr::Record {
        fields: BTreeMap::from([(
            "id".to_owned(),
            CoreExpr::Const(CoreValue::String("x".to_owned())),
        )]),
    };
    let (_, _, body) = core_loop(&mut core, 0);
    let CoreNode::Effect {
        input: effect_input,
        obstruction_map,
        ..
    } = &mut body.nodes[0]
    else {
        panic!("Core effect expected");
    };
    *effect_input = CoreExpr::Const(CoreValue::String("x".to_owned()));
    if let Some(id) = arm_id {
        let arm = obstruction_map.values_mut().next().expect("arm");
        assert_eq!(arm.binder.ty, input.ty, "only the identity changes");
        let old = arm.binder.id.clone();
        arm.binder = LocalRef {
            id: id.to_owned(),
            alpha_name: input.alpha_name.clone(),
            ty: arm.binder.ty.clone(),
        };
        let binder = arm.binder.clone();
        body.locals.retain(|local| local.id != old);
        body.locals.push(binder);
    }
    core
}

const SIBLING_ARM_EFFECTS: &str = "  if input.id != \"\" {\n\
        let first: Receipt = target.replace(input.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      } else {\n\
        let second: Receipt = target.replace(input.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      }\n\
      return { id: input.id };\n\
    }";

/// The single effect node of a block, mutably.
fn block_effect(block: &mut CoreBlock) -> &mut CoreNode {
    block
        .nodes
        .iter_mut()
        .find(|node| matches!(node, CoreNode::Effect { .. }))
        .expect("Core effect in block")
}

fn arm_binder(node: &CoreNode) -> LocalRef {
    let CoreNode::Effect {
        obstruction_map, ..
    } = node
    else {
        panic!("Core effect expected");
    };
    obstruction_map
        .values()
        .next()
        .expect("obstruction arm")
        .binder
        .clone()
}

/// Replace a block's effect obstruction binder (and its local-table entry).
fn reuse_arm_binder(block: &mut CoreBlock, binder: &LocalRef) {
    let old = arm_binder(block_effect(block));
    let CoreNode::Effect {
        obstruction_map, ..
    } = block_effect(block)
    else {
        unreachable!()
    };
    obstruction_map.values_mut().next().expect("arm").binder = binder.clone();
    block.locals.retain(|local| local.id != old.id);
    block.locals.push(binder.clone());
}

/// `SIBLING_LOOPS` whose second loop is raw Core over `[null]` with `binder`.
fn sibling_loops_with_second_binder(binder: impl FnOnce(&LocalRef) -> LocalRef) -> CoreModule {
    let mut core = loop_core(SIBLING_LOOPS);
    let (_, _, first_body) = core_loop(&mut core, 0);
    let first_arm = arm_binder(block_effect(first_body));
    let (second_binder, iter, body) = core_loop(&mut core, 1);
    let CoreExpr::Field { base: input, .. } = iter.clone() else {
        panic!("field iterable expected");
    };
    *iter = CoreExpr::List {
        values: vec![CoreExpr::Const(CoreValue::Null)],
    };
    let CoreNode::Effect {
        input: effect_input,
        ..
    } = block_effect(body)
    else {
        unreachable!()
    };
    *effect_input = CoreExpr::Field {
        base: input,
        field: "id".to_owned(),
    };
    let new_binder = binder(&first_arm);
    let old_id = second_binder.id.clone();
    body.locals.retain(|local| local.id != old_id);
    body.locals.push(new_binder.clone());
    *second_binder = new_binder;
    core
}

#[test]
fn v3_binders_are_unique_across_the_intent() {
    // Controls: fresh binders lower and encode.
    let fresh_unit = |arm: &LocalRef| LocalRef {
        id: "local.fresh".to_owned(),
        alpha_name: "$fresh".to_owned(),
        ty: arm.ty.clone(),
    };
    let mut controls = vec![
        ("sibling loops", loop_core(SIBLING_LOOPS)),
        ("sibling arms", loop_core(SIBLING_ARM_EFFECTS)),
        (
            "raw second loop",
            sibling_loops_with_second_binder(fresh_unit),
        ),
    ]
    .into_iter()
    .map(|(case, core)| {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{case}: {report:?}"
        );
        let artifact = report.artifact.expect("artifact");
        encode_target_ir_artifact(&artifact).unwrap_or_else(|error| panic!("{case}: {error:?}"));
        artifact
    });
    let sibling_loops = controls.next().expect("sibling loops");
    let sibling_arms = controls.next().expect("sibling arms");
    let raw_second_loop = controls.next().expect("raw second loop");

    // Lowering: the second sibling loop's effect reuses the first's binder.
    let mut loops = loop_core(SIBLING_LOOPS);
    let (_, _, first_body) = core_loop(&mut loops, 0);
    let first_arm = arm_binder(block_effect(first_body));
    let (_, _, second_body) = core_loop(&mut loops, 1);
    reuse_arm_binder(second_body, &first_arm);

    // Lowering: the else arm's effect reuses the then arm's binder.
    let mut arms = loop_core(SIBLING_ARM_EFFECTS);
    let CoreNode::Branch {
        then_block,
        else_block,
        ..
    } = &mut arms.intents.get_mut("t").expect("intent").body.nodes[0]
    else {
        panic!("Core branch expected");
    };
    let then_arm = arm_binder(block_effect(then_block));
    reuse_arm_binder(else_block, &then_arm);

    // Lowering: a later loop binder reuses an earlier obstruction binder.
    let later_loop = sibling_loops_with_second_binder(Clone::clone);

    for (case, core) in [
        ("sibling loop obstruction binder", loops),
        ("sibling arm obstruction binder", arms),
        ("later loop binder", later_loop),
    ] {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            None,
            case,
        );
    }

    // Encoder: the same reuses in hand-built artifacts.
    let first_step_arm = |artifact: &TargetIrArtifact| {
        artifact.intents["t"].steps[0]
            .obstruction_arms
            .values()
            .next()
            .expect("arm")
            .binder
            .clone()
    };
    let reuse_second_step_arm = |artifact: &TargetIrArtifact| {
        let mut changed = artifact.clone();
        let binder = first_step_arm(artifact);
        changed.intents.get_mut("t").expect("intent").steps[1]
            .obstruction_arms
            .values_mut()
            .next()
            .expect("arm")
            .binder = binder;
        changed
    };
    let mut later_binder = raw_second_loop.clone();
    let binder = first_step_arm(&raw_second_loop);
    for_entry_mut(&mut order_mut(&mut later_binder)[1]).binder = binder;
    let mut shared_producer = sibling_loops.clone();
    let first_binding = shared_producer.intents["t"].steps[0].binding.clone();
    shared_producer.intents.get_mut("t").expect("intent").steps[1].binding = first_binding;
    for (case, artifact) in [
        (
            "sibling loop obstruction binder",
            reuse_second_step_arm(&sibling_loops),
        ),
        (
            "sibling arm obstruction binder",
            reuse_second_step_arm(&sibling_arms),
        ),
        ("later loop binder", later_binder),
        ("sibling loop step binding", shared_producer),
    ] {
        assert_unsupported_value(&artifact, case);
    }
}

/// A loop over one of two `List<U64, max=4>` fields chosen by a conditional.
const CONDITIONAL_SCALAR_LIST: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, nums: List<U64, max=4>, others: List<U64, max=4>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.loop {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      for n in input.nums bounded 4 {\n\
        let seen: U64 = n;\n\
      }\n\
      return { id: input.id };\n\
    }";

#[test]
fn conditional_scalar_list_iterable_lowers_under_v3() {
    let module = edict_syntax::parse_module(CONDITIONAL_SCALAR_LIST).expect("source parses");
    let baseline = compile_to_core(&module, &loop_context()).expect("source compiles");
    let with_predicate = |predicate: CorePredicate| {
        let mut core = baseline.clone();
        let (_, iter, _) = core_loop(&mut core, 1);
        let CoreExpr::Field { base: input, .. } = iter.clone() else {
            panic!("field iterable expected");
        };
        *iter = CoreExpr::If {
            predicate: Box::new(predicate),
            then_value: Box::new(iter.clone()),
            else_value: Box::new(CoreExpr::Field {
                base: input,
                field: "others".to_owned(),
            }),
        };
        core
    };

    let report = lower_to_target_ir(
        &with_predicate(CorePredicate::All(vec![CorePredicate::True])),
        &structured_facts(),
    );
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let artifact = report.artifact.expect("conditional iterable artifact");
    assert!(matches!(
        for_entry(&order(&artifact)[1]).iterable,
        CoreExpr::If { .. }
    ));
    encode_target_ir_artifact(&artifact).expect("conditional iterable encodes");

    for (case, predicate) in [
        ("empty all", CorePredicate::All(Vec::new())),
        ("empty any", CorePredicate::Any(Vec::new())),
    ] {
        let report = lower_to_target_ir(&with_predicate(predicate), &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            Some("empty_predicate_aggregate"),
            case,
        );
    }
}

const COMPARISONS: &str = "package a.b@1;\n\
    type Leaf = { id: String<max=16>, };\n\
    type Input = { id: String<max=16>, tag: Bytes<max=8>, n: U64, leaves: List<Leaf, max=4>, nums: List<U64, max=4>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.loop {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      if input.tag == input.tag {\n\
        require input.n == input.n else domain.WriteRejected;\n\
      }\n\
      for leaf in input.leaves bounded 4 {\n\
        if input.tag != input.tag {\n\
          require input.n == input.n else domain.WriteRejected;\n\
        }\n\
      }\n\
      for x in input.nums bounded 4 {\n\
        let seen: U64 = x;\n\
      }\n\
      return { id: input.id };\n\
    }";

fn input_field(core: &CoreModule, field: &str) -> CoreExpr {
    let input = core.intents["t"]
        .body
        .locals
        .iter()
        .find(|local| local.id == "arg.0")
        .expect("input local")
        .clone();
    CoreExpr::Field {
        base: Box::new(CoreExpr::Local { reference: input }),
        field: field.to_owned(),
    }
}

fn compare(op: CompareOp, left: CoreExpr, right: CoreExpr) -> CorePredicate {
    CorePredicate::Compare { op, left, right }
}

/// The top-level branch (node 1) or the branch inside the first loop (node 2).
fn comparison_branch(core: &mut CoreModule, nested: bool) -> &mut CoreNode {
    let nodes = &mut core.intents.get_mut("t").expect("intent").body.nodes;
    if nested {
        let CoreNode::For { body, .. } = &mut nodes[2] else {
            panic!("Core for expected");
        };
        &mut body.nodes[0]
    } else {
        &mut nodes[1]
    }
}

fn set_branch_predicate(core: &mut CoreModule, nested: bool, predicate: CorePredicate) {
    let CoreNode::Branch { predicate: old, .. } = comparison_branch(core, nested) else {
        panic!("Core branch expected");
    };
    *old = predicate;
}

fn comparisons_core() -> CoreModule {
    let module = edict_syntax::parse_module(COMPARISONS).expect("source parses");
    compile_to_core(&module, &loop_context()).expect("source compiles")
}

#[test]
fn v3_ordered_comparisons_require_integer_operands() {
    let baseline = comparisons_core();
    let tag = input_field(&baseline, "tag");
    let n = input_field(&baseline, "n");
    let ordered = [CompareOp::Lt, CompareOp::Le, CompareOp::Gt, CompareOp::Ge];

    // Controls: byte equality as compiled, and integer ordering, lower.
    let report = lower_to_target_ir(&baseline, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let artifact = report.artifact.expect("artifact");
    encode_target_ir_artifact(&artifact).expect("byte equality encodes");
    for nested in [false, true] {
        let mut core = baseline.clone();
        set_branch_predicate(
            &mut core,
            nested,
            compare(CompareOp::Lt, n.clone(), n.clone()),
        );
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "integer ordering, nested={nested}: {report:?}"
        );
    }
    let mut integer_iterable = baseline.clone();
    let (_, iter, _) = core_loop(&mut integer_iterable, 3);
    *iter = CoreExpr::If {
        predicate: Box::new(compare(CompareOp::Ge, n.clone(), n.clone())),
        then_value: Box::new(iter.clone()),
        else_value: Box::new(iter.clone()),
    };
    let report = lower_to_target_ir(&integer_iterable, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");

    // Lowering: byte ordering in branch, requirement and iterable predicates.
    let mut cases = Vec::new();
    for op in ordered {
        let mut core = baseline.clone();
        set_branch_predicate(&mut core, false, compare(op, tag.clone(), tag.clone()));
        cases.push((format!("top-level branch {op:?}"), core));
    }
    let mut nested = baseline.clone();
    set_branch_predicate(
        &mut nested,
        true,
        compare(CompareOp::Ge, tag.clone(), tag.clone()),
    );
    cases.push(("nested branch Ge".to_owned(), nested));
    let mut requirement = baseline.clone();
    let CoreNode::Branch { then_block, .. } = comparison_branch(&mut requirement, false) else {
        panic!("Core branch expected");
    };
    let CoreNode::Require { predicate, .. } = &mut then_block.nodes[0] else {
        panic!("Core require expected");
    };
    *predicate = compare(CompareOp::Lt, tag.clone(), tag.clone());
    cases.push(("requirement in branch Lt".to_owned(), requirement));
    let mut iterable = baseline.clone();
    let (_, iter, _) = core_loop(&mut iterable, 3);
    *iter = CoreExpr::If {
        predicate: Box::new(compare(CompareOp::Lt, tag.clone(), tag.clone())),
        then_value: Box::new(iter.clone()),
        else_value: Box::new(iter.clone()),
    };
    cases.push(("conditional iterable Lt".to_owned(), iterable));
    for (case, core) in cases {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            Some("inadmissible_comparison_operator"),
            &case,
        );
    }
}

#[test]
fn v3_encoder_refuses_ordered_comparisons_of_non_integers() {
    let report = lower_to_target_ir(&comparisons_core(), &structured_facts());
    let artifact = report.artifact.expect("artifact");
    // Encoder: provably non-integer ordering in branch predicates.
    let bytes = || CoreExpr::Const(CoreValue::Bytes(vec![1]));
    let int = || {
        CoreExpr::Const(CoreValue::Int {
            width: "U64".to_owned(),
            value: "1".to_owned(),
        })
    };
    let with_predicate = |nested: bool, predicate: CorePredicate| {
        let mut encoded = artifact.clone();
        let order = order_mut(&mut encoded);
        let entry = if nested {
            &mut for_entry_mut(&mut order[2]).body[0]
        } else {
            &mut order[1]
        };
        branch_entry_mut(entry).predicate = predicate;
        encoded
    };
    for nested in [false, true] {
        assert_unsupported_value(
            &with_predicate(nested, compare(CompareOp::Lt, bytes(), bytes())),
            &format!("byte ordering, nested={nested}"),
        );
        encode_target_ir_artifact(&with_predicate(
            nested,
            compare(CompareOp::Lt, int(), int()),
        ))
        .unwrap_or_else(|error| panic!("integer ordering, nested={nested}: {error:?}"));
    }

    // v2 requirement encoding is unchanged by the v3 check.
    let report = lower_to_target_ir(
        &loop_core(TOP_LEVEL_REQUIREMENT),
        &facts_for(ECHO_ORDERED_SPAN_IR_DOMAIN),
    );
    let mut ordered_v2 = report.artifact.expect("v2 artifact");
    ordered_v2
        .intents
        .get_mut("t")
        .expect("intent")
        .requirements[0]
        .predicate = compare(CompareOp::Lt, bytes(), bytes());
    encode_target_ir_artifact(&ordered_v2).expect("v2 encoding is unchanged");
}

/// `ENSURE_EACH_LEAF` whose body is replaced by `nodes` built from its loop.
fn control_only_core(build: impl FnOnce(CoreNode) -> Vec<CoreNode>) -> CoreModule {
    let mut core = loop_core(ENSURE_EACH_LEAF);
    let nodes = &mut core.intents.get_mut("t").expect("intent").body.nodes;
    let CoreNode::For {
        binder,
        iter,
        bound,
        body,
    } = nodes.remove(0)
    else {
        panic!("Core for expected");
    };
    let empty_loop = CoreNode::For {
        body: CoreBlock {
            locals: vec![binder.clone()],
            nodes: Vec::new(),
            result: body.result,
        },
        binder,
        iter,
        bound,
    };
    *nodes = build(empty_loop);
    core
}

fn empty_branch() -> CoreNode {
    let empty = || CoreBlock {
        locals: Vec::new(),
        nodes: Vec::new(),
        result: CoreExpr::Const(CoreValue::Null),
    };
    CoreNode::Branch {
        binding: None,
        predicate: CorePredicate::True,
        then_block: empty(),
        else_block: empty(),
    }
}

#[test]
fn v3_intents_without_instructions_have_no_target_steps() {
    // Control: a loop with one real instruction lowers.
    let report = lower_to_target_ir(&loop_core(ENSURE_EACH_LEAF), &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let artifact = report.artifact.expect("artifact");

    let nested = control_only_core(|mut empty_loop| {
        let CoreNode::For { body, .. } = &mut empty_loop else {
            unreachable!()
        };
        body.nodes.push(empty_branch());
        vec![empty_loop]
    });
    for (case, core) in [
        (
            "empty branch only",
            control_only_core(|_| vec![empty_branch()]),
        ),
        (
            "empty loop only",
            control_only_core(|empty_loop| vec![empty_loop]),
        ),
        ("loop containing only an empty branch", nested),
    ] {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::NoTargetSteps,
            None,
            case,
        );
    }

    // Encoder: a v3 intent with empty tables and a control-only order.
    let mut empty_tree = artifact;
    let intent = empty_tree.intents.get_mut("t").expect("intent");
    intent.steps.clear();
    let order = intent.execution_order.as_mut().expect("order");
    for_entry_mut(&mut order[0]).body.clear();
    assert_unsupported_value(&empty_tree, "control-only v3 order");

    // v2 encoding of an intent with empty tables and order is unchanged.
    let report = lower_to_target_ir(
        &loop_core(TOP_LEVEL_REQUIREMENT),
        &facts_for(ECHO_ORDERED_SPAN_IR_DOMAIN),
    );
    let mut empty_v2 = report.artifact.expect("v2 artifact");
    let intent = empty_v2.intents.get_mut("t").expect("intent");
    intent.steps.clear();
    intent.requirements.clear();
    intent.execution_order = Some(Vec::new());
    intent.result = CoreExpr::Const(CoreValue::Null);
    encode_target_ir_artifact(&empty_v2).expect("v2 empty intent encodes as before");
}

const COMPARISON_HELPERS: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, tag: Bytes<max=8>, n: U64, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    fn same(a: Bytes<max=8>, b: Bytes<max=8>) -> Bool { return a == b; }\n\
    fn outer(a: Bytes<max=8>, b: Bytes<max=8>) -> Bool { return same(a, b); }\n\
    fn less(x: U64, y: U64) -> Bool { return x < y; }\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.big {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      if outer(input.tag, input.tag) {\n\
        require input.n == input.n else domain.WriteRejected;\n\
      }\n\
      if less(input.n, input.n) {\n\
        require input.n == input.n else domain.WriteRejected;\n\
      }\n\
      return { id: input.id };\n\
    }";

/// Replace a helper's result with `if a < b then true else false` over its
/// first two parameters.
fn order_helper_parameters(core: &mut CoreModule, helper: &str) {
    let function = core.functions.get_mut(helper).expect("helper");
    let parameter = |index: usize| CoreExpr::Local {
        reference: function.params[index].clone(),
    };
    let (left, right) = (parameter(0), parameter(1));
    function.body.result = CoreExpr::If {
        predicate: Box::new(CorePredicate::Compare {
            op: CompareOp::Lt,
            left,
            right,
        }),
        then_value: Box::new(CoreExpr::Const(CoreValue::Bool(true))),
        else_value: Box::new(CoreExpr::Const(CoreValue::Bool(false))),
    };
}

#[test]
fn v3_helper_bodies_require_admissible_comparison_operators() {
    let module = edict_syntax::parse_module(COMPARISON_HELPERS).expect("source parses");
    let context = loop_context().with_budget(
        "p.big",
        CoreBudget {
            max_steps: 4096,
            max_allocated_bytes: 65_536,
            max_output_bytes: 4096,
        },
    );
    let baseline = compile_to_core(&module, &context).expect("source compiles");

    // Controls: byte equality and integer ordering in helpers lower; ordering
    // the integer helper's own parameters is still admissible.
    let report = lower_to_target_ir(&baseline, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let mut integer = baseline.clone();
    order_helper_parameters(&mut integer, "less");
    let report = lower_to_target_ir(&integer, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");

    for (case, helper) in [
        ("helper called from a branch predicate", "outer"),
        ("helper called through another helper", "same"),
    ] {
        let mut core = baseline.clone();
        order_helper_parameters(&mut core, helper);
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            Some("inadmissible_comparison_operator"),
            case,
        );
    }
}

#[test]
fn requirement_inside_loop_stays_in_the_loop_body() {
    let artifact = structured_artifact(REQUIREMENT_IN_LOOP);
    assert_eq!(shape(order(&artifact)), "t.step.0,t.for.0[t.require.0]");
    assert!(matches!(
        artifact.intents["t"].requirements[0].on_failure,
        TargetIrRequireFailure::ContinueObstructed { .. }
    ));
    encode_target_ir_artifact(&artifact).expect("loop requirement encodes");
}

#[test]
fn encoder_refuses_loop_and_arm_locals_after_their_block() {
    let baseline = structured_artifact(ENSURE_EACH_LEAF);
    let receipt = baseline.intents["t"].steps[0].binding.clone();
    let mut after_loop = baseline.clone();
    after_loop.intents.get_mut("t").expect("intent").result =
        CoreExpr::Local { reference: receipt };
    assert_unsupported_value(&after_loop, "loop local after loop");

    let mut binder_after_loop = baseline.clone();
    let binder = for_entry(&order(&baseline)[0]).binder.clone();
    binder_after_loop
        .intents
        .get_mut("t")
        .expect("intent")
        .result = CoreExpr::Local { reference: binder };
    assert_unsupported_value(&binder_after_loop, "binder after loop");

    let nested = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    let then_receipt = nested.intents["t"].steps[0].binding.clone();
    let mut other_arm = nested.clone();
    other_arm.intents.get_mut("t").expect("intent").requirements[0].predicate =
        CorePredicate::Compare {
            op: CompareOp::Eq,
            left: CoreExpr::Local {
                reference: then_receipt.clone(),
            },
            right: CoreExpr::Local {
                reference: then_receipt,
            },
        };
    assert_unsupported_value(&other_arm, "then-arm local in else arm");

    let observed = nested.intents["t"].pure_bindings[0].binding.clone();
    let mut after_branch = nested;
    after_branch.intents.get_mut("t").expect("intent").result = CoreExpr::Local {
        reference: observed,
    };
    assert_unsupported_value(&after_branch, "arm loop local after branch");
}

#[test]
fn binder_shadowing_refuses_and_sibling_reuse_gets_fresh_identity() {
    let module = edict_syntax::parse_module(&source(SHADOWING_LOOP)).expect("shadowing parses");
    let errors = edict_syntax::validate_surface(&module).expect_err("shadowed binder");
    assert!(errors
        .iter()
        .any(|error| error.kind == edict_syntax::SemanticErrorKind::ShadowedName));

    let siblings = structured_artifact(SIBLING_LOOPS);
    assert_eq!(
        shape(order(&siblings)),
        "t.for.0[t.step.0],t.for.1[t.step.1]"
    );
    let first = &for_entry(&order(&siblings)[0]).binder;
    let second = &for_entry(&order(&siblings)[1]).binder;
    assert_ne!(first.id, second.id);

    // Core: an inner binder that reuses the visible outer binder identity.
    let mut core = loop_core(NESTED_LOOPS);
    let (outer, _, body) = core_loop(&mut core, 0);
    let outer = outer.clone();
    let CoreNode::For {
        binder,
        body: inner_body,
        ..
    } = &mut body.nodes[0]
    else {
        panic!("inner loop");
    };
    let inner_id = binder.id.clone();
    let reused = LocalRef {
        ty: binder.ty.clone(),
        ..outer
    };
    *binder = reused.clone();
    rename_block(inner_body, &inner_id, &reused);
    let report = lower_to_target_ir(&core, &structured_facts());
    assert_refused(
        &report,
        TargetLoweringFailureKind::InvalidCoreIdentity,
        None,
        "inner binder reuses outer binder",
    );

    // Encoder: a binder that reuses a visible producer identity.
    let mut artifact = structured_artifact(REQUIREMENT_IN_LOOP);
    let receipt = artifact.intents["t"].steps[0].binding.clone();
    let loop_entry = for_entry_mut(&mut order_mut(&mut artifact)[1]);
    let old = loop_entry.binder.id.clone();
    loop_entry.binder = receipt.clone();
    let requirement = &mut artifact.intents.get_mut("t").expect("intent").requirements[0];
    rename_predicate(&mut requirement.predicate, &old, &receipt);
    let (TargetIrRequireFailure::Terminal { reason }
    | TargetIrRequireFailure::ContinueObstructed { reason }) = &mut requirement.on_failure;
    reason
        .payload
        .values_mut()
        .for_each(|value| rename_expr(value, &old, &receipt));
    assert_unsupported_value(&artifact, "binder reuses visible producer");
}

#[test]
fn core_validation_covers_nested_blocks() {
    let mut cases = Vec::new();

    // A loop-body pure binding whose value does not fit its declared type.
    let mut wrong_type = loop_core(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    {
        let CoreNode::Branch { then_block, .. } =
            &mut wrong_type.intents.get_mut("t").expect("intent").body.nodes[1]
        else {
            panic!("branch");
        };
        let CoreNode::For { body, .. } = &mut then_block.nodes[0] else {
            panic!("loop in branch");
        };
        let CoreNode::Let { binding, .. } = &mut body.nodes[0] else {
            panic!("loop let");
        };
        "U64".clone_into(&mut binding.ty);
        let id = binding.id.clone();
        for local in &mut body.locals {
            if local.id == id {
                "U64".clone_into(&mut local.ty);
            }
        }
    }
    cases.push(("nested binding type", wrong_type));

    // A Core result that reads a loop-body local after the loop.
    let mut after_loop = loop_core(ENSURE_EACH_LEAF);
    let (_, _, body) = core_loop(&mut after_loop, 0);
    let CoreNode::Effect { binding, .. } = &body.nodes[0] else {
        panic!("loop effect");
    };
    let receipt = binding.clone();
    after_loop.intents.get_mut("t").expect("intent").body.result =
        CoreExpr::Local { reference: receipt };
    cases.push(("loop local after loop", after_loop));

    // A loop-body effect whose input reads an unavailable local.
    let mut foreign = loop_core(ENSURE_EACH_LEAF);
    let (_, _, body) = core_loop(&mut foreign, 0);
    let CoreNode::Effect { input, .. } = &mut body.nodes[0] else {
        panic!("loop effect");
    };
    *input = CoreExpr::Local {
        reference: LocalRef {
            id: "local.999".to_owned(),
            alpha_name: "$foreign".to_owned(),
            ty: "String<max=16,canonical=raw-utf8>".to_owned(),
        },
    };
    cases.push(("foreign local in loop body", foreign));

    // An iterable that is not a list.
    let mut not_a_list = loop_core(ENSURE_EACH_LEAF);
    let input_local = not_a_list.intents["t"]
        .body
        .locals
        .iter()
        .find(|local| local.id == "arg.0")
        .expect("application input local")
        .clone();
    let (_, iter, _) = core_loop(&mut not_a_list, 0);
    *iter = CoreExpr::Field {
        base: Box::new(CoreExpr::Local {
            reference: input_local,
        }),
        field: "id".to_owned(),
    };
    cases.push(("non-list iterable", not_a_list));

    // A branch predicate inside a loop that reads an unavailable local.
    let mut foreign_predicate = loop_core(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    let (_, _, body) = core_loop(&mut foreign_predicate, 0);
    let CoreNode::Branch { predicate, .. } = &mut body.nodes[0] else {
        panic!("branch in loop");
    };
    *predicate = CorePredicate::Compare {
        op: CompareOp::Eq,
        left: CoreExpr::Local {
            reference: LocalRef {
                id: "local.999".to_owned(),
                alpha_name: "$foreign".to_owned(),
                ty: "Bool".to_owned(),
            },
        },
        right: CoreExpr::Const(CoreValue::Bool(true)),
    };
    cases.push(("foreign branch predicate in loop", foreign_predicate));

    for (case, core) in cases {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            None,
            case,
        );
    }
}

#[test]
fn structured_encoder_refuses_missing_duplicate_foreign_and_cross_table_ids() {
    let baseline = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    let mut cases = Vec::new();

    let mut missing = baseline.clone();
    branch_entry_mut(&mut for_entry_mut(&mut order_mut(&mut missing)[0]).body[0])
        .then_entries
        .clear();
    cases.push(("missing instruction", missing));

    let mut duplicate = baseline.clone();
    branch_entry_mut(&mut for_entry_mut(&mut order_mut(&mut duplicate)[0]).body[0])
        .then_entries
        .push(instruction("t.step.0"));
    cases.push(("duplicate instruction", duplicate));

    let mut duplicate_across_scopes = baseline.clone();
    order_mut(&mut duplicate_across_scopes).push(instruction("t.require.0"));
    cases.push((
        "duplicate instruction across scopes",
        duplicate_across_scopes,
    ));

    let mut foreign = baseline.clone();
    order_mut(&mut foreign).push(instruction("other.step.9"));
    cases.push(("foreign instruction", foreign));

    let mut cross_table = baseline.clone();
    "t.step.0".clone_into(
        &mut cross_table
            .intents
            .get_mut("t")
            .expect("intent")
            .requirements[0]
            .id,
    );
    cases.push(("cross-table id", cross_table));

    let mut control_collides = baseline.clone();
    "t.step.0".clone_into(&mut for_entry_mut(&mut order_mut(&mut control_collides)[0]).id);
    cases.push(("control id collides with instruction", control_collides));

    let mut duplicate_control = baseline.clone();
    "t.for.0".clone_into(&mut branch_entry_mut(&mut order_mut(&mut duplicate_control)[1]).id);
    cases.push(("duplicate control id", duplicate_control));

    let mut empty_control = baseline.clone();
    for_entry_mut(&mut order_mut(&mut empty_control)[0])
        .id
        .clear();
    cases.push(("empty control id", empty_control));

    let mut absent = baseline;
    absent.intents.get_mut("t").expect("intent").execution_order = None;
    cases.push(("absent order", absent));

    for (case, artifact) in cases {
        assert_unsupported_value(&artifact, case);
    }
}

#[test]
fn ordered_encoder_refuses_control_entries() {
    let mut downgraded = structured_artifact(ENSURE_EACH_LEAF);
    ECHO_ORDERED_SPAN_IR_DOMAIN.clone_into(&mut downgraded.domain);
    assert_unsupported_value(&downgraded, "v2 with a for entry");

    let mut branch = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    ECHO_ORDERED_SPAN_IR_DOMAIN.clone_into(&mut branch.domain);
    assert_unsupported_value(&branch, "v2 with branch entries");

    let mut legacy = structured_artifact(ENSURE_EACH_LEAF);
    ECHO_SPAN_IR_DOMAIN.clone_into(&mut legacy.domain);
    assert_unsupported_value(&legacy, "v1 with an order");
}

#[test]
fn legacy_domains_still_refuse_structured_control() {
    let statement_branch = "  if input.id != \"\" {\n\
            let receipt: Receipt = target.replace(input.id)\n\
              else { rejected(reason) => domain.WriteRejected };\n\
          }\n\
          return { id: input.id };\n\
        }";
    for domain in [ECHO_SPAN_IR_DOMAIN, ECHO_ORDERED_SPAN_IR_DOMAIN] {
        let report = lower_to_target_ir(&loop_core(ENSURE_EACH_LEAF), &facts_for(domain));
        assert_refused(
            &report,
            TargetLoweringFailureKind::UnsupportedCoreNode,
            Some("for"),
            domain,
        );
        let report = lower_to_target_ir(&loop_core(statement_branch), &facts_for(domain));
        assert_refused(
            &report,
            TargetLoweringFailureKind::UnsupportedCoreNode,
            Some("branch"),
            domain,
        );
    }
}

#[test]
fn value_yielding_branch_refuses_under_v3() {
    let report = lower_to_target_ir(&loop_core(VALUE_BRANCH), &structured_facts());
    assert_refused(
        &report,
        TargetLoweringFailureKind::UnsupportedCoreNode,
        Some("branch_binding"),
        "value-yielding branch",
    );
}

#[test]
fn structured_contract_refuses_block_results_and_external_requests() {
    let mut loop_result = loop_core(ENSURE_EACH_LEAF);
    let (_, _, body) = core_loop(&mut loop_result, 0);
    body.result = CoreExpr::Const(CoreValue::Bool(true));
    let report = lower_to_target_ir(&loop_result, &structured_facts());
    assert_refused(
        &report,
        TargetLoweringFailureKind::UnsupportedCoreNode,
        Some("control_block_result"),
        "loop body result",
    );

    let core = workspace_request_core();
    let mut facts = structured_facts();
    facts
        .operation_profiles
        .push("continuum.profile.read-only/v1".to_owned());
    let report = lower_to_target_ir(&core, &facts);
    assert_refused(
        &report,
        TargetLoweringFailureKind::UnsupportedCoreNode,
        Some("ordered_external_action_request"),
        "external request",
    );
}

#[test]
fn structure_mutations_move_the_digest() {
    let baseline = structured_artifact(INDEPENDENT_LOOP_EFFECT);
    let original = digest_target_ir_artifact(&baseline).expect("baseline digest");

    let mut hoisted = baseline.clone();
    for_entry_mut(&mut order_mut(&mut hoisted)[0]).body.clear();
    order_mut(&mut hoisted).push(instruction("t.step.0"));
    let mut rebounded = baseline.clone();
    for_entry_mut(&mut order_mut(&mut rebounded)[0]).bound = CoreBound::Literal(5);

    let branched = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    let branched_digest = digest_target_ir_artifact(&branched).expect("branch digest");
    let mut swapped = branched;
    let arm = branch_entry_mut(&mut for_entry_mut(&mut order_mut(&mut swapped)[0]).body[0]);
    std::mem::swap(&mut arm.then_entries, &mut arm.else_entries);

    for (case, artifact, before) in [
        ("instruction hoisted out of the loop", hoisted, &original),
        ("changed loop bound", rebounded, &original),
        ("swapped branch arms", swapped, &branched_digest),
    ] {
        assert_ne!(
            &digest_target_ir_artifact(&artifact).expect(case),
            before,
            "{case}"
        );
    }
}

fn leaf_record(id: &str) -> CoreExpr {
    CoreExpr::Record {
        fields: BTreeMap::from([(
            "id".to_owned(),
            CoreExpr::Const(CoreValue::String(id.to_owned())),
        )]),
    }
}

fn with_loop(core: &mut CoreModule, iterable: Option<CoreExpr>, bound: CoreBound) {
    let CoreNode::For {
        iter, bound: old, ..
    } = &mut core.intents.get_mut("t").expect("intent").body.nodes[0]
    else {
        panic!("Core for expected");
    };
    if let Some(iterable) = iterable {
        *iter = iterable;
    }
    *old = bound;
}

#[test]
fn target_refuses_literal_bounds_below_the_iterable_maximum() {
    // `input.leaves` is `List<Leaf, max=4>`; a forged bound of 2 is unsound.
    let mut below_list_max = loop_core(ENSURE_EACH_LEAF);
    with_loop(&mut below_list_max, None, CoreBound::Literal(2));

    // A literal list of three elements under a literal bound of two.
    let three = CoreExpr::List {
        values: vec![leaf_record("a"), leaf_record("b"), leaf_record("c")],
    };
    let mut literal_over_bound = loop_core(ENSURE_EACH_LEAF);
    with_loop(&mut literal_over_bound, Some(three), CoreBound::Literal(2));

    for (case, core) in [
        ("bound below list maximum", below_list_max),
        ("literal list above bound", literal_over_bound),
    ] {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::InvalidCoreIdentity,
            None,
            case,
        );
    }

    // Controls: the same shapes at the bound lower.
    let mut at_list_max = loop_core(ENSURE_EACH_LEAF);
    with_loop(&mut at_list_max, None, CoreBound::Literal(4));
    let two = CoreExpr::List {
        values: vec![leaf_record("a"), leaf_record("b")],
    };
    let mut literal_at_bound = loop_core(ENSURE_EACH_LEAF);
    with_loop(&mut literal_at_bound, Some(two), CoreBound::Literal(2));
    for (case, core) in [
        ("bound equals list maximum", at_list_max),
        ("literal list at bound", literal_at_bound),
    ] {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_eq!(
            report.status,
            TargetLoweringStatus::Lowered,
            "{case}: {report:?}"
        );
    }
}

const BOUNDS_IMPORT: &str = "use lawpack example.bounds@1 digest \"sha256:1111111111111111111111111111111111111111111111111111111111111111\" as bounds;\n";

fn coordinate_bound_core() -> CoreModule {
    let source = source(ENSURE_EACH_LEAF)
        .replace("type Leaf", &format!("{BOUNDS_IMPORT}    type Leaf"))
        .replace("bounded 4", "bounded bounds.maxItems");
    let module = edict_syntax::parse_module(&source).expect("coordinate loop parses");
    let context = loop_context().with_bound(
        "bounds.maxItems",
        edict_syntax::BoundFact {
            lawpack: ResourceRef {
                coordinate: "example.bounds@1".to_owned(),
                digest: Some(
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
                ),
            },
            coordinate: "example.bounds@1.maxItems".to_owned(),
            value: 4,
        },
    );
    compile_to_core(&module, &context).expect("coordinate loop compiles")
}

#[test]
fn coordinate_loop_bounds_refuse_without_authenticated_constant_values() {
    // Target has no authenticated constant values, so it cannot prove that a
    // coordinate bound covers the iterable maximum and must fail closed.
    let compiled = coordinate_bound_core();
    let forged = |coordinate: &str| {
        let mut core = coordinate_bound_core();
        with_loop(
            &mut core,
            None,
            CoreBound::Coordinate(coordinate.to_owned()),
        );
        core
    };
    // A loop nested inside a branch is checked as well.
    let mut nested = loop_core(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    let CoreNode::Branch { then_block, .. } =
        &mut nested.intents.get_mut("t").expect("intent").body.nodes[1]
    else {
        panic!("top-level branch expected");
    };
    let CoreNode::For { bound, .. } = &mut then_block.nodes[0] else {
        panic!("loop inside branch expected");
    };
    *bound = CoreBound::Coordinate("example.bounds@1.maxItems".to_owned());

    for (case, core) in [
        (
            "compiled in-namespace constant covering the maximum",
            compiled,
        ),
        (
            "in-namespace constant the compiler never checked",
            forged("example.bounds@1.fewItems"),
        ),
        (
            "nonexistent constant in the namespace",
            forged("example.bounds@1.absent"),
        ),
        (
            "coordinate outside every imported lawpack",
            forged("other.package@1.maxItems"),
        ),
        ("coordinate loop inside a branch", nested),
    ] {
        let report = lower_to_target_ir(&core, &structured_facts());
        assert_refused(
            &report,
            TargetLoweringFailureKind::UnsupportedCoreNode,
            Some("coordinate_loop_bound"),
            case,
        );
    }

    // Control: the same compiled loop with a literal bound lowers.
    let mut literal = coordinate_bound_core();
    with_loop(&mut literal, None, CoreBound::Literal(4));
    let report = lower_to_target_ir(&literal, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
}

#[test]
fn encoder_refuses_coordinate_loop_bounds() {
    let coordinate = || CoreBound::Coordinate("example.bounds@1.maxItems".to_owned());
    let mut top = structured_artifact(ENSURE_EACH_LEAF);
    for_entry_mut(&mut order_mut(&mut top)[0]).bound = coordinate();
    assert_unsupported_value(&top, "coordinate bound on a top-level loop");

    let nested = structured_artifact(NESTED_LOOPS);
    let mut inner = nested.clone();
    let outer = for_entry_mut(&mut order_mut(&mut inner)[0]);
    for_entry_mut(&mut outer.body[0]).bound = coordinate();
    assert_unsupported_value(&inner, "coordinate bound on a loop inside a loop");

    let branched = structured_artifact(BRANCH_IN_LOOP_AND_LOOP_IN_BRANCH);
    let mut in_branch = branched.clone();
    let branch = branch_entry_mut(&mut order_mut(&mut in_branch)[1]);
    for_entry_mut(&mut branch.then_entries[0]).bound = coordinate();
    assert_unsupported_value(&in_branch, "coordinate bound on a loop inside a branch");

    // Control: the unmodified artifacts with literal bounds encode.
    for (case, artifact) in [
        ("top", structured_artifact(ENSURE_EACH_LEAF)),
        ("nested", nested),
        ("branched", branched),
    ] {
        encode_target_ir_artifact(&artifact).unwrap_or_else(|error| panic!("{case}: {error:?}"));
    }
}

#[test]
fn empty_literal_iterable_lowers_with_its_body_intact() {
    let mut core = loop_core(ENSURE_EACH_LEAF);
    with_loop(
        &mut core,
        Some(CoreExpr::List { values: Vec::new() }),
        CoreBound::Literal(4),
    );
    let report = lower_to_target_ir(&core, &structured_facts());
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let artifact = report.artifact.expect("empty-iterable artifact");
    assert_eq!(shape(order(&artifact)), "t.for.0[t.step.0]");
    assert_eq!(
        for_entry(&order(&artifact)[0]).iterable,
        CoreExpr::List { values: Vec::new() }
    );
    assert_eq!(artifact.intents["t"].steps.len(), 1);
    let encoded = encoded_intent(&artifact);
    let CanonicalValue::Array(entries) = field(&encoded, "executionOrder") else {
        panic!("order array");
    };
    assert_eq!(
        field(&entries[0], "body"),
        &CanonicalValue::Array(vec![CanonicalValue::Text("t.step.0".to_owned())])
    );
}

#[test]
fn empty_list_bound_in_source_lowers_with_its_body_intact() {
    // A bare `[]` needs an expected type, so source binds it first.
    let body = "  let empty: List<Leaf, max=4> = [];\n\
          for leaf in empty bounded 4 {\n\
            let receipt: Receipt = target.replace(leaf.id)\n\
              else { rejected(reason) => domain.WriteRejected };\n\
          }\n\
          return { id: input.id };\n\
        }";
    let artifact = structured_artifact(body);
    assert_eq!(shape(order(&artifact)), "t.binding.0,t.for.0[t.step.0]");
    assert_eq!(
        artifact.intents["t"].pure_bindings[0].value,
        CoreExpr::List { values: Vec::new() }
    );
    encode_target_ir_artifact(&artifact).expect("empty source list encodes");
}
