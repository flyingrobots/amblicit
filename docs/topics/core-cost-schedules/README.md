# Core cost schedules

This shelf owns how Core evaluation is converted into the steps and allocated
bytes that an intent's `coreEvaluationBudget` bounds
([EDICT-LANG-COST-SCHEDULE-001]). A module is compiled and admitted under one
of two schedules.

| Schedule | Selected by | Core marker |
| --- | --- | --- |
| Copying, `edict.core-cost/v1` | Default | None |
| Shared-read, `edict.core-cost/v2` | `"coreCostSchedule": "edict.core-cost/v2"` in `edict.application/v1`, or `CompilerContext::with_core_cost_schedule(CoreCostSchedule::SharedReads)` | Required capability `core.cost.shared-reads/v1` |

## Copying schedule

Every evaluated value is charged at its full type size, in 64-byte cells.
Reading a local, projecting a field, indexing a list, passing an argument and
carrying fold state each cost as much as copying the whole value. This mirrors
Echo's copying pure evaluator. It is unchanged by the shared-read schedule.

## Shared-read schedule

Values are immutable and shared by reference.

- Reading a local, a parameter, a field, a list element (`listAt`) or fold
  state costs one step and allocates nothing. So do `if`, `match`, `fold`, a
  call and `fail` themselves.
- A call costs one step per parameter plus one for the result, in addition to
  the callee's body. A fold costs one step per iteration plus the step call,
  for every iteration of its bound.
- Building a value charges its shell, `cells + 1` steps and `bytes`
  allocated: a record is one cell plus one per field (64 bytes plus 64 and the
  key length per field); a list literal, `listAppend` or `listConcat` result is
  one cell plus one per item slot up to the declared maximum, 64 bytes each; a
  variant is seven cells and `463 + type name + case` bytes; a scalar constant
  or `listLen` result is one cell. Shared children are not charged again.
- A match payload binder costs one step; dispatch costs one step per arm plus
  one.
- Unchanged from the copying schedule: comparisons scan both operands in full,
  intent input and output are charged at full size, and every other
  operation (byte, text, integer, Option, effect and target intrinsic) keeps
  its copying charge.

A new list shell (`listAppend`, `listConcat`) is sized by the largest length
the list can provably have, when one is known, instead of its declared
maximum. The meter tracks length bounds through locals, record fields,
branches (the larger arm, and only when both arms prove a bound), calls and
folds. A list literal of `n` items has bound `n`; an append adds one and a
concatenation adds the right operand's bound, both capped at the declared
maximum. A list from intent input, an element read, a foreign call or any
other source has no proven bound and is charged at its maximum. A fold is
charged round by round under the bounds the previous round produced, and
once a round's bounds stop changing every remaining round is charged at that
round's cost. Every round of the bound and every arm's maximum are still
charged.

The source checker admits a shared-read budget with native Target's meter in
`target_ir/fold_work/shared_reads.rs`, so source and Target minimums are equal.
Native Target applies the schedule only when Core requires the capability.
Removing the capability makes Target apply the copying schedule, which refuses
the smaller budget with `InvalidCoreIdentity`.

A call to an imported lawpack helper or a target pure intrinsic runs outside
the shared heap. Each argument is charged as a full copy in and the result as
a full copy out, as under the copying schedule, plus the callee's declared
cost template: the lawpack adapter's helper budget, or the intrinsic's
`base + perArgumentMax × operand maximum` rule. The source checker reads the
same authenticated helper and intrinsic facts that native Target receives.

Native Target's separate intrinsic-cost check follows the same loop rule:
under the shared-read schedule it charges a literal-bound `for` body once per
iteration instead of refusing an intrinsic reached from a loop. Under the
copying schedule an intrinsic inside a loop body is still refused.

The meter has no rule for an external-action request or a `for` loop over a
non-literal bound, so a shared-read module containing one is refused with
`InvalidBound`. A refused shared-read budget reports the static steps and
allocated bytes it needs.

## Effect on the bounded-descent fixture

For `fixtures/lang/bounded-descent/avl-rope.amblicit`, measured by
`copying_schedule_keeps_the_rope_minimum_budgets` and
`shared_read_schedule_cuts_the_rope_budget_by_two_orders`:

| Schedule | Minimum steps | Minimum allocated bytes |
| --- | ---: | ---: |
| Copying | 647,943,859 | 1,805,252,203 |
| Shared-read | 1,324,669 | 11,615,315 |

## Effect on Jedit's rope modules

Measured through the public CLI with `jedit#410`'s Octets, Codec, Identity and
Rope modules (head `10b8d82`, intrinsic import retargeted to the measuring
provider's profile) and one entry intent that calls `rope.join`, `ids.leafId`
over `codec.leafAtom`, and a lawpack helper. The shared-read minimum is
exact: one step less is refused.

| Schedule | Minimum steps | Minimum allocated bytes |
| --- | ---: | ---: |
| Copying | 6,457,252,919 | 16,528,953,978 |
| Shared-read | 1,570,430 | 26,468,042 |

`join` alone needs 1,505,486 steps and 25,854,213 bytes under the shared-read
schedule. The shared-read byte figure is higher than the step figure suggests
because a list shell is charged at its declared 512-node maximum on every
append; it is still about 600 times smaller than the copying figure.

## Consumers

A consumer that executes shared-read Core must meter it at no more than this
schedule, and its static checks must use the same rules. Echo's evaluator and
provider lowerer do not yet do so; they recognise only the capabilities they
implement, so they refuse shared-read modules.

## Soundness evidence

A test-only reference evaluator
(`crates/amblicit-syntax/tests/core_cost_schedules/reference.rs`) meters this
schedule on actual values. Adversarial programs run at their minimum accepted
budgets on worst-case inputs and never exceed them; the
[test plan](test-plan.md) lists them. The [rationale](rationale.md) gives the
argument and rejected alternatives.
