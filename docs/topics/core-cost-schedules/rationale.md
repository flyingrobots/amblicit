# Core cost schedule rationale

Issue #324 asks for bounded list reads to be charged by the work they do, so a
rope-sized descent fits a budget. This page records the design and the
alternatives rejected. The [README](README.md) states what is current.

## The constraint: the runtime meters copies today

The static estimate is only sound if no conforming runtime does more metered
work than it predicts. Echo's generic pure evaluator
(`crates/warp-core/src/edict_pure/evaluate.rs` at Echo `b611df32`) meters a
copying evaluator:

- reading a local calls `meter.copy`, which charges every cell of the value;
- `core.list.at` validates the whole list against its type before indexing;
- helper arguments and fold state are validated and copied on every call.

So Edict's current static rules (`edict.core-cost/v1`) mirror real runtime
work, and simply lowering them would admit programs that Echo then refuses
with `StepBudgetExceeded`. Echo's provider lowerer also recomputes the static
cost (`source_functions.rs`, `cost.steps > maxSteps`), so today it would refuse
such packages at build time.

## Decision: a second, selected schedule

Keep `edict.core-cost/v1` unchanged as the default. Add `edict.core-cost/v2`,
the shared-read schedule, which an application selects explicitly. A module
compiled under v2 requires the Core capability `core.cost.shared-reads/v1`.
The capability is hash-significant and fails closed: a consumer that does not
implement v2 metering refuses the module instead of running it under a budget
sized for v2.

Under v2 values are immutable and shared by reference. Only building a value
costs work proportional to its size. Reading a local, a parameter, a field, a
list element or fold state, passing an argument and returning a result each
cost one step and allocate nothing. Building a value charges only its shell,
because its children are shared. Byte, text, integer and other operations
keep their v1 charges. An earlier draft kept full v1 charges for building
records and lists; that still copied the whole arena into every new fold
state, so it was dropped.

The source checker and native Target use one meter for v2, so their minimum
budgets are equal by construction. Target still recomputes it from Core alone
and trusts no compiler estimate.

## Soundness argument

In a sharing evaluator, each evaluated Core node does constant work apart from
building its result. The static estimate charges every node at least one step
on every evaluation the bounds allow, and charges every building node the
maximum size of its result type. That is at least the actual size, because
the type's cell and byte formula dominates any value of the type. Branches
take the maximum, folds multiply by the full bound and loops by their declared
bound, so the estimate dominates every path. Allocation is charged as the sum
of everything built, which bounds peak live bytes from above.

The executable check is a test-only reference evaluator that implements the v2
metering rules from this page directly, not from the static meter. Adversarial
programs (deep nesting, large shared state, worst-case branches, maximum list
fills) run on worst-case inputs, and the test asserts metered work never
exceeds the accepted budget.

## Alternatives rejected

| Alternative | Why rejected |
| --- | --- |
| Change v1 in place | Unsound against Echo's copying runtime until Echo changes, and it silently changes the meaning of every published budget. |
| Charge `listAt` by element size only, leaving local reads at full size | Leaves the dominant cost: every helper call and local read still copies the arena, so the fixture improves only by a small factor. |
| Select v2 from the target profile | The cleaner home in the long run, but the profile manifest schema is frozen in published packs. A Core capability fails closed without a schema change. |
| A separate compiler-side v2 accounting | Two implementations of one schedule invite drift; sharing the Target meter makes the minimums equal by construction. |

## Reachable-size bounds for fold-grown values (#335, design)

Under the shared-read schedule a fold is charged `N` times its step's cost,
and the step's cost charges each `listAppend` or `listConcat` result shell at
the list type's declared maximum. A fold that appends one node per round to a
512-node arena is therefore charged 513 slots on every round, even though by
round `i` the arena can hold at most `L0 + i` items. Nested folds multiply the
error: the rope's `cut` (66 rounds) calls `join` (66 rounds) on the arena it
is growing.

### Level 1: size-bounded abstract evaluation of folds (implemented)

The meter evaluates a pure function against an abstract environment that
gives every list value an upper bound on its length, and every record its
fields' bounds, instead of assuming each value's type maximum:

- a list literal of `n` items has bound `n`;
- `listAppend(xs, x)` has bound `min(max, bound(xs) + 1)`, and
  `listConcat(a, b)` has bound `min(max, bound(a) + bound(b))`;
- a local, field, `listAt` element or call result carries the bound of the
  value it names; any other list expression falls back to its declared
  maximum;
- `if` and `match` take the larger bound of their arms, field by field.

A fold is charged round by round. Round `i` charges the step's cost under the
abstract state that round `i - 1` produced, and the next state's bounds are
the step's abstract result. Bounds never exceed the declared maximum, so the
abstract state stops growing once it saturates; from then on every remaining
round is charged at that fixed point. A call is charged under its arguments'
bounds, and results are cached per function and abstract argument tuple.

Soundness: by induction over rounds, every concrete value's length is at most
its abstract bound, and every shell, comparison scan and copy is charged by
an abstract bound that is at least the concrete size. Charging still covers
every round of every fold, and every arm's maximum, so no early exit or
branch pruning is assumed.

Native Target runs the same meter on Core alone. The source checker keeps
admitting with that meter, so the two minimums stay equal.

### Level 2 and level 3

Level 2 refines record sizes through constructors where level 1 still charges
a type maximum (byte and text payloads, nested records in lists). Level 3, an
amortised bound across nested folds, needs a source construct and will be
proposed separately before any implementation. Neither is started until level
1 is measured.

### Level 1 result

Measured on jedit#410 at `cacd74c` (whole ReplaceRange application), level 1
moves the shared-read minimum from 484,963,522 steps and 1,245,181,033 bytes
to 484,726,795 steps and 1,230,030,505 bytes. The rope's arena comes from the
closure read, which is intent input with no proven length, so its appends
stay charged at the 512-node maximum. Level 1 is sound and tightens folds
over lists built from literals, but it does not fit the rope under
`text.replaceRangeBudget`.

## Effect cost templates (#341, design)

Echo's lowerer charges effect frames (atom reads, mutation emissions, graph
read records, scans and closure costs) that Amblicit's meters do not, so a
compiler-minimum effect program is refused. Option (a), decided on
2026-11-19: the provider's intrinsic catalog carries a cost template per
effect intrinsic, and the compiler charges it under every schedule. Echo's
metering and its v1 goldens do not change. The format below was agreed with
echo#1016.

### Catalog

`edict.target-profile.intrinsics/v3`: every `effect`-class entry carries a required `effectCostRule`. Pure `costRule` is unchanged. `/v2` catalogs and frozen packs stay admissible byte for byte. Without a template, Amblicit charges effects exactly as today, so v1 goldens are unchanged.

### Shape

```cddl
effect-cost-rule = { steps: cost-expr, bytes: cost-expr }

cost-expr = { const: uint }
          / { sum: [2*16 cost-expr] }
          / { product: [cost-expr, cost-expr] }
          / { measure: measure }

measure = { bytesMax: type-expr }                          ; Bytes<max=N> -> N
        / { listMax: type-expr }                           ; List<_, max=N> -> N
        / { copy: type-expr, of: "steps" / "bytes" }       ; Echo shape().0 (below)
        / { validations: type-expr }                       ; Echo shape().1 (below)
        / { parameter: tstr }                              ; named u64 the provider publishes
                                                           ; per operation configuration,
                                                           ; e.g. "maxEdgeScans"
        / { costliestHelper: "steps" / "bytes" }           ; most expensive source helper
                                                           ; in the module
        / { helper: { parameter: tstr }, of: "steps" / "bytes" }
                                                           ; one call to the helper a `helper`
                                                           ; parameter names (shared-read only;
                                                           ; copying charges the costliest)

parameters = { * tstr => { path: [+ tstr], kind: "uint" / "helper" } }
                                                           ; top-level catalog map: where each
                                                           ; parameter lives in the operation
                                                           ; configuration

type-expr = { path: { side: "input" / "output", fields: [* tstr], item: uint, ? then: [+ tstr] } }
                                                           ; record fields, then `item` list-item steps,
                                                           ; then (only when item >= 1) record fields
                                                           ; inside that item
          / { word: null }                                 ; Word(u64::MAX)
          / { record: { * tstr => type-expr } }            ; a synthesized record, e.g. the
                                                           ; closure selector's candidate
```

**`copy` and `validations` follow Echo's `shape()` exactly:**

| Type | copy (steps, bytes) | validations |
| --- | --- | --- |
| Bool, Word | (1, 64) | 1 |
| `Bytes<max=N>` | (1, 64+N) | 1 |
| `Option<T>` | (T.steps+1, T.bytes+64) | T+1 |
| `List<T,max=N>` | (T.steps·N+1, T.bytes·N+64) | T·N+1 |
| Record | (1 + Σ(field.steps+1), 64 + Σ(field.bytes+64+len(name))) | 1 + Σ field |
| Variant | max(payload, (1, 64)) + (6, 6·64+15+len(identity)+max label) | max(payload)+3 |
| Nominal | its representation | its representation |

**Paths.** A path names a type, not a value. It starts at the effect's declared input or output type, steps through each name in `fields` (each must reach a record field), then `item` times into a list's item type, then through each name in `then`. Nominal types are seen through at every step. `then` may appear only when `item` is at least 1 and must not be empty, and a path map with any other key is refused. Because every item of a list has the same type, a measure of a path through an item gives the bound for *one* item, for example `bytesMax(output.nodes[].atomBytes)` = the atom byte limit of every returned node. A total over all items is written as a product with `listMax` of the list. Amblicit did not choose a "sum over all items" meaning: that total is already `product(listMax, …)`, and there is no subtraction or division to get the per-item bound back from a sum. `/v3` paths without `then` parse and evaluate as before.

**Evaluation:**
- Arithmetic is checked `u64`; overflow refuses.
- Expression depth is at most 16.
- Every path is validated against the effect's declared input and output types when the catalog is admitted.
- `costliestHelper` is Amblicit's own static cost of the module's costliest source helper under the selected schedule. Echo's conformance test must show that Echo's helper cost is at most that, so Amblicit stays at or above Echo.

### What Amblicit charges

For each effect node, the source checker and native Target charge:
- the input expression, as today;
- the result binding and obstruction annotations, as today;
- plus `effectCostRule`.

Amended for the shared-read schedule (#341, second half): there the template replaces the result binding and annotation scratch, so a shared-read template must cover them itself. One charge stays outside the template: when the step maps any obstruction arm, Amblicit adds (1 step, 64 bytes) once. This is Echo's empty obstruction payload, `shape(record {})` (echo `crates/echo-edict-provider-lowerer/src/executable_operation/source_functions/effect_block.rs`, the `obstructionMap` branch of the effect step), and a template cannot see a step's arms. Nullary annotation arms cost nothing more, and callable arms cost their expression. The copying schedule keeps the additive rule above.

Templates cover what Echo charges beyond that: input validations (`validations(input)`), the obstruction-arm scratch, frame records, scans and closure costs. Double charging is conservative. The compiler minimum is therefore at or above Echo's, and Echo admits every package at the compiler minimum.

### Echo's rules in this format (for its conformance test)

- **node-atom-read:** steps `bytesMax(output)+1`, bytes `2·bytesMax(output)+448`, plus `validations(input)` steps and the arm scratch.
- **Mutations:** 1 step and 64+280 bytes, plus `validations(input)` and the scratch.
- **outgoing-edge-read:**
  - range record: 1 step and 384 bytes;
  - scans: `parameter(maxEdgeScans)` × 1 step and `parameter(maxEdgeScans)` × (128+128) bytes;
  - result: `copy(output)` plus `validations(output)` steps.
- **closure-read:** `read_cost` (`read_cost.rs` 66–121) and the frame records (`effect_intent.rs` 373–386), term by term:
  - **start:** `validations(guide)` + `copy(guide)`;
  - **per node** × `listMax(output.nodes)`:
    - atom charge: (atom steps, atom+64 bytes), with atom = `bytesMax(output.nodes[].atomBytes)`;
    - node copy: `copy(node)`;
    - node record: (2, 384+atom);
  - **per occurrence** × `listMax(output.occurrences)`: (1, 0) + `copy(guide)` + `copy(occurrence)`;
  - **per edge** × `listMax(output.edges)`:
    - copies: `copy(edge)` + `copy(guide)` + 2·`copy(node)`;
    - candidate: `copy(candidate)` + (`validations(candidate)`+2, 0), where candidate = `record{guide, depth: word, slot: word, parent: node, child: node}`;
    - selector: `costliestHelper`;
    - returned guide: `copy(guide)`;
  - **ranges:** (1, 384) × `listMax(output.occurrences)` × `listMax(input.edgeTypes)`;
  - **result cells:** 256 bytes;
  - **scans:** `parameter(maxEdgeScans)` × (1, 128+128) + `validations(output)` steps.

Amblicit is implementing this on the PR for #341 and will charge these rules from a fixture `/v3` catalog until Echo publishes its own.
