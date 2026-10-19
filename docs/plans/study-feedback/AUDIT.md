# Author Study Claim Audit

This page records the source audit for the 2026-10-06 feedback.
The audit uses Edict `01161c1745baad0d713234ba1a671b9a26923aa4`.
It uses Echo `a93e9d82e89455ed1fa0b63447c88de544b9da26` and Hello Echo `ee45716e8839efa14798b3e9e35c87538f456b25` for cross-repository facts.
GitHub main-commit queries confirmed these two external revisions on 2026-10-07.
The original feedback is preserved without changes in [the study source](../../studies/2026-10-06-edict-feedback.md).

The study's raw results are unavailable on this workstation.
No row below claims to reproduce its Echo execution or timing measurements.
Source inspection can confirm a missing check without proving the reported runtime output.
Each task names immutable source evidence.

| Feedback | Audit result | Evidence and boundary | Task |
| --- | --- | --- | --- |
| 1 | Confirmed compiler flaw | `check_intent` checks helper costs before the body. The declared return frame enters those costs only when source functions exist. An intent without source functions has no return-size comparison. The suggested 4,140-byte number is not a verified bound. `String<max=N>` limits Unicode scalars, so UTF-8 can require four bytes per scalar. | [F01](F01.md), #231 |
| 2 | Confirmed diagnostic flaws | Pure lookup does not recognize a bare imported effect. The signature error gives no field detail. A failed binding remains absent from the environment, and body validation adds a missing-return error. The source confirms these paths; the exact three study records remain source-reported. | [F02 container](F02.md); [F02A](F02A.md), [F02B](F02B.md), [F02C](F02C.md) |
| 3 | Confirmed tooling flaw | `application_build.rs` converts the compiler error vector to a Debug string in one `ApplicationCompilationFailed` failure. Existing check/projection record paths do not repair the application-build path. | [F03](F03.md), #233 |
| 4 | Confirmed documentation flaw | README shipping status excludes CLI paths implemented by the CLI topic. Existing issue #229 owns this outcome. The task also distinguishes Edict compilation from Echo execution. | [F04](F04.md), #229 |
| 5 | Confirmed missing causal-cell capability; general claim too broad | The causal-cell export has one create effect. Echo's Edict provider checks a create-if-absent intrinsic and program kind. Echo's generic runtime separately supports CAS. Edict also has pure and external-request paths, so create-if-absent is not the only executable Edict capability. | [F07](F07.md), #236 |
| 6 | Confirmed guide gap; generator-fork requirement rejected | The stock fixture has fixed constants. JSON lawpack authoring supports bounded types, local resources, and target adapters. Its independent application test already authors and builds a workspace-snapshot closure. Authors do not need a generator fork; they need an executable causal-cell example. | [F06](F06.md), #235 |
| 7 | Confirmed entry-point guidance gap | Imports match the validated manifest domain digest. The fixture identifies the domain, and the formal spec explains domain separation. Entry import examples do not clearly distinguish the sidecar digest from file SHA-256. A mismatch exposes the expected digest without domain guidance. | [F05](F05.md), #234 |
| 8 | Pins confirmed; defect not established | Hello Echo deliberately rejects producer drift and documents the process for advancing pins. Older pins are a compatibility contract, not proof of a broken template. No pin-update issue is justified by this study alone. F04 must link the lock and identify the version boundary. | [F04](F04.md), #229 |
| 9 | Confirmed missing key-bound contract; runtime result not reproduced | The Edict target configuration has `maxReplacementBytes` and no key bound. Echo's create lowerer has no key-bound configuration field. General ingress schema validation is an explicit Echo exclusion. The create lowerer and verifier read known fields without universal unknown-field rejection. An added key-limit field can be ignored. Metadata alone establishes no enforcement. Compatible Echo enforcement must precede the Edict artifact change. | [F08](F08.md), #237 |
| 10 | Praise retained as source-reported evidence | The report names successful execution, identical bytes, refusals, and a build duration. This audit does not verify those raw outputs or infer current compatibility from them. | No defect issue |

## Execution Rules

Use one executable issue for one independently verifiable outcome.
Use a regular merge or squash integration that retains the issue and PR link.
Do not infer a prerequisite from shared files.
F06 depends on F01 because its larger-value application must use the corrected output-budget contract.
F07 depends on F01 because the new CAS output must satisfy that contract.
F07 and F08 also have external Echo prerequisites. Those are unresolved, not recorded merged edges.

Use `cargo xtask verify` before branch readiness.
Run behavior tests in Docker. Record RED before implementation.
For prose changes, use concrete before/after source review and existing documentation-tool checks.
Do not add a test that passes only because prose contains a phrase.
Do not close a runtime task with compiler-only evidence.

## Resource Contract

Use the reusable `edict-validation` worker and `rust:1.96.0` image.
Use the workstation git-locks store and both `host/heavy-work` and `host/docker/edict-validation/` for validation.
The owned `edict-build-cache` volume holds Cargo dependencies, the owned Rust toolchain, and compiler output within one aggregate 20 GiB budget. Temporary compiler outputs also enter that accounting.
The worker root is read-only. Temporary source, test, and scratch paths use bounded tmpfs mounts.
The task monitor checks host and VM free space, volume use, temporary use, and host logs every two seconds.
A failed measurement, timeout, or budget breach stops the worker and its child workloads.
The workload timeout is 2,400 seconds. The worker has four CPUs, 12 GiB memory, and a 512-process limit.
The log budget is 128 MiB, including three bounded 1 MiB Docker log files.
This source audit is not test evidence. Validation receipts will record actual usage and results.

## Keep Context Supplied After the Initial Audit

The user directs CAS design to Keep, Echo's eventual storage backend.
Keep `001ae2a5babdbdab11c0c8c4fec3829dfc44f1dd` describes content-addressed storage and conditional retention publication.
Its reference adapter is explicitly non-durable. Its transition planner checks expected generations and predecessor identity without I/O.
[F07](F07.md) records the source and separates storage identity from the application's compare-and-set effect.
This context does not establish a current Echo-to-Keep integration or prove durability for the new effect.

## Independent Plan Review

[Review reconciliation](REVIEW.md) records each accepted, qualified, and disputed finding.
The complete original agy feedback is preserved in Reader receipt `b8b6b30b-e230-48b7-95b5-87f70b6f2d0c`.
The reviewer requested changes to the plan. It gave no implementation or merge approval.
