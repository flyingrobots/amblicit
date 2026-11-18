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
