# Roadmap Visualization Topic

Status: current implementation contract for the offline task-graph viewer.

This shelf owns the working documentation tool: snapshot validation, graph
analysis, HTML generation, and interactive exploration. The collections/optics
language and runtime work shown inside the graph remains
[planned design](../../design/collections-optics-roadmap.md).

For use and rebuild instructions, read the
[artifact guide](../../design/collections-optics-dag/README.md) or open the
[standalone page](../../design/collections-optics-dag/index.html). The
[test plan](test-plan.md) maps actual Python/browser evidence to requirements,
cases, fixtures, and oracles, and states the Rust-only evidence registry's
integration gap.

## Graph and analysis boundary

The builder reads a dated JSON snapshot. It rejects duplicate supplied GitHub
issue identities as well as unbound, duplicate,
reflexive, cyclic, and nonprecedence edges before producing the page. Every
displayed arrow is a supplied prerequisite-to-dependent relationship. Project
membership, workstreams, native parent hierarchy, and routing geometry do not
add semantic edges. [RDAG-REQ-001]

Earliest Kahn generations provide preview antichains. Workstream membership
covers every task exactly once. Reachability matching produces an exact-width
antichain and chain-cover certificate; the independent tests check both, and
distinguish maximum width from largest generation. The longest-path result
uses real direct edges and reports task/edge counts rather than time.
[RDAG-REQ-002]

## Viewer and readiness boundary

The generated HTML embeds its data, styles, and interactions. Map, List, and
Waves views provide project colors, context-preserving search, prerequisite and
dependent focus, keyboard selection, pan/zoom, JSON export, theme switching,
and narrow-screen layouts without a server or network access. Following a
GitHub link deliberately leaves the offline view. [RDAG-REQ-003]

Resizing preserves a reader's pan or zoom until Fit or Reset restores automatic
fitting. Completing a simulated task moves keyboard focus to the same card's
enabled task button. [RDAG-REQ-006]

The optional readiness preview stores assumptions in page memory. Finishing
one eligible task can enable its consumers immediately; unrelated members of
an earlier preview wave do not hold them back. Eligibility still requires all
of a task's own predecessors. The preview does not verify accepted completion,
reserve resources, run a worker, persist task state, or change GitHub.
[RDAG-REQ-004]

## Generation and authority

The Python builder embeds escaped JSON into one template insertion point and
reproduces the checked-in page from fixed inputs. Its check mode compares that
output without rewriting the file. A rebuild does not refresh the GitHub
snapshot. [RDAG-REQ-005]

The snapshot is the authority for this artifact's displayed edge set, while
[roadmap #288](https://github.com/flyingrobots/amblicit/issues/288) is the live
tracker. A task with no recorded blocker is not thereby proved independent or
ready to merge. The viewer's tests establish tooling behavior, not the
compiler/provider/rope outcomes represented by its task cards.

| Relationship | Targets |
| --- | --- |
| `refines` | none |
| `supersedes` | none |
| `depends_on` | [Documentation standards](../documentation/README.md), [Testing workflow](../tests/README.md) |
| `related` | [Planned collections rationale](../../design/collections-optics-roadmap.md), [Live roadmap](https://github.com/flyingrobots/amblicit/issues/288) |
