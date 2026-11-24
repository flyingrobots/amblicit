# Bounded descent rationale

The owner ruled on 2026-10-21 that Edict v1 has no recursive types and no
recursive functions; Jim's rope is written iteratively over shallow bounded
structures. Issue #262 asked for the smallest construct that lets the rope's
split, join and balance be written under that ruling.

## Decision

Add no syntax and no Core or Target node. Specify and prove the composition of
existing constructs instead: an index-linked arena, an explicit path stack and
first-order `fold`. Before deciding, an iterative AVL split, join and balance
was written against the merged compiler. It compiled and lowered through
native Target unchanged, so a new construct would add contract surface without
enabling anything the rope cannot already express.

## Alternatives rejected

| Alternative | Why rejected |
| --- | --- |
| Recursive functions with a `decreases` measure, or recursive variants with `depth <= N` | Forbidden by the owner ruling. They would also need a termination checker and depth-indexed type identity in Core, Target and every provider. |
| A dedicated `descend(initial, step, N)` that returns `Option<S>` and treats running out of steps as `none` | Makes incompleteness impossible to ignore, but adds a Core node that Echo's generic runtime does not execute. Fold already executes there. Typed failures give the same guarantee at the call site. |
| Compiler unrolling of bounded recursion | It is recursion in source, so it is forbidden by the ruling, and it multiplies Core size by the depth bound. |
| New stack primitives (`listLast`, `listPop`) | Popping with a cursor over an unchanged list needs only existing `listAt` and subtraction proofs. A new `core.list` call would need provider support. |
| Allowing lawpack coordinates as fold bounds | Useful for tying the bound to `text.maxTreeHeight`, but Edict has no named constants yet. That is a separate language feature; literal bounds are sound because exhaustion is a typed failure. |
| Cheaper accounting for arena reads | The measured ceilings are large. Charging an index read by element size instead of list size changes the Core work contract that native Target and Echo's metering share. It needs its own decision. |
