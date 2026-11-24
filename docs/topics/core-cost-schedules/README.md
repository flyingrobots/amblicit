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
  intent input and output are charged at full size, and every prelude call
  that builds a new value (byte and text operations, integer operations,
  `Option` constructors and readers) is charged that value's full size as
  steps and allocated bytes, plus its copying-schedule call work. Effects and
  target intrinsics keep their own rules below.

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

## Effect cost templates

An effect node is charged its provider's cost template when the target's
intrinsic catalog publishes one (`edict.target-profile.intrinsics/v3`,
`effectCostRule` on each effect intrinsic). Under the shared-read schedule the
template replaces the generic charge of that step, and no result binding or
annotation scratch is charged. The full shared-read charge of an effect step
with a template is:

```text
effect step = input expression
            + template
            + (1 step, 0 bytes)    the effect node's own step
            + (1 step, 64 bytes)   if the step maps any obstruction arm
            + each callable obstruction arm's expression
```

The node step and the empty obstruction payload stay outside the template, as
in Echo's effect-body judge, which charges every node of an effect body one
step. In an intent whose body holds an effect node, each `let` is also charged
its binding type's validations (`validations` of the provider value shape, the
same measure templates use) on top of its expression and node step, because
Echo validates the bound value against its declared type there. A pure
shared-read intent's `let` is not, matching Echo's pure-intent judge. A `let` whose type the provider value shape
cannot express (text, unit, a map or a reference) therefore leaves a
shared-read effect program's work unestablished, and the program is refused
with `InvalidBound` ("shared-read Core work is unestablished") on any target,
with or without a `/v3` template; Echo has no such values and refuses it too.
The same `let` in a pure shared-read intent still compiles.

An `attempt` in such an intent is charged what Echo's shared executor charges
on its costlier path. Success binds the call's result without validating it,
so it adds nothing. A failure arm builds its payload record and encodes it as
the obstruction payload:

```text
attempt = node step (1, 0)
        + value expression
        + max over arms of:
            each payload field's expression
          + (1 + 1 + n steps, 64 + sum of (64 + name length) bytes)
                                   the record's expression step and its shell
          + copying size of the payload record's type
                                   its encoding, Echo's shape(record) scratch
```

An empty payload therefore costs (3 steps, 128 bytes). A pure shared-read
intent's `attempt` charges only its payload fields' expressions, as before. In
a literal-bound loop the effect node step, the `let` validations and the
`attempt` arm are charged per iteration. Under the copying schedule the template is
added on top of the generic charge, so copying-schedule results are unchanged
from the first template release. A type path may step into list items with `item` and then name fields inside
the item with `then`. The path names one item's type, so its measure is a
per-item bound; see the grammar in `rationale.md`. Templates are closed `u64`
expressions over the effect's declared input and output types (`bytesMax`,
`listMax`, the provider value shape `copy` and `validations`), named
parameters, the module's costliest source helper, and
`{ helper: { parameter: name }, of: "steps" / "bytes" }`, one call to the
source helper a `helper` parameter names. Under the shared-read schedule
`helper` prices exactly the named helper; under the copying schedule it charges
the costliest helper, as `costliestHelper` does, once the named helper is known
to exist.

The catalog's top-level `parameters` map declares each parameter's field path
into the application's operation configuration (the `targetConfiguration` the
lawpack pins by digest) and its kind, `uint` or `helper`. Echo's catalog at
echo `44098b15` declares `maxEdgeScans` at `graphReads.maxEdgeScans` (`uint`)
and `selector` at `graphReads.closureSelector` (`helper`). That commit is on
Echo's unmerged draft PR #1029, not on Echo's main; Amblicit vendors the
catalog from it in `fixtures/echo-target-intrinsics-v3/` and must re-vendor it
when #1029 merges. Amblicit reads the configuration only
at the declared paths, and only for parameters a selected template uses. An
undeclared parameter, a missing value, or a value of the wrong kind refuses
the catalog with `InvalidParameter` (`TargetIntrinsicInvalidParameter` in the
application build). Pure-intrinsic admission, which the build runs first on the
same file, accepts the `parameters` key in a `/v3` catalog only; a `/v2`
catalog with it is still `MalformedCatalog`.
A `/v2` catalog publishes no templates and charges effects as before.

`admit_target_effect_costs` admits the templates of the effects a lowering
selects from the same pinned catalog as the pure intrinsics; the application
build does this whenever `target.pureIntrinsics` names a catalog. The
templates reach the compiler with `CompilerContext::with_target_effect_costs`
and native Target with `TargetIrLoweringFacts::with_target_effect_costs`.
Under the copying schedule the source checker re-checks each intent with its
budget reduced by its template work; under the shared-read schedule the shared
meter charges each template where the effect node is charged. A branch is
charged its larger arm and a literal-bound loop body per iteration. A template
whose path does not resolve, or whose `helper` parameter names no source
helper in the module, is refused with `InvalidBound`.

A `helper` parameter is a bare function name of the root (application)
module, the module Echo qualifies a `closureSelector` with. In a multi-module
source-closure build the linker gives every root-module function a fresh
linked name; `LinkedSourceClosure::link_effect_costs` rebinds each configured
helper through the linker's own record of root-module names, and the
application build calls it before compiling. A name that is not a root-module
function (a function of an imported module, a qualified name, or no function)
refuses admission with `InvalidParameter`
(`TargetIntrinsicInvalidParameter`). The application build reports every effect
cost refusal under the same specific `TargetIntrinsic*` category as
pure-intrinsic admission.

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
