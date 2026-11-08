//! Load-bearing witnesses for each structured correspondence guard.
//!
//! Every case changes exactly one property that the structured matcher or its
//! header check compares, and must be refused by emission and verification.

use super::structured::{lower_structured, order_mut, pair, refuses, INTENT, STRUCTURED_SOURCE};
use super::*;
use amblicit_syntax::{
    TargetIrBranchEntry, TargetIrIntent, TargetIrOrderEntry, TargetIrRequireFailure,
    ECHO_STRUCTURED_SPAN_IR_DOMAIN,
};

type Mutation = fn(&CoreModule, &mut TargetIrArtifact);

const NESTED_LOOP: usize = 1;
const NESTED_BRANCH: usize = 1;
const NESTED_STEP: usize = 0;
const NESTED_REQUIRE: usize = 0;

fn intent_mut(target: &mut TargetIrArtifact) -> &mut TargetIrIntent {
    target.intents.get_mut(INTENT).expect("intent")
}

fn loop_mut(target: &mut TargetIrArtifact) -> &mut amblicit_syntax::TargetIrForEntry {
    let TargetIrOrderEntry::For(entry) = &mut order_mut(target)[NESTED_LOOP] else {
        panic!("loop")
    };
    entry
}

fn branch_mut(target: &mut TargetIrArtifact) -> &mut TargetIrBranchEntry {
    let TargetIrOrderEntry::Branch(branch) = &mut loop_mut(target).body[NESTED_BRANCH] else {
        panic!("branch")
    };
    branch
}

fn application_input(core: &CoreModule) -> LocalRef {
    let intent = &core.intents[INTENT];
    intent
        .body
        .locals
        .iter()
        .find(|local| local.ty == intent.input)
        .cloned()
        .expect("application input local")
}

fn rename_instruction(entries: &mut [TargetIrOrderEntry], from: &str, to: &str) {
    let renamed = entries
        .iter_mut()
        .filter_map(|entry| match entry {
            TargetIrOrderEntry::Instruction(id) if id == from => Some(id),
            _ => None,
        })
        .map(|id| to.clone_into(id))
        .count();
    assert_eq!(renamed, 1, "exactly one {from} entry");
}

fn require_reason_mut(
    target: &mut TargetIrArtifact,
) -> &mut amblicit_syntax::CoreObstructionReason {
    match &mut intent_mut(target).requirements[NESTED_REQUIRE].on_failure {
        TargetIrRequireFailure::Terminal { reason }
        | TargetIrRequireFailure::ContinueObstructed { reason } => reason,
    }
}

const MUTATIONS: [(&str, Mutation); 12] = [
    ("M05 loop iterable emptied", |_, target| {
        loop_mut(target).iterable = CoreExpr::List { values: vec![] };
    }),
    ("M15 loop id out of sequence", |_, target| {
        "createGreeting.for.9".clone_into(&mut loop_mut(target).id);
    }),
    ("M15 loop id unique but wrong family", |_, target| {
        "createGreeting.loop.0".clone_into(&mut loop_mut(target).id);
    }),
    ("M16 branch id renamed", |_, target| {
        "createGreeting.for.0x".clone_into(&mut branch_mut(target).id);
    }),
    (
        "M18 nested step id renamed in table and order",
        |_, target| {
            let forged = "createGreeting.forgedStep";
            let step = &mut intent_mut(target).steps[NESTED_STEP];
            let original = std::mem::replace(&mut step.id, forged.to_owned());
            rename_instruction(&mut branch_mut(target).then_entries, &original, forged);
        },
    ),
    ("M22 require id renamed in table and order", |_, target| {
        let forged = "createGreeting.forgedReq";
        let requirement = &mut intent_mut(target).requirements[NESTED_REQUIRE];
        let original = std::mem::replace(&mut requirement.id, forged.to_owned());
        rename_instruction(&mut branch_mut(target).else_entries, &original, forged);
    }),
    (
        "M17 require Terminal flipped to ContinueObstructed",
        |_, target| {
            let requirement = &mut intent_mut(target).requirements[NESTED_REQUIRE];
            let TargetIrRequireFailure::Terminal { reason } = requirement.on_failure.clone() else {
                panic!("fixture requirement is terminal")
            };
            requirement.on_failure = TargetIrRequireFailure::ContinueObstructed { reason };
        },
    ),
    ("M28 require reason kind changed", |_, target| {
        "forged.Reason".clone_into(&mut require_reason_mut(target).kind);
    }),
    ("M19 extra empty root loop", |_, target| {
        let mut extra = loop_mut(target).clone();
        "createGreeting.for.1".clone_into(&mut extra.id);
        extra.binder = LocalRef {
            id: "local.99".to_owned(),
            alpha_name: "$local99".to_owned(),
            ty: extra.binder.ty.clone(),
        };
        extra.body.clear();
        order_mut(target).push(TargetIrOrderEntry::For(extra));
    }),
    ("M20 nested step input changed", |core, target| {
        intent_mut(target).steps[NESTED_STEP].input = CoreExpr::Field {
            base: Box::new(CoreExpr::Local {
                reference: application_input(core),
            }),
            field: "item".to_owned(),
        };
    }),
    ("M27 nested step binding forged", |_, target| {
        "$forged".clone_into(&mut intent_mut(target).steps[NESTED_STEP].binding.alpha_name);
    }),
    ("M29 nested step effect changed", |_, target| {
        "hello.other".clone_into(&mut intent_mut(target).steps[NESTED_STEP].effect);
    }),
];

#[test]
fn structured_projection_refuses_changed_nested_payloads_and_identities() {
    let (core, target) = pair();
    let projection = emit_result_projection(&core, &target, INTENT).expect("projection");
    for (label, mutate) in MUTATIONS {
        let mut changed = target.clone();
        mutate(&core, &mut changed);
        assert_ne!(changed, target, "{label} must change the Target");
        println!("mutation: {label}");
        refuses(&core, &changed, &projection);
    }
}

#[test]
fn structured_header_refusal_names_its_own_subject() {
    let (core, target) = pair();
    let projection = emit_result_projection(&core, &target, INTENT).expect("projection");
    let mut changed = target.clone();
    "not.echo".clone_into(&mut changed.target_profile.coordinate);
    let subject = format!("{INTENT}.structuredHeader");
    let emitted = emit_result_projection(&core, &changed, INTENT).expect_err("emit must refuse");
    assert_eq!(
        (emitted.kind(), emitted.subject()),
        (
            ResultProjectionFailureKind::CoreTargetMismatch,
            subject.as_str()
        )
    );
    let verified = verify_result_projection(
        &core,
        &changed,
        INTENT,
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect_err("verify must refuse");
    assert_eq!(
        (verified.kind(), verified.subject()),
        (
            ResultProjectionFailureKind::CoreTargetMismatch,
            subject.as_str()
        )
    );
}

#[test]
fn flat_source_under_v3_emits_and_verifies_through_the_structured_path() {
    let (core, report) = lower_structured(SOURCE);
    let target = report.artifact.as_ref().expect("Target");
    assert_eq!(target.domain, ECHO_STRUCTURED_SPAN_IR_DOMAIN);
    assert!(target.intents[INTENT].execution_order.is_some());
    assert!(report.result_projection_failures.is_empty(), "{report:?}");
    let projection = report.result_projections.get(INTENT).expect("projection");
    assert_eq!(projection.projection().expression, expected_expression());
    let verified = verify_result_projection(
        &core,
        target,
        INTENT,
        projection.canonical_bytes(),
        projection.digest(),
    )
    .expect("verify flat v3 projection");
    assert_eq!(verified.projection(), projection.projection());
}

const LOOP_BODY: &str = "for item in input.items bounded 2 {\n    let observed = item;\n    if observed.message != \"\" {\n      let receipt: hello.GreetingReceipt = hello.createGreeting(observed)\n        else { alreadyExists(existing) => hello.AlreadyExists };\n    } else {\n      require observed.message == \"\" else hello.AlreadyExists;\n    }\n  }";

#[test]
fn empty_body_loop_refuses_a_binder_only_change() {
    assert_eq!(STRUCTURED_SOURCE.matches(LOOP_BODY).count(), 1);
    let source = STRUCTURED_SOURCE.replace(LOOP_BODY, "for item in input.items bounded 2 {\n  }");
    let (core, report) = lower_structured(&source);
    let target = report.artifact.expect("Target");
    let projection = report
        .result_projections
        .get(INTENT)
        .expect("empty-loop projection");
    let binder_changes: [(&str, Mutation); 2] = [
        (
            "M04 loop binder retyped to the application input",
            |core, target| {
                core.intents[INTENT]
                    .input
                    .clone_into(&mut loop_mut(target).binder.ty);
            },
        ),
        ("M04 loop binder alpha name changed", |_, target| {
            "$forgedBinder".clone_into(&mut loop_mut(target).binder.alpha_name);
        }),
    ];
    for (label, mutate) in binder_changes {
        let mut changed = target.clone();
        mutate(&core, &mut changed);
        assert_eq!(loop_mut(&mut changed).body, [] as [TargetIrOrderEntry; 0]);
        println!("mutation: {label}");
        refuses(&core, &changed, projection);
    }
}
