//! Target profile `echo.dpo@2` selects the structured `echo.span-ir/v3`
//! contract when the adapter names it, and keeps its own intrinsic namespace
//! inside loops and branches (issue #319).

use super::*;
use amblicit_syntax::{TargetIrOrderEntry, ECHO_STRUCTURED_SPAN_IR_DOMAIN};

const ECHO_DPO_2: &str = "echo.dpo@2";
const ORDERED: &str = "echo.span-ir/v2";

/// A bounded `for` and statement branches, each calling an effect intrinsic.
/// The same source is the `echo-dpo-2-structured` Target IR golden.
const LOOP_AND_BRANCHES: &str = "package a.b@1;\n\
    type Leaf = { id: String<max=16>, };\n\
    type Input = { id: String<max=16>, leaves: List<Leaf, max=4>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.loop {\n\
      for leaf in input.leaves bounded 4 {\n\
        if leaf.id != \"\" {\n\
          let written: Receipt = target.replace(leaf.id)\n\
            else { rejected(reason) => domain.WriteRejected };\n\
        }\n\
      }\n\
      if input.id != \"\" {\n\
        let stamped: Receipt = target.replace(input.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      }\n\
      return { id: input.id };\n\
    }";

/// The fixture's root statement branch without the loop.
const ROOT_BRANCH_ONLY: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.loop {\n\
      if input.id != \"\" {\n\
        let stamped: Receipt = target.replace(input.id)\n\
          else { rejected(reason) => domain.WriteRejected };\n\
      }\n\
      return { id: input.id };\n\
    }";

fn core_for(source: &str) -> CoreModule {
    let context = effectful_context().with_budget(
        "p.loop",
        CoreBudget {
            max_steps: 64,
            max_allocated_bytes: 4096,
            max_output_bytes: 256,
        },
    );
    let module = amblicit_syntax::parse_module(source).expect("source parses");
    compile_to_core(&module, &context).expect("source compiles")
}

fn loop_and_branches_core() -> CoreModule {
    core_for(LOOP_AND_BRANCHES)
}

fn facts(profile: &str, domain: &str, target_intrinsic: &str) -> TargetIrLoweringFacts {
    let mut facts = echo_facts();
    profile.clone_into(&mut facts.target_profile.coordinate);
    domain.clone_into(&mut facts.target_ir_domain);
    target_intrinsic.clone_into(&mut facts.effect_lowerings[0].target_intrinsic);
    facts
}

fn structured_echo_dpo_2_artifact() -> TargetIrArtifact {
    let report = lower_to_target_ir(
        &loop_and_branches_core(),
        &facts(
            ECHO_DPO_2,
            ECHO_STRUCTURED_SPAN_IR_DOMAIN,
            "echo.dpo@2.replace",
        ),
    );
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    assert!(report.failures.is_empty(), "{report:?}");
    report.artifact.expect("structured echo.dpo@2 artifact")
}

/// `id[body|else]` for control entries, the bare id for instructions.
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

fn map_entry<'a>(value: &'a CanonicalValue, key: &str) -> &'a CanonicalValue {
    let CanonicalValue::Map(entries) = value else {
        panic!("map expected")
    };
    entries
        .iter()
        .find_map(|(name, value)| (name == &CanonicalValue::Text(key.to_owned())).then_some(value))
        .unwrap_or_else(|| panic!("map key `{key}` exists"))
}

fn text(value: &str) -> CanonicalValue {
    CanonicalValue::Text(value.to_owned())
}

/// Refused with no artifact and exactly these `(kind, node index, detail)`
/// failures, in report order.
fn assert_refused(
    report: &amblicit_syntax::TargetLoweringReport,
    expected: &[(TargetLoweringFailureKind, Option<usize>, &str)],
) {
    assert_eq!(
        report.status,
        TargetLoweringStatus::Unsupported,
        "{report:?}"
    );
    assert!(report.artifact.is_none(), "{report:?}");
    let actual = report
        .failures
        .iter()
        .map(|failure| (failure.kind, failure.node_index, failure.detail.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

const NO_STEPS: (TargetLoweringFailureKind, Option<usize>, &str) = (
    TargetLoweringFailureKind::NoTargetSteps,
    None,
    "intent has no target-owned steps",
);

#[test]
fn echo_dpo_2_lowers_loops_and_branches_to_structured_target_ir() {
    let artifact = structured_echo_dpo_2_artifact();

    assert_eq!(artifact.domain, ECHO_STRUCTURED_SPAN_IR_DOMAIN);
    assert_eq!(artifact.target_profile.coordinate, ECHO_DPO_2);
    let intent = &artifact.intents["t"];
    assert_eq!(
        shape(intent.execution_order.as_deref().expect("structured order")),
        "t.for.0[t.branch.0[t.step.0|]],t.branch.1[t.step.1|]"
    );
    let intrinsics = intent
        .steps
        .iter()
        .map(|step| step.target_intrinsic.as_str())
        .collect::<Vec<_>>();
    assert_eq!(intrinsics, ["echo.dpo@2.replace", "echo.dpo@2.replace"]);

    let bytes = encode_target_ir_artifact(&artifact).expect("structured echo.dpo@2 encoding");
    let value = decode_canonical_cbor(&bytes).expect("canonical decoding");
    assert_eq!(
        map_entry(&value, "kind"),
        &text("structuredTargetIrArtifact")
    );
    assert_eq!(
        map_entry(&value, "domain"),
        &text(ECHO_STRUCTURED_SPAN_IR_DOMAIN)
    );
    assert_eq!(
        map_entry(map_entry(&value, "targetProfile"), "id"),
        &text(ECHO_DPO_2)
    );
}

#[test]
fn echo_dpo_2_with_the_ordered_adapter_still_refuses_loops_and_branches() {
    let branch_core = core_for(ROOT_BRANCH_ONLY);
    let ordered = facts(ECHO_DPO_2, ORDERED, "echo.dpo@2.replace");

    let report = lower_to_target_ir(&loop_and_branches_core(), &ordered);
    assert_refused(
        &report,
        &[
            (
                TargetLoweringFailureKind::UnsupportedCoreNode,
                Some(0),
                "for",
            ),
            (
                TargetLoweringFailureKind::UnsupportedCoreNode,
                Some(1),
                "branch",
            ),
        ],
    );
    let report = lower_to_target_ir(&branch_core, &ordered);
    assert_refused(
        &report,
        &[(
            TargetLoweringFailureKind::UnsupportedCoreNode,
            Some(0),
            "branch",
        )],
    );
}

#[test]
fn structured_echo_profiles_refuse_each_others_intrinsics() {
    for (profile, foreign) in [
        (ECHO_DPO_2, "echo.dpo@1.replace"),
        (ECHO_DPO_2, "echo.dpo@20.replace"),
        (ECHO_DPO_TARGET_PROFILE, "echo.dpo@2.replace"),
    ] {
        let report = lower_to_target_ir(
            &loop_and_branches_core(),
            &facts(profile, ECHO_STRUCTURED_SPAN_IR_DOMAIN, foreign),
        );

        // Both effects, the one nested in the loop's branch and the root one,
        // refuse; no step remains.
        let refused = TargetLoweringFailureKind::UnsupportedTargetIntrinsic;
        assert_refused(
            &report,
            &[
                (refused, Some(0), foreign),
                (refused, Some(1), foreign),
                NO_STEPS,
            ],
        );
    }
}

#[test]
fn structured_encoder_refuses_a_step_intrinsic_outside_the_selected_profile() {
    let echo_dpo_2 = structured_echo_dpo_2_artifact();
    let mut echo_dpo_1 = echo_dpo_2.clone();
    ECHO_DPO_TARGET_PROFILE.clone_into(&mut echo_dpo_1.target_profile.coordinate);
    for step in &mut echo_dpo_1.intents.get_mut("t").expect("intent").steps {
        "echo.dpo@1.replace".clone_into(&mut step.target_intrinsic);
    }
    // Controls: each profile encodes its own namespace.
    encode_target_ir_artifact(&echo_dpo_2).expect("echo.dpo@2 steps encode");
    encode_target_ir_artifact(&echo_dpo_1).expect("echo.dpo@1 steps encode");

    // One foreign step, nested in the loop's branch, is enough to refuse.
    for (artifact, foreign) in [
        (&echo_dpo_2, "echo.dpo@1.replace"),
        (&echo_dpo_1, "echo.dpo@2.replace"),
    ] {
        let mut forged = artifact.clone();
        foreign.clone_into(
            &mut forged.intents.get_mut("t").expect("intent").steps[0].target_intrinsic,
        );

        let error = encode_target_ir_artifact(&forged).expect_err(foreign);
        assert_eq!(
            error.kind(),
            CanonicalErrorKind::UnsupportedValue,
            "{foreign}"
        );
        let error = digest_target_ir_artifact(&forged).expect_err(foreign);
        assert_eq!(
            error.kind(),
            CanonicalErrorKind::UnsupportedValue,
            "{foreign}"
        );
    }
}

#[test]
fn structured_echo_dpo_2_maps_nested_obstruction_keys_through_the_adapter() {
    let mut mapped = facts(
        ECHO_DPO_2,
        ECHO_STRUCTURED_SPAN_IR_DOMAIN,
        "echo.dpo@2.replace",
    );
    mapped.effect_lowerings[0]
        .failure_mappings
        .insert("rejected".to_owned(), "echo.rejected".to_owned());
    let mut undeclared = mapped.clone();
    vec!["echo.rejected".to_owned()].clone_into(&mut mapped.obstruction_coordinates);

    let report = lower_to_target_ir(&loop_and_branches_core(), &mapped);
    assert_eq!(report.status, TargetLoweringStatus::Lowered, "{report:?}");
    let artifact = report.artifact.expect("mapped artifact");
    for step in &artifact.intents["t"].steps {
        assert_eq!(step.obstruction_failures, ["echo.rejected"]);
        assert_eq!(
            step.obstruction_arms.keys().collect::<Vec<_>>(),
            ["echo.rejected"]
        );
    }

    // The adapter declares only the unmapped key, so the mapped one refuses.
    vec!["rejected".to_owned()].clone_into(&mut undeclared.obstruction_coordinates);
    let report = lower_to_target_ir(&loop_and_branches_core(), &undeclared);
    let missing = TargetLoweringFailureKind::MissingObstruction;
    assert_refused(
        &report,
        &[
            (missing, Some(0), "echo.rejected"),
            (missing, Some(1), "echo.rejected"),
            NO_STEPS,
        ],
    );
}
