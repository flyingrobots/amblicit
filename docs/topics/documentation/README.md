# Documentation Standards Topic

Status: current HEAD workflow contract.

Documentation is a product interface. Its job is to help a specific reader do a
specific task, not to prove that the repository contains enough Markdown.

This policy adapts the reusable reader-task documentation standard to Edict's
current repository shape. It does not mass-convert existing pages. It gives new
and changed documentation a direction that fits the topic-shelf, fixture, spec,
and release workflow already in this repository.

## Reader jobs

Each documentation page should have one primary reader job. A page may link to
other jobs, but it should not try to be all of them at once. [DOCS-REQ-001]

Edict uses these page types:

- tutorial: a guided first-success path that teaches by doing;
- how-to: a task guide for a competent reader trying to finish one job;
- reference: exact facts such as syntax, schema fields, release gates,
  requirement IDs, fixtures, commands, and error identities;
- explanation: mental models, mechanisms, tradeoffs, and design boundaries;
- troubleshooting: symptom-led diagnosis and recovery;
- contributor: architecture, edit paths, invariants, impact rules, and
  verification evidence.

Topic shelves in `docs/topics/` are contributor and evidence material first.
They may contain explanation or reference facts, but they are not a substitute
for user-facing tutorials or task guides for directly operated surfaces such as
the `edict` CLI. [DOCS-REQ-002]

## Edict documentation shape

Edict's current documentation set has these roles:

- [README.md](../../../README.md): product landing page and high-level
  explanation.
- [docs/README.md](../../README.md): documentation router.
- `docs/SPEC_*.md`: normative protocol, language, ABI, and admission reference;
  `docs/SPEC_edict-language-v1.md` is the maintained formal language and Core
  semantics contract.
- [docs/REQUIREMENTS.md](../../REQUIREMENTS.md): requirement and fixture
  registry.
- `docs/topics/*`: current-truth contributor/evidence shelves.
- `docs/releases/*`: versioned release notes.
- [CONTRIBUTING.md](../../../CONTRIBUTING.md): contributor entry point.
- [AGENTS.md](../../../AGENTS.md): agent workflow entry point.

New pages should extend this structure by reader job. Do not add a new omnibus
README when a focused tutorial, how-to, reference, explanation,
troubleshooting, or contributor page is the clearer shape. [DOCS-REQ-001]

## Documentation Coverage Matrix

Coverage is selected by capability. "Not needed" requires a reason; it does not
require an empty placeholder page. [DOCS-REQ-003]

| Capability | Audience | First success | Task guides | Reference | Explanation | Troubleshooting | Contributor |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Edict language | user, integrator, maintainer | planned when a runnable compiler surface exists | planned around compile/canonicalize tasks | current specs and requirement registry | current README and assurance guide | gap until user-facing failures exist | current topic shelves |
| Syntax and surface validation | maintainer, integrator | not needed: library surface is narrow and test-driven | optional parser/validator recipes | current syntax and semantic-validation shelves | current topic READMEs | gap for diagnostics beyond stable error kinds | current test plans and fixtures |
| Compiler spine and Core IR | maintainer, integrator | planned for source-to-Core/canonicalization once #21 lands | planned for canonicalize and inspect workflows | current compiler-spine and Core IR shelves | current topic READMEs and specs | gap for compiler-stage diagnostics | current test plans and CDDL fixtures |
| CLI | user, integrator, maintainer | current JSONL `check` smoke path | current CLI topic | current CLI topic and CLI stream-contract JSON Schemas | current CLI topic | current structured stderr diagnostics | current CLI test plan |
| Fixtures and golden artifacts | maintainer, reviewer | not needed: fixture use is contributor workflow | current fixture topic and Core golden regeneration notes | current fixtures topic and `fixtures/README.md` | current topic README | gap for fixture-generation failures beyond Core goldens | current fixture and Core IR test plans |
| Lawpacks and assurance | integrator, maintainer | planned when executable lawpack or assurance tools exist | planned around lawpack validation and assurance explainers | current lawpacks and assurance shelves plus specs | current topic READMEs and assurance guide | gap until executable tools exist | current lawpacks and assurance test plans |
| Release process | maintainer | not needed: release is operator workflow, not newcomer product use | current release process topic and release notes | current release workflow and roadmap | current release-process shelf | current tag recovery policy, limited to known release failures | current release-process test plan |
| Contributor workflow | maintainer, agent | current `CONTRIBUTING.md` and `AGENTS.md` orientation | current testing and documentation workflow topics | current local verification commands | current topic-shelf policy | gap for local environment failures | current `xtask` evidence |

When a capability gains a CLI, public API, generated reference, visual UI, or
operational failure mode, update this matrix before claiming the documentation
set is complete. [DOCS-REQ-003]

## Examples

Examples are contract material. Runnable examples must be valid, use supported
behavior, include required context, and show expected output when the output is
part of the reader's task. Illustrative examples must be labeled or clearly
framed as illustrative. Abridged examples must explain what was omitted when the
omission could affect interpretation. [DOCS-REQ-004]

Copyable shell commands must not include prompts. Present commands and output as
separate blocks. Use `text` for exact or representative output, and say which
parts vary when output is nondeterministic. [DOCS-REQ-004]

Use safe fictional values such as `example.com`, `example-target`, and
`test_token_example`. Placeholder digest prose such as `sha256:...` is useful in
explanations, but runnable Edict fixtures must use lexable digest strings. The
fixture corpus documents that distinction. [DOCS-REQ-004]

## Change impact

Documentation must move with behavior. A contract-bearing change must do one of
the following before it is claimed complete:

1. update affected documentation;
2. show that existing documentation remains accurate;
3. declare `docs-impact: none` with a concise rationale.

Source syntax, static-semantics, compiler-visible language, or Core semantic
changes must update `docs/SPEC_edict-language-v1.md` in the same pull request.
Wire-shape changes must also update the coupled normative CDDL, and changed
claims must update their owning requirement and test-plan evidence. Executable
tests demonstrate conformance; they do not silently replace the formal
language specification.

Changed documentation must preserve page type. Do not turn a reference page into
a tutorial, or a topic-shelf contributor page into a marketing page, as an
incidental side effect. [DOCS-REQ-005]

Do not copy live issue lists, pull request lists, CI timestamps, or dashboards
into prose as current truth. Link to live systems or use generated artifacts
when those facts matter. [DOCS-REQ-005]

## Durable Decision Discipline

Important decisions are incomplete until their durable owner is current.
Architecture, authority, identity, canonical-format, recovery, compatibility,
ownership, public-API, and release-boundary decisions MUST be recorded in the
same change in the canonical topic shelf, specification, requirement, or release
document that owns the concept. Chat transcripts, Think memories, pull-request
prose, and review threads may explain or motivate a decision, but they are not
its canonical repository home.

For every such decision:

1. Identify one canonical owner before completing the change. Prefer the
   relevant `docs/topics/<topic>/README.md` for current behavior,
   `architecture.md` for machinery, a normative `docs/SPEC_*.md` or ABI schema
   for protocol law, and `test-plan.md` for planned and implemented evidence.
2. Record the accepted rule and whether it is implemented or planned. Put
   implemented behavior in its current-truth owner. Put target behavior in
   explicitly planned `test-plan.md` rows or a linked design proposal; a topic
   `README.md` may link to that future work but must not describe it as current
   behavior. Record the decision's refinement, supersession, dependency, and
   related-document edges in the page that owns its actual posture.
3. Update `docs/topics/README.md`, `docs/README.md`, or another relevant router
   when a durable page or topic shelf is added, moved, or renamed.
4. Link to the canonical owner from reader-specific pages instead of copying
   the same rule into several places.
5. Keep implementation checklists, review state, and delivery status in GitHub.
   Current topic shelves describe branch or HEAD truth; they are not a second
   project tracker.
6. Revisit the same canonical owner whenever later work refines the decision.
   A refinement is not complete while code, schemas, packages, fixtures, or
   release behavior disagree with the documented rule.

Treat missing or stale canonical decision documentation as incomplete
engineering work, not optional polish. Historical design and release documents
remain evidence; update the current owning shelf rather than silently relying
on an old decision record. [DOCS-REQ-007]

### Decision relationship format

Place a two-column `Relationship` / `Targets` table beside the decision's rule
and posture. Include all four field names below, in this order. Each target is
a Markdown link to the canonical document or section; use the requirement or
decision ID as the link label when one exists. Separate multiple targets with
commas. Write the literal `none` when a relationship has no targets; an omitted
row or blank cell is incomplete. [DOCS-REQ-007]

| Field | Meaning from this decision to its target |
| --- | --- |
| `refines` | Adds detail or a narrower rule while the target remains authoritative. |
| `supersedes` | Replaces the target rule. Preserve its history and identify this replacement in the prior owner. |
| `depends_on` | Requires the target contract to hold for this decision to hold. |
| `related` | Provides relevant context without asserting refinement, replacement, or dependency. |

For example, this durable-decision policy has current posture and the following
relationships. It refines the documentation-impact rule by requiring a named
canonical owner; it depends on the existing topic-shelf contract.

| Relationship | Targets |
| --- | --- |
| `refines` | [DOCS-REQ-005](./test-plan.md#requirements) |
| `supersedes` | none |
| `depends_on` | [Topic shelf contract](../README.md) |
| `related` | [Review process](../review-process/README.md) |

These edges are reviewed as policy metadata. Link and topic checks verify the
local references and evidence records; they do not prove that a claimed
refinement, supersession, or dependency is semantically correct.

## Deterministic checks and editorial review

The local gate already checks links, topic metadata, evidence names, fixture
paths, status values, doctests, Rust tests, formatting, and diff whitespace.
Markdown lint is available for changed Markdown files. These checks block on
facts the repo can determine reliably. [DOCS-REQ-006]

Human review is still required for reader-task quality. A page is not done only
because Markdown syntax passes. Review should ask whether the intended reader can
complete the page's primary job without source archaeology. [DOCS-REQ-006]

The verification matrix is tracked in [test-plan.md](./test-plan.md).

## Literal Execution Evidence

Literal command output under `docs/plans/study-feedback/evidence/` retains its
original bytes, including terminal blank lines. The scoped Git attribute
permits that EOF form for these `.txt` artifacts. Ordinary source retains its
EOF whitespace check. Evidence indexes distinguish failed, partial, superseded,
and passed runs; a retained file is not automatically a passing claim.
[DOCS-REQ-008]
