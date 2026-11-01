# Interactive task dependency map

Open [index.html](index.html) directly in a browser. It is one self-contained
file: no server, installation, CDN or network access is needed. GitHub issue
links open only when followed. Use the map to inspect prerequisite paths and
the List view to read tasks without navigating the canvas.

Project colors identify Amblicit, Echo and Jim/Jedit. Search and project
spotlights retain the surrounding dependency context. Selecting a task exposes
the reasons for its direct prerequisites and dependents; the scope selector
can isolate prerequisites, dependents or both. These views never synthesize
transitive shortcuts. On narrow screens the map opens at a readable zoom on
the entry tasks; **Fit** shows the whole graph.

## Data boundary

[snapshot.json](snapshot.json) is a dated capture of native GitHub issue
dependencies. Its `observedAt` field is the retrieval time, not a build time or
a promise of live status. Every edge is oriented **prerequisite → dependent**
and was checked against both `blocked_by` and inverse `blocking` responses.
Parent/sub-issue links supply the node inventory only. The roadmap container
is excluded, while the external full-rope acceptance issue is included.

The snapshot does not add edges for project membership, conceptual grouping,
shared files, shared workers, or placement in the display. An entry task means
zero recorded incoming edges; it does not assert merge readiness. Node titles
come from GitHub. Short labels, outcome summaries and dependency reasons are
editorial annotations from the roadmap and are not a separate live tracker.

GitHub remains authoritative. Refresh the snapshot from its native dependency
API when the roadmap changes; preserve edge direction, include any newly
referenced blockers, check inverse relationships, and record the new retrieval
time. Do not infer prerequisites from Markdown links or the parent hierarchy.

## Rebuild

The Python standard library is sufficient. From this directory:

```sh
python3 build.py
python3 build.py --check
```

The builder validates endpoints and edge kinds, rejects duplicate/self/cyclic
dependencies, computes a deterministic layered layout, and embeds the data
and interface into `index.html`. Routing slots are geometry only; they are
never rendered as tasks or added to the semantic graph. The generated HTML
is the deliverable. Keep [page.template.html](page.template.html) and
[build.py](build.py) for future edits rather than modifying the generated file.

## Verification boundary

Browser checks cover exact edge-set/direction fidelity, exclusion of hierarchy
and synthetic edges, project colors, prerequisite focus, search/empty states,
the text alternative, pan/zoom, keyboard interaction, narrow-screen layout,
both themes and operation without network requests. These checks validate this
visualization, not compiler, provider, runtime or rope correctness.

With Node and Playwright already installed, run `node verify.mjs` from this
directory. It compares rendered edges with the checked snapshot; it does not
refresh GitHub. `PLAYWRIGHT_MODULE` can name an existing Playwright module and
`CHROMIUM_EXECUTABLE` can select an existing compatible Chromium executable.
No browser or package is installed by the check. Set `DAG_CHECK_OUTPUT` to an
owned scratch directory if screenshots are wanted; otherwise no screenshots
are retained. The check closes its browser and has a one-minute deadline.

This is a generated illustration of a plan, with no normative language or ABI
change. The current-truth topic shelves remain unchanged. The
[implementation rationale](../collections-optics-roadmap.md) explains the plan;
the [GitHub roadmap](https://github.com/flyingrobots/amblicit/issues/288) owns
live execution status and acceptance.
