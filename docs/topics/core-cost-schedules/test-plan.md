# Core Cost Schedules Test Plan

## Scope

This shelf owns the selectable Core work schedules that convert evaluation into
steps and allocated bytes: the default copying schedule `edict.core-cost/v1`
and the shared-read schedule `edict.core-cost/v2`. Runtime metering belongs to
each consumer.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| COST-REQ-001 | planned | Without an explicit selection, compilation and Target accounting are unchanged, including the bounded-descent fixture's minimum budgets. | EDICT-LANG-COST-SCHEDULE-001 |
| COST-REQ-002 | planned | Selecting the shared-read schedule records `core.cost.shared-reads/v1` in Core, and native Target applies the schedule only when that capability is present. | EDICT-LANG-COST-SCHEDULE-001 |
| COST-REQ-003 | planned | Under the shared-read schedule, reads, arguments, results and fold state cost one step; construction keeps its v1 charge; source and native Target minimum budgets are equal. | EDICT-LANG-COST-SCHEDULE-001 |
| COST-REQ-004 | planned | The bounded-descent fixture's minimum step and byte budgets fall by at least two orders of magnitude under the shared-read schedule. | EDICT-LANG-COST-SCHEDULE-001 |
| COST-REQ-005 | planned | No program accepted under a shared-read budget does more metered work than the budget on a reference evaluator that implements the schedule. | EDICT-LANG-COST-SCHEDULE-001 |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| COST-TP-001 | planned | Regression | COST-REQ-001 | The default schedule keeps the fixture's minimum budgets at 647,943,859 steps and 1,805,252,203 bytes. | - | fixtures/lang/bounded-descent/avl-rope.amblicit | Values measured on main at 1a0178cb. |
| COST-TP-002 | planned | Capability | COST-REQ-002 | The shared-read schedule records the capability; removing it from Core makes Target apply v1 and refuse the v2 budget. | - | fixtures/lang/bounded-descent/avl-rope.amblicit | Fails closed. |
| COST-TP-003 | planned | Equality | COST-REQ-003 | Source and native Target minimum budgets are equal under the shared-read schedule. | - | fixtures/lang/bounded-descent/avl-rope.amblicit | One meter. |
| COST-TP-004 | planned | Reduction | COST-REQ-004 | Both shared-read minimums are at most 1/100 of the copying minimums. | - | fixtures/lang/bounded-descent/avl-rope.amblicit | |
| COST-TP-005 | planned | Soundness | COST-REQ-005 | Adversarial programs run on worst-case inputs never exceed their minimum accepted budget on the reference evaluator. | - | - | Reference evaluator is test-only. |
