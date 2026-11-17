# Bounded descent and rebuild

This shelf owns how an Edict author walks and rebuilds a depth-bounded tree,
such as Jedit's AVL rope, without recursive types or recursive functions
([EDICT-LANG-DESCENT-001]). It is the v1 answer to
[issue #262](https://github.com/flyingrobots/amblicit/issues/262) under the
owner ruling of 2026-10-21 on
[issue #261](https://github.com/flyingrobots/amblicit/issues/261).

The construct adds no syntax, no Core node and no Target node. A tree is a
bounded list of node records, a walk is a first-order bounded `fold` over an
explicit path stack, and the nodes an edit creates come back as a bounded
list. Every piece already has its own contract: `fold`
([compiler spine](../compiler-spine/README.md), `EDICT-LANG-FOLD-001` in the
language spec), bounded lists (`EDICT-LANG-LIST-OPS-001`), unsigned arithmetic
proofs and typed function failures. This shelf states how they compose, what the
compiler checks statically, what the composition costs and what is refused.

## Representation

A depth-bounded tree is an *arena*: a `List<Node, max=A>` whose node records
name their children by list index. An index that is not below `listLen(arena)`
is the empty tree, so a malformed reference reads as empty instead of failing.

```amblicit
type Node = { length: U64, height: U64, left: U64, right: U64, leaf: Bool };
type Arena = List<Node, max=64>;
```

The arena is an ordinary immutable value. It is not a stored collection, a
place or a heap. Path copying appends: a fresh branch is `listAppend(arena,
node)`, and its index is the arena length before the append. Unchanged
subtrees keep their indexes, which is the sharing a persistent rope needs.

## Descent

Descent is a `fold` whose state holds the arena, the current index and a path
stack `List<Frame, max=H>`, where `H` is the maximum tree height. Each step
reads the current node, pushes a frame and moves to a child, or marks the walk
complete at a leaf. A push needs the proof `listLen(path) < H` in a dominating
condition, exactly as any `listAppend` does.

```amblicit
fn descendTo(s: Split, frame: Frame, next: U64, offset: U64) -> Split {
  return if listLen(s.path) < 8u64
    then { arena: s.arena, path: listAppend(s.path, frame), remaining: s.remaining,
           cur: next, offset, rebuilding: false, done: false, left: s.left, right: s.right }
    else s;
}
```

## Rebuild

Rebuild pops the path with a cursor instead of shrinking the list. The state
keeps the path unchanged and a `remaining: U64` count; a pop reads
`listAt(path, remaining - 1u64)` under the guard
`1u64 <= remaining && remaining - 1u64 < listLen(path)`, which proves both the
subtraction and the index. Each pop combines the carried subtree with the
parent's other child, for example through AVL `balance`, and appends the fresh
nodes it creates.

One fold can run both phases: a `rebuilding` flag selects the phase, so the
bound is `2H + 2` steps for a descent plus a rebuild. Join and split in the
reference fixture each use one 18-step fold for `H = 8`, and split's rebuild
calls join, which nests one fold inside another's step.

## Completion is state

`fold` has no early exit and always runs its full bound. A walk that finishes
early keeps returning its terminal state. A walk that runs out of steps
before finishing returns a state whose `done` flag is false. The author maps
that state to a declared typed failure, never to a result:

```amblicit
failure DescentIncomplete;

fn split(arena: Arena, root: U64, offset: U64) -> Pair fails DescentIncomplete {
  let initial: Split = { arena, path: [], remaining: 0u64, cur: root, offset,
    rebuilding: false, done: false, left: absent(), right: absent() };
  let final = fold(initial, splitStep, 18u64);
  return if final.done then { arena: final.arena, left: final.left, right: final.right }
    else fail DescentIncomplete;
}
```

The intent then maps the failure to a domain obstruction with `else`, as for
any other typed function failure.

## Fresh nodes

The nodes an edit created are the arena suffix after the input length. A
collecting `fold` copies them into a `List<Node, max=F>`, which an intent can
return or iterate with `for ... bounded F`. Emitting an effect per fresh node
inside such a loop is outside this shelf: it currently refuses at Target with
`UnsupportedCoreNode` and belongs to issue #267 and Echo #947.

## Static checks

Nothing here is a new check. The compiler and native Target already enforce:

| Fact | Where it is proved | Refusal |
| --- | --- | --- |
| Every fold bound is a `U64` literal | `fold` | `InvalidBound` |
| A step preserves its single state type, so the arena and stack maxima cannot grow | `fold` | `TypeMismatch` |
| A push stays within the stack and arena maxima | `listAppend` proof | `UnsupportedSourceShape` |
| A pop and its index are in range | subtraction and `listAt` proofs | `UnsupportedSourceShape` |
| Lengths and heights cannot overflow | unsigned addition proofs | `UnsupportedSourceShape` |
| No helper reaches itself, directly, mutually or as its own fold step | source reference graph | `UnsupportedSourceShape` (`ReferenceCycle`) |
| No source type contains itself | source type graph | `InvalidCoreTypeIntegrity`, reported once |

The compiler does not infer transitive bounds. `listLen(xs) < 63u64` does not
prove `listLen(xs) < 64u64`; the guard must name the declared maximum.

## Core and Target

The composition lowers to Core `fold` expressions under `core.fold/v1` and
typed `core.list.*` calls under `core.list/v1`, plus the existing integer,
option and failure forms. Native Target re-derives fold typing, list proofs
and work from Core without trusting compiler estimates. The reference fixture
compiles and lowers natively with no other capability.

## Work accounting

`fold` charges its step's transitive work once per iteration, for all `N`
iterations, and nested folds multiply. The Core cost model charges every value
read and every list operation for the full size of its operands, in 64-byte
cells. An arena in fold state is therefore charged on every step and on every
`listAt` or helper call that receives it. Total work grows roughly with
`H² × A`, times the node reads per step.

Measured on the reference fixture (`A = 64`, `H = 8`, join and split bounds
18): the smallest Core step budget the compiler accepts for `splitRope` is
647,943,859 steps, and the smallest allocation budget is 1,805,252,203 bytes.
These are conservative static ceilings, not runtime measurements. A rope at
Jedit's envelope multiplies them. Whether that fits `text.replaceRangeBudget`
is a cost-model question for the Core work contract and Echo's metering; this
shelf does not change either.

## Refused

- Recursive types, including the draft's `variant { ... } depth <= N` clause,
  which does not parse.
- Recursive functions, including `decreases` termination measures, which do
  not parse.
- Dynamic or named fold bounds; lawpack coordinates such as
  `text.maxTreeHeight` cannot bound a fold or a list in the current compiler.
- Early exit from a fold.

## Evidence

The executable reference is
[`fixtures/lang/bounded-descent/avl-rope.amblicit`](../../../fixtures/lang/bounded-descent/avl-rope.amblicit),
an iterative AVL split, join and balance with fresh-node collection. The
[test plan](test-plan.md) lists every case. The fixture is compiler evidence
only: no provider or runtime has executed it.
