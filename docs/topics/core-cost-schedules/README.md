# Core cost schedules

This shelf owns how Core evaluation is converted into the steps and allocated
bytes that an intent's `coreEvaluationBudget` bounds
([EDICT-LANG-COST-SCHEDULE-001]).

## Copying schedule

The current schedule is `edict.core-cost/v1`. It charges every evaluated value
at its full type size, in 64-byte cells: reading a local, projecting a field,
indexing a list, passing an argument and carrying fold state each cost as much
as copying the whole value. This mirrors Echo's copying pure evaluator.
Native Target recomputes the charge from Core in `target_ir/fold_work.rs`.

The design for a second, shared-read schedule is in the
[rationale](rationale.md).
