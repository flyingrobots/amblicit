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
cost one step and allocate nothing. Building a record, list, variant or
constant, appending to or concatenating lists, and byte and integer
operations keep their v1 charges.

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
