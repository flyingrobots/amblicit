# Documentation Standards Test Plan

Status: current verification design for Edict documentation workflow.

This plan verifies that Edict's local documentation standard is discoverable,
reader-task oriented, and connected to executable contract checks.

## Scope

In scope:

- repo entry-point links to the documentation standards shelf;
- page type separation for tutorials, how-tos, reference, explanation,
  troubleshooting, and contributor guidance;
- Edict-specific documentation coverage by capability;
- example honesty, command/output separation, and placeholder rules;
- documentation impact declarations for contract-bearing changes;
- deterministic checks plus human reader-task review.

Out of scope:

- mass-converting existing documentation into a new directory layout;
- automatically proving prose quality;
- generating a full documentation catalog;
- building a public documentation site.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| DOCS-REQ-001 | policy | Documentation pages have one primary reader job and use the local page type vocabulary. | docs/topics/documentation/README.md |
| DOCS-REQ-002 | policy | Topic shelves remain contributor/evidence material and do not replace user-facing tutorials or task guides. | AGENTS.md, docs/topics/README.md |
| DOCS-REQ-003 | policy | Edict tracks documentation coverage by capability and reader need. | docs/topics/documentation/README.md |
| DOCS-REQ-004 | policy | Examples distinguish runnable, illustrative, and abridged use; copyable shell commands omit prompts. | docs/topics/documentation/README.md, fixtures/README.md |
| DOCS-REQ-005 | policy | Contract-bearing changes update affected docs or declare `docs-impact: none`; source-language and Core semantic changes keep the formal language spec, coupled CDDL, and owning evidence current in the same pull request; changed documentation preserves page type. | AGENTS.md, CONTRIBUTING.md, docs/SPEC_edict-language-v1.md, docs/topics/documentation/README.md |
| DOCS-REQ-006 | policy | Documentation quality uses deterministic checks for software facts and human review for reader-task success. | docs/topics/documentation/README.md, xtask/src/contract_check.rs, xtask/src/tests.rs |
| DOCS-REQ-007 | policy | Important durable decisions are complete only when their canonical repository owner and affected entry points are current in the same change. | docs/topics/documentation/README.md |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| AGENTS.md | Agent-facing documentation workflow entry point. | Links to the documentation standards topic and states impact rules. |
| CONTRIBUTING.md | Human-facing contributor entry point. | Links to the same documentation standards topic and summarizes reader-task practice. |
| docs/topics/documentation/README.md | Canonical local documentation policy. | Defines page types, coverage matrix, example rules, and impact rules. |
| docs/topics/README.md | Topic index. | Lists the documentation standards topic. |
| docs/README.md | Documentation router. | Links to the documentation standards topic. |
| fixtures/README.md | Fixture corpus contract. | Defines placeholder digest handling and golden fixture expectations. |
| xtask/src/contract_check.rs, xtask/src/tests.rs | Local documentation-contract verification implementation. | Provides contract-check behavior and regression tests; documentation policy prose is reviewed, not string-tested. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DOCS-TP-001 | policy | Contract discovery | DOCS-REQ-001, DOCS-REQ-002 | Review confirms the documentation standards policy is discoverable from contributor, agent, and docs entry points. | - | AGENTS.md, CONTRIBUTING.md, docs/README.md, docs/topics/README.md | Documentation detail; do not encode as a Rust test. |
| DOCS-TP-002 | policy | Page type policy | DOCS-REQ-001, DOCS-REQ-002 | Review confirms the documentation shelf names the reader-task model and local page types. | - | docs/topics/documentation/README.md | Policy detail; do not encode as a Rust test. |
| DOCS-TP-003 | policy | Coverage policy | DOCS-REQ-003 | Review confirms the documentation shelf contains an Edict coverage matrix. | - | docs/topics/documentation/README.md | Policy detail; do not encode as a Rust test. |
| DOCS-TP-004 | policy | Example and impact policy | DOCS-REQ-004, DOCS-REQ-005 | Review confirms the documentation shelf states runnable example rules, copyable shell command rules, formal language-spec/CDDL/evidence synchronization, `docs-impact: none`, and page-type preservation. | - | CONTRIBUTING.md, docs/SPEC_edict-language-v1.md, docs/topics/documentation/README.md, docs/topics/documentation/test-plan.md | Policy detail; do not encode as a Rust test. |
| DOCS-TP-005 | policy | Local gate policy | DOCS-REQ-006 | Review confirms deterministic checks are described as fact checks and behavior tests, with prose quality left to human review. | - | docs/topics/documentation/README.md, docs/topics/documentation/test-plan.md | Tool behavior is covered by existing `contract_graph_*` tests. |
| DOCS-TP-006 | policy | Durable decision ownership | DOCS-REQ-007 | Review identifies one canonical owner and rejects a change whose accepted decision exists only in chat, memory, PR prose, or a stale owning page. | - | AGENTS.md, docs/topics/documentation/README.md | Human workflow contract; existing link and topic checks verify discovery, not whether prose faithfully records a decision. |
| DOCS-TP-007 | policy | Decision posture | DOCS-REQ-007 | Review keeps implemented rules in current-truth pages and planned target rules in explicitly planned evidence or linked proposals; an implemented rule may link to future work without claiming it exists. | - | AGENTS.md, docs/topics/documentation/README.md | Resolve current-versus-target placement without turning topic READMEs into delivery trackers. |
| DOCS-TP-008 | policy | Decision relationships | DOCS-REQ-007 | Review checks all four relationship fields, linked targets with their relationship direction, and explicit `none` for absent edges; superseded rules identify their replacement. | - | docs/topics/documentation/README.md, docs/topics/public-rust-api/README.md | The canonical table format supports consistent human review; no semantic graph checker is claimed. |

## Determinism Obligations

- Documentation policy is reviewed as documentation, not asserted through prose
  string tests.
- Executable tests assert software behavior, such as validator acceptance or
  rejection of structured input.
- Contract graph validation behavior checks local links, requirement sources,
  evidence test names, and fixture paths.

## Open Gaps

- No `docs/catalog.yaml` exists yet.
- Automated coverage checking does not yet map public surfaces to page types.
- Executable tutorial harnesses are deferred until Edict has a user-facing CLI
  surface.

## Literal Execution Evidence

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| DOCS-REQ-008 | implemented | Literal command logs retain terminal blank lines under a scoped Git attribute; ordinary source retains its EOF whitespace rejection. | .gitattributes, docs/plans/study-feedback/evidence/README.md |

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DOCS-TP-009 | implemented | Literal evidence preservation | DOCS-REQ-008 | Git accepts the exact log terminal blank line and returns status 2 for the same EOF defect in ordinary source. | literal_execution_evidence_retains_source_whitespace_checks | xtask/tests/literal_evidence.rs, .gitattributes | Exercises Git behavior over the stable attribute artifact, not prose or file existence. Initial Python Docker witness observed RED before the scoped attribute. |
