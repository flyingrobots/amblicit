# Independent Plan Review Reconciliation

This page records decisions from agy's read-only review on 2026-10-07.
The reviewed planning head is `ac656c4996a89b4e091bf65dadc6bf8c8a53ed65`.
The reviewer also inspected the incomplete F01 worktree and its test logs.
The exact feedback has SHA-256 `711358cf76b23c1be659b407accb3d6c8f8c9d914e6f5238e0e3e198bbbfcb71`.
Reader receipt `b8b6b30b-e230-48b7-95b5-87f70b6f2d0c` preserves the original report and prompt.
The report verdict is `REQUEST CHANGES`. It is not implementation or merge approval.

## Decisions

| Review item | Disposition | Evidence and plan decision |
| --- | --- | --- |
| F01: failing experimental tests | Accepted | Retained logs show a byte_concat parser failure and a projection budget mismatch. F01 requires their correction and the full gate before readiness. Those experimental failures were corrected after the plan review. Main has not received this change. |
| F01: split compiler check from required fixture corrections | Disputed | The lawpack generator compiles source witnesses before emitting artifacts. The current greeting type needs 1,299 bytes against 512. A compiler-check-only merge rejects that witness. Keep the correction and full digest closure together. A separate preparatory budget PR could work, but no split is necessary for one coherent output-safety outcome. |
| F01: exact size | Accepted with correction | The independent encoder witness produces 16,662 bytes. CBOR text headers use major type 3, not the byte-string header examples in the report. The size calculation remains correct. |
| F01: memory/wire distinction | Accepted with correction | `VALUE_CELL_BYTES` is 64 in `crates/edict-syntax/src/compiler/source_functions.rs#11@01161c1745baad0d713234ba1a671b9a26923aa4`. Nine bytes bounds its CBOR headers and integer cells. The report incorrectly calls nine bytes an allocation cell. Imported helper output costs can still contribute without source functions; only the declared return frame is omitted on that path. Neither accounting model proves physical backend memory. |
| F02: separate independently verifiable outcomes | Accepted | F02A owns primary effect resolution, F02B owns signature detail, and F02C owns recovery. #232 becomes a container. The report proposes two children in one section and three elsewhere; the final plan uses three with complete ownership. |
| F02A -> F02C edge | Disputed | Recovery can suppress a dependent error while preserving any existing primary kind. Primary kinds can improve without recovery. Each has an independent acceptance witness. No code prerequisite is established. |
| F03: source/request coordinates | Accepted | Diagnostic `line` currently means the JSONL request line in `docs/schemas/edict.cli-diagnostic.v1.schema.json`. Keep it. Add a separate sourceLocation and byte span. Validate build diagnostic/event schemas. The report's `schemas/cli/diagnostic.json` and `src/operations/application_build.rs` paths do not exist here. |
| F04/F05: status and digest guidance | Accepted | Preserve exact producer locks and failure kinds. The formal spec already describes domain separation; the gap is entry-point guidance. |
| F06: no compiler lawpack parameters | Accepted | The existing `edict.lawpack-authoring/v1` API supplies types and adapters. The report's `edict.lawpack-authoring.manifest/v1` identifier is incorrect. Use application-owned derivation with explicit scalar and byte limits. |
| F07/F08: premature artifacts | Qualified | The original tasks already required compatible merged Echo support before fixture changes. No CAS/key-limit artifacts have been emitted by this work. Replace unresolved descriptions with actual E01/E02 issues and exact provider-package gates. |
| F08: unknown-field rejection | Accepted | Echo create `validate_configuration` reads known fields without universal extra-field rejection. An unhandled limit field can be ignored. Require corroboration, runtime enforcement, and provider/version admission. |
| F07: Keep terminology | Qualified | The original F07 already distinguishes the two meanings and states the reference adapter/planner limits. Strengthen the digest distinction and cite Keep origin/main `3165890e9291cfb5fe10e81a9d7cd151f3e59464`. Keep adoption is related Echo #722/#760 work, not an established prerequisite. |
| Raw study execution and praise | Qualified | Raw runtime results remain unavailable. Source and local compiler logs do not reproduce the reported Echo commits or timings. The report's checklist wording overstates that evidence. |
| STE compliance | Qualified | Frontmatter and template structure exist. No full Issue 9 dictionary audit occurred. Simple drafting is not certified compliance. |

## Evidence Limits

The reviewer inspected source and retained logs. It executed no tests.
The review checklist is useful coverage evidence, but contains wrong paths and overstatements noted above.
No accepted finding changes runtime authority by itself.
The final implementation still needs deterministic evidence, full local verification, current-head independent review, required bot review, and CI.
No review grade or absence of comments authorizes a broken merge.

## Final Task Graph

F01 precedes F06 and F07.
E01 and E02 precede F07. E02 precedes F08.
All other task pairs have no unconditional implementation edge.
Each task must preserve already merged contracts at its execution time.
The twelve executable tasks exclude the F02 tracking container.
