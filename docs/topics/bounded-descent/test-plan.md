# Bounded Descent Test Plan

## Scope

This shelf owns the composition of bounded `fold`, bounded lists, unsigned
arithmetic proofs and typed failures into iterative descent and rebuild over a
depth-bounded arena. The constituent contracts keep their own shelves. Provider
and runtime execution belong to Echo.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| BDESC-REQ-001 | planned | An arena tree with an explicit path stack supports iterative AVL split, join and balance, with fresh-node collection into a bounded list, using only first-order `fold` and bounded list operations. | EDICT-LANG-DESCENT-001 |
| BDESC-REQ-002 | planned | The composition compiles to Core requiring only `core.fold/v1` and `core.list/v1` and lowers through native Target without new Core or Target nodes. | EDICT-LANG-DESCENT-001 |
| BDESC-REQ-003 | planned | Work for nested descent and rebuild is admitted by one conservative ceiling that native Target re-derives; a larger fold bound never costs less. | EDICT-LANG-DESCENT-001 |
| BDESC-REQ-004 | planned | Recursive functions, recursive types, depth-bounded type clauses, termination measures, unproved stack pushes, growing state and dynamic bounds are refused with stable kinds. | EDICT-LANG-DESCENT-001 |
| BDESC-REQ-005 | planned | A source type that contains itself is refused with exactly one `InvalidCoreTypeIntegrity` diagnostic and no cascading claim that the declared type is undeclared. | EDICT-LANG-DESCENT-001 |

## Fixtures

| Fixture | Purpose | Oracle |
| --- | --- | --- |
| `fixtures/lang/bounded-descent/avl-rope.amblicit` | Iterative AVL split, join and balance over a 64-node, height-8 arena, with a typed failure for incomplete descent and fresh-node collection. | The textbook AVL join and split algorithms, written as explicit-stack iteration; the compiler and native Target accept it under declared budgets. |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| BDESC-TP-001 | planned | Golden path | BDESC-REQ-001, BDESC-REQ-002 | The reference rope compiles, requires only the fold and list capabilities beyond failures, and lowers through native Target. | - | fixtures/lang/bounded-descent/avl-rope.amblicit | Compiler evidence only. |
| BDESC-TP-002 | planned | Budget boundary | BDESC-REQ-003 | Source admission at its minimum step and byte budgets lowers natively; native Target refuses Core whose budget is one below its own minimum. | - | fixtures/lang/bounded-descent/avl-rope.amblicit | Mirrors the bounded-list work boundary. |
| BDESC-TP-003 | planned | Metamorphic | BDESC-REQ-003 | Raising the split fold bound strictly raises the minimum accepted step budget. | - | fixtures/lang/bounded-descent/avl-rope.amblicit | Relation, not an absolute cost formula. |
| BDESC-TP-004 | planned | Refusal | BDESC-REQ-004 | Direct, mutual and fold-step recursion refuse `UnsupportedSourceShape`; unproved push refuses `UnsupportedSourceShape`; growing state refuses `TypeMismatch`; a dynamic bound refuses `InvalidBound`. | - | - | One case per refused shape. |
| BDESC-TP-005 | planned | Refusal | BDESC-REQ-004 | The draft's `depth <=` type clause and a `decreases` measure fail to parse with `ExpectedToken`. | - | - | Syntax stays unreserved. |
| BDESC-TP-006 | planned | Diagnostics | BDESC-REQ-005 | Direct and record-mediated recursive variants produce exactly one diagnostic, of kind `InvalidCoreTypeIntegrity`. | - | - | RED on main: a second `UnresolvedType` diagnostic follows. |
