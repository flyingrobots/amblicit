# Compiler Spine Topic

Status: current HEAD contract.

This chapter describes the executable compiler-spine stages that exist today.
The spine is the path from parsed source AST to in-memory Core IR. The lowerer
does not embed canonical bytes or hashes into Core modules, and it is not a hash
freezer, target lowerer, or admission tool.

## Public Surface

The public compiler-spine surface lives in `edict_syntax`:

- `validate_surface` checks context-free source-AST invariants.
- `resolve_module` resolves source names that can be resolved from the module
  plus explicit compiler context facts. [CSPINE-REQ-001]
- `type_check` builds a typed module boundary distinct from source AST.
  [CSPINE-REQ-002]
- `lower_core` lowers the typed initial subset to in-memory Core IR.
  [CSPINE-REQ-003]
- `compile_to_core` runs the full executable path:
  `validate_surface -> resolve_module -> type_check -> lower_core`.
  [CSPINE-REQ-004]

`CompilerContext` is intentionally explicit. Source clauses such as
`profile hello.readOnly` and `budget <= hello.tinyBudget` do not magically
become Core facts; the caller must supply deterministic profile and budget facts
before the resolver can produce Core-ready metadata. [CSPINE-REQ-005]
The caller must also supply deterministic write-class facts for operation
profiles and imported effect calls before the compiler can check profile/effect
compatibility. [CSPINE-REQ-009]
Those first compiler context facts may be supplied with builder methods or by
loading explicit authority-facts files through
`load_compiler_context_from_authority_fact_files`. Canonical
`edict.authority-facts/v1` bytes decode to the same validated document model and
enter the same `compiler_context_from_authority_facts` path. [CSPINE-REQ-010]

## Current Contract

`source_linker::link_source_closure` links an authenticated witness into a
separate immutable compiler-owned surface. `LinkedSourceClosure::compile`
accepts explicit compiler context facts and maps compiler failures back to the
original package and source-local byte span. Original source bytes and closure
identity remain unchanged. [CSPINE-REQ-051]

Source-module aliases expose only the named dependency's own types and pure
functions. They do not implicitly re-export transitive imports. Every unit's
definitions are checked, including unused functions. A library cannot capture
an application's declarations, and library parameters/local bindings cannot
collide with unrelated application names. Imported intent entries refuse as
`ImportedIntent`; the root owns application entry declarations. [CSPINE-REQ-051]

The linker chooses deterministic fresh compiler names for declarations,
external import aliases and lexical bindings, preserving root entry names,
field labels and runtime member access. Source-owned record identities remain
`package@version.Type`, independent of the importing application; root record
table keys retain their original source names. Source declarations cannot use
renaming to shadow reserved types or redeclare imported callable/type authority.
Type-constructor height is limited to 128 independently of statement/expression
frames; active lexical traversal and flattened member paths are also bounded.
Built-in `ExternalActionRequest` types and bare capability request callees
remain supported. Authored namespace calls, including `core.*`, require an
import owned by that same source module. These checks grant no runtime
authority. Public application/provider closure
consumption remains a separate requirement. [CSPINE-REQ-051]

Source `type Alias = Target;` declarations are transparent spellings for
currently lowerable targets. They introduce no new nominal authority or Core
type table entry. Aliases retain bounded representations and the exact owning
contract of an imported nominal type. Forward aliases and record dependencies
resolve before checking function signatures and intent bodies. [CSPINE-REQ-052]

Source type dependencies are acyclic, with at most 128 declarations on a path
and fully expanded structural height at most 128, including inline constructors,
cached alias targets and record fields. A scalar has height zero; a list adds
one and a nonempty record adds one above its greatest field. Height 128 is
accepted and 129 refuses as `InvalidBound`. Staged `resolve_module` then
`type_check` also refuses duplicate source type declarations as
`SurfaceValidation`. Unsupported string canonicalization refuses as
`UnsupportedSourceShape`, even in an unused alias. Source reference traversal and
cumulative source-defined declaration expansion each have a 65,536-unit ceiling.
A scalar or opaque imported reference costs one expansion unit; list/map/option
constructors add one plus their children; a record adds one plus its fields.
References to source-defined types use their complete expansion weight on every
occurrence. This bounds source-defined DAG expansion before shape construction;
it is not a physical allocator metric or a replacement for imported-type and
runtime budgets. Cycles, missing types and exceeded bounds refuse structurally.
[CSPINE-REQ-052]

`source_closure::resolve_source_closure` authenticates an explicitly supplied
root and complete dependency set without filesystem lookup. Module imports pin
the exact `edict.source/v1` digest of each dependency. The witness retains exact
UTF-8 bytes and parsed surfaces, rejects invalid, duplicate, missing,
substituted, disconnected and cyclic source inputs, and bounds unit count,
aggregate source bytes and every import path. Its canonical closure bytes and
identity are independent of supplied dependency order. [CSPINE-REQ-050]

Closure authentication grants no execution authority and does not link or
type-check the units. Public application builds still accept one source;
`compile_to_core` requires source-module imports to be resolved before use.
The [source-closure contract](../../SPEC_edict-language-v1.md#source-compilation-closures)
defines source pins, artifact framing and fixed ceilings.

Source byte literals lower to `CoreValue::Bytes` with an exact decoded-length
type. Equivalent escape spellings produce identical semantic Core digests;
declared result byte bounds remain enforced. [CSPINE-REQ-049]

Source-owned pure functions compile with signatures collected before bodies,
isolated lexical frames, ordered immutable bindings and one terminal return.
Calls stay explicit and retain ordered arguments, including arguments unused
by the callee. Every definition is checked, including unused definitions.
The [source-function reference](source-functions.md) explains authority,
canonical identity, conservative budgeting and target restrictions.
[CSPINE-REQ-044] [CSPINE-REQ-045]

Boolean `&&` and `||` predicates lower to ordered Core `All` and `Any` forms
where predicate checking is used: input constraints, statement conditionals,
requirements, branch-yield conditions, and pure conditionals. This preserves
predicate structure at source-to-Core; it does not claim target execution
support. Only conjunction supplies input-domain proof evidence.
[CSPINE-REQ-039]

The prelude call `len(value)` lowers for bounded structural `Bytes`, including
exact and zero-length bounds and authenticated imported byte aliases. It
returns U64 and retains the operand's exact coordinate in
`core.bytes.length<OperandType>(value)`. It takes one value and no authored
type arguments. Wrong operand families, malformed calls, and incompatible
result widths reject with structured compiler errors. String scalar length,
list length, and implicit nominal unwrapping are not implemented by this slice.
The target validates the emitted primitive independently; compilation alone
does not establish runtime support or execution evidence.
[CSPINE-REQ-040]

`isUtf8(bytes)` takes one bounded structural byte operand and no authored type
arguments, retaining its exact coordinate in `core.bytes.isUtf8<OperandType>`
and returning Bool. It is a reserved prelude name, including through authenticated
source-module linking. Wrong operand families, signatures and result types reject.
The source-function cost bound includes operand validation and conservative scan
work using the complete bounded operand storage. Direct intent calls also
contribute their declared byte maximum to the static step bound, including
metadata predicates; sequential calls add and lazy branches use a maximum.
A helper scan is counted in its function summary and is not charged again at
the call site. This compiler evidence does not
execute UTF-8 validation. [CSPINE-REQ-053]

`slice(bytes, start, end)` lowers bounded raw bytes and U64 endpoints to
`core.bytes.slice<OperandType>(bytes,start,end)`. Its result keeps the operand
maximum and drops positive minimum/exact length, since empty slices are valid.
Operand type failures point to the wrong byte or endpoint expression; two wrong
endpoints receive separate `TypeMismatch` diagnostics. [CSPINE-REQ-042]
The compiler requires exact evidence of `start <= end <= len(bytes)` from
conjunctive input constraints or unconditional equal/zero relationships. It
does not infer aliases, transitive orders, or branch-local facts. Within an
ordered `&&` predicate, checked left predicates may establish the domain of
later predicates. Separate input constraints remain a canonical set and cannot
justify one another or the earlier basis. Conjunction proof facts do not escape
into another expression. Missing
proof rejects with `UnsupportedSourceShape`; wrong operands or output bounds
reject with `TypeMismatch`. Raw byte slicing has no UTF-8 semantics.
[CSPINE-REQ-041]

Bounded structural byte operands also support `left + right`, preserving order
as `core.bytes.concat<LeftType,RightType>(left,right)`. The result is conservatively
`Bytes<max=L+R>`, where L and R are the operand maxima. Exact or positive minimum
bounds do not propagate to the result. A sum beyond U64 rejects with
`InvalidBound`; incompatible operands or a narrower result destination reject
with `TypeMismatch`. Nominal types are not implicitly unwrapped, and this does
not add integer addition. [CSPINE-REQ-043]
The [canonical lowering decision](../target-ir/README.md#byte-concatenation-boundary)
records its refinement and contract relationships.

This compiler boundary carries the declared budget and finite result maximum.
It does not claim whole-program primitive allocation inference or runtime byte
copy charging. Public `project` can expose the source-produced artifacts;
application `build` also crosses a provider packaging and verification boundary.
A provider can preserve the Core/Target call in an executable-package artifact
without its consumer evaluator implementing that call. Successful packaging or
an accepted verifier report therefore does not establish slice execution,
input-constraint enforcement, or runtime byte-copy charging.

- The lowerable subset is deliberately narrow: local record type declarations and transparent aliases,
  first-order nongeneric source functions, one-parameter intents, `profile`, `basis none` or one input-derived explicit
  basis, `budget <=`, `where` predicates, pure `let` bindings, one annotated
  effectful `let ... else` shape, lowerable `require ... else` obstruction
  arms, `return`, bounded strings and bytes, booleans, fixed-width integers,
  field access, record literals, equality predicates and boolean `&&`/`||`
  connectives, bounded string and byte concatenation, and
  pure conditional expressions whose branches have compatible bounded types,
  and branch-yield lets whose isolated blocks use already-supported
  statements and produce compatible bounded values. Statement conditionals
  lower to isolated branch blocks, and literal- or
  coordinate-bounded loops lower over bounded lists when the resolved cap
  covers the list maximum and cumulative sequential/nested loop work stays
  within the operation step budget.
  [CSPINE-REQ-006] [CSPINE-REQ-011] [CSPINE-REQ-017] [CSPINE-REQ-023]
  [CSPINE-REQ-025] [CSPINE-REQ-026]
- The fixed-width source scalar set is `I32`, `I64`, `U32`, and `U64`.
  Explicitly suffixed literals retain their exact width and signedness;
  bare literals inherit an unambiguous expected width from supported comparison,
  annotation, and record-return contexts. Unconstrained bare literals, overflow,
  negative unsigned values, and cross-width assignments reject in type checking;
  signed minima are accepted through unary-negative literal folding. Byte
  forms lower while preserving these bounds and identities:

  | Source form | Lowered byte bounds | Preserved Core identity |
  | --- | --- | --- |
  | `Bytes<max=N>` | `max=N`, with no minimum | Structural byte type |
  | `Bytes<exact=N>` | Closed interval `min=N,max=N` | Structural byte type |
  | Digest-bound imported lawpack alias | Bounds supplied by the exported definition | Nominal exported coordinate |

  [CSPINE-REQ-019] [CSPINE-REQ-021] [CSPINE-REQ-033]
- Same-width `U32`/`U64` subtraction lowers to
  `core.integer.subtract<T>(left, right)` when the operands are identical,
  the subtrahend is zero, literal operands are ordered, or a checked input
  constraint directly establishes the required order. Conjunctions supply
  evidence; disjunctions and negations do not. Within `&&`, the checked left
  predicate can prove the right operand's domain. These temporary facts are
  restored afterward; completed input constraints also justify the intent body.
  Evidence is cleared between intents, including failed bodies.
  Signed and mixed-width subtraction remain outside this subset. This is a
  source-to-Core contract, not target execution support. [CSPINE-REQ-038]
- An explicit basis expression is checked in the pure pre-body environment
  containing the intent parameter, before body locals exist. The typed
  expression is preserved in Core; this is authoring evidence, not runtime
  basis resolution or admission. [CSPINE-REQ-020]
- Core lowering produces structured in-memory `CoreModule` values with module
  coordinate, imports, types, source functions, intents, input constraints, budgets, locals,
  ordered nodes, and result expressions. Public `lower_core` runs the shared
  whole-module Core type-integrity judgment before returning, so a caller-built
  `TypedModule` cannot bypass the source checker and publish invalid Core. That
  shared judgment measures depth after named expansion and applies the cached
  expansion height at every emitted occurrence rather than trusting a prior
  shallow use. [CSPINE-REQ-003] [CSPINE-REQ-037]
- A source type declaration must classify under Core's shared reference grammar
  as a named identity. Intrinsics and reserved bare structural constructors
  reject with `ReservedTypeIdentity` at the declaration span. Compiler-produced
  `core.types` contains authored local named definitions and exact authenticated
  imported named definitions only: record fields live in their parent
  definition, and no `Type.field` or equivalent scratch entry enters Core
  identity. [CSPINE-REQ-037]
- Resolver/type-checker failures use stable `CompilerErrorKind` and
  `CompilerStage` values. Tests assert those structured values rather than
  diagnostic prose. [CSPINE-REQ-007]
- Effectful source bodies are checked against the resolved operation profile's
  allowed write classes before Core lowering. A write-class effect under a
  read-only profile rejects with `ProfileEffectMismatch`. [CSPINE-REQ-009]
- The first lowerable effectful body shape is an annotated
  `let name: Type = effect(arg) else { failure(binder) => Obstruction };`
  where `effect` is an untyped plain dotted callee. It lowers to a semantic
  Core effect node with the effect coordinate, input expression, result binding,
  and deterministic obstruction map. [CSPINE-REQ-011] [CSPINE-REQ-014]
  [CSPINE-REQ-015]
- A branch-yield `let` lowers to a Core branch with one optional result binding;
  each selected block retains its own locals, effects, and yielded result.
  The accepted shape is:

  ```edict
  let name = if predicate {
    supported_statement;
    yield value;
  } else {
    yield other_value;
  };
  ```

  Incompatible branch results reject before Core exists. Unsupported effect
  calls and bare effect statements still reject with stable compiler stage and
  kind identities before Core lowering. Bare-integer width inference memoizes
  successful yield-block shapes within one compilation, so nested valid
  branches do not cause exponential repeated checking. Compatible byte ranges
  join by independently taking the minimum lower bound and maximum upper bound;
  for example, `Bytes<min=2,max=4>` and `Bytes<min=3,max=5>` infer
  `Bytes<min=2,max=5>` in either branch order. This least-upper-bound affects
  unannotated branch inference only and does not weaken exact or annotated
  assignment checks.
  [CSPINE-REQ-032] [CSPINE-REQ-035]
  [CSPINE-REQ-012]
- Duplicate failure keys in an obstruction map reject with
  `DuplicateObstructionFailure` before Core lowering. [CSPINE-REQ-013]
- Each obstruction binder for an imported effect receives the exact
  authenticated failure-payload type from the effect signature closure. The
  compiler does not synthesize an `effect.failure` coordinate; a missing or
  unresolved payload root rejects before Core. [CSPINE-REQ-037]
- Lowerable `require ... else <obstruction>` statements lower to Core
  terminal require-failure arms, and
  `require ... else continue obstructed { reason: ... }` lowers to a preserved
  obstruction require-failure arm. Duplicate preserved-obstruction payload
  fields reject with `DuplicateObstructionPayloadField` before Core digesting.
  [CSPINE-REQ-017] [CSPINE-REQ-018]
- File-backed authority facts can supply the same profile, budget, profile
  write-class, and effect write-class facts consumed by the compiler spine.
  [CSPINE-REQ-010]
- Pure-helper calls resolve only from compiler facts owned by an exact imported
  lawpack. The preparation path derives those facts, including primitive or
  bounded exported signature types, from the validated export closure, while
  Core records the canonical helper coordinate. Missing helpers, substituted
  import digests, mismatched source aliases or export suffixes, incompatible
  arguments, unowned or digest-substituted cost facts, and missing cost templates
  reject before Core. Conservative helper steps, allocation, and output costs
  add across sequential calls, take the component-wise maximum across exclusive
  branches, and multiply through enclosing bounded loops. Helper steps and
  structural loop work combine on each control-flow path before exclusive
  branch maxima are selected, preserving branch correlation under the shared
  operation step budget. Imported type-alias traversal rejects beyond a
  deterministic depth of 128 rather than risking unbounded recursion. Every
  resolved named helper parameter, return type, or reachable named child enters
  the emitted Core type closure even when no application declaration names it.
  Inline records and other structural constructors use Core's shared canonical
  renderer and are traversed without being interned as names. Synthesized record
  literals and conditional joins therefore expose deterministic, branch-order-
  independent structural references; no `anonymous.record` scratch coordinate
  crosses into Core. The imported lawpack digest remains the helper
  implementation, cost, and named-closure identity.
  [CSPINE-REQ-024]
  [CSPINE-REQ-029] [CSPINE-REQ-031] [CSPINE-REQ-034] [CSPINE-REQ-036]
- Coordinate loop bounds resolve only from explicit compiler facts. Exact
  lawpack preparation projects exported `U32` and `U64` constants through the
  source alias, uses their numeric values for static soundness and budget
  checks, and preserves their canonical exported coordinates in Core.
  [CSPINE-REQ-027]
- The lowerer output carries no embedded canonical bytes, exact digest, target
  IR, or admission fields. Canonical encoding is a separate Core IR surface, and
  reviewed golden bytes and exact digests are separate Core IR artifacts.
  [CSPINE-REQ-008]

The authored mutation witnesses compare actual public lawpack authoring and
compilation results. A changed helper body changes the exact import and Core
identity; source-only conditional, loop-bound, and loop-body changes also move
Core identity. An old source pin rejects the changed helper closure. These
witnesses establish compiler behavior: the current Target lowerer still rejects
a Core `for` node with `UnsupportedCoreNode` and emits no Target artifact.
[CSPINE-REQ-023] [CSPINE-REQ-024] [CSPINE-REQ-025]

## Deferred

The following are not implemented by this compiler-spine slice:

- target-profile lowering;
- obstruction exhaustiveness against target/lawpack failure facts;
- effect obstruction payload lowering;
- bare effect-statement lowering;
- shape/lawpack schema loading;
- full lawpack or target-profile manifest loading beyond authority-facts
  documents;
- full source language lowering.

Those items remain assigned to later lowerability/admission milestones.

The verification matrix is tracked in [test-plan.md](./test-plan.md).

## Failed binding recovery

The compiler keeps lexical failure markers separate from typed local values. A failed let binding removes any value for that name in its scope and marks dependent uses as unavailable. Cloned branch environments retain those markers without leaking them back to the outer scope. No failure marker has a Core value, local reference, or type. Source-level shadowing remains rejected by surface validation; the explicit public type-check phase also recovers safely on an invalid shadowed AST.

Records continue checking independent siblings after a failed value. Statement branches are checked even when their condition fails, and annotated yield branches and concatenation operands are checked independently. Invalid composites produce no typed value. A syntactically present failed return differs from an absent return, preventing a redundant missing-return error. A truly absent return retains its error.

Obstruction payload shorthand suppresses dependent reads of failed locals while genuinely unknown names still fail. Logical and comparison predicate operands, require failure arms and source-function statements retain their independently checkable causes after earlier failures. A record containing an unavailable field still reports an incompatible non-record annotation. These checks collect diagnostics without constructing missing Core parts.

Both real contextual-yield branches and pure-conditional arms are checked despite earlier failures. Scalar and record yields retain peer literal-width context in either branch order despite unrelated statement errors. Shape probes remain observational and never publish diagnostics or replace authoritative branch checks. Authored record and payload keys retain their identity independently of successfully typed values; missing or extra closed-record fields and duplicate payload fields remain diagnosable. Unsigned subtraction keeps independently checkable operand errors without inventing an underflow proof.

Invalid annotations retain independently checkable initializer errors in intent, source-function and yield bindings. A scoped unavailable-context check suppresses only unsuffixed integer-width errors that need that missing annotation; explicit suffix range failures and later independent statements retain their errors. Imported arguments with unavailable parameter types retain independent expression errors without acquiring a substitute expected type. Later source/imported helper arguments are checked after a failed peer, and failed external-action requests poison their binders. Request annotations, operations, authority, basis, budgets and resource references are checked independently before a request node is constructed. No missing annotation supplies a substitute type or authorizes a binding. At the public type-check boundary, poisoned callee roots cannot fall through to source or imported helper resolution; their argument expressions still retain independent failures.

Byte slices check all operands before constructing a value or proving bounds. Failed loop iterators retain independent body errors with their binder unavailable; when the iterator type is known but its bound fails, body diagnostics use the real item type without constructing a loop. Failed effect inputs and invalid receipt annotations still permit independent obstruction-map checks using existing effect facts and discarded diagnostic locals. A rejected effect profile also preserves independent binding diagnostics; its arguments and receipt annotation are checked, and no effect local or Core node is constructed for the rejected profile.

Intrinsic concatenation and byte-input requirements remain active even when an outer annotation is unavailable. The UTF-8 predicate uses the same intrinsic byte requirement as length and slice, so an invalid outer annotation cannot suppress a non-byte operand error. Missing outer context does not make integer operands admissible to concatenation, slice or length. Direct numeric predicates retain their existing ExpectedPredicate refusal. Comparison operands retain their intrinsic width requirements through unavailable outer annotations, including conditional and record contexts.

The complete recovery requirement remains planned. Implemented cases establish only their named evidence. The test plan records the complete requirement as planned and names the implemented cases; passing those cases does not establish the remaining paths.

[CSPINE-REQ-054]

## Effect signature mismatch detail

Explicit mapped effect bindings report `TypeMismatch` at the exact call span when their argument or receipt annotation differs from the exact exported signature. `CompilerError.signature_mismatch` identifies the source effect, input or receipt boundary, typed structural path, and expected/actual self-describing bounded types. Record fields use lexical depth-first first-difference order. List-length incompatibility identifies the list itself; a compatible length with an incompatible item descends through a `listItem` segment. Nominal types retain their distinct coordinates. A missing field has no actual type; an extra field has no expected type. Compatibility rules are unchanged, and independent later errors remain visible.

This is an implementation-crate Rust API addition: code constructing `CompilerError` with a struct literal must initialize `signature_mismatch`, usually to `None`. The curated facade adds no named re-export of the new context types. Bare-call classification and dependent-error suppression remain separate work.

[CSPINE-REQ-048]

Source and imported helper calls retain independent supplied-argument diagnostics
after an argument-count mismatch. Known parameter positions retain their type
context; extra arguments are checked without inventing an expected type. Invalid
calls produce no Core value. [CSPINE-TP-124]

When helper resolution fails, supplied arguments still retain independent
diagnostics without an expected parameter type. Poisoned dependencies remain
suppressed and the compiler constructs no call. [CSPINE-TP-125]

Supported branch-yield bindings check effect profiles while checking each
branch, rather than rejecting the whole binding during a recursive preflight.
A disallowed mapped effect leaves independent sibling diagnostics visible;
predicate and yielded-value profile checks still reject disallowed effects.
[CSPINE-TP-126]

Duplicate failure-map keys reject the complete map while retaining independent
arm diagnostics. Every source arm is checked, including a duplicate that would
otherwise be overwritten by key indexing. These diagnostic checks allocate no
obstruction locals and publish no recovered map; valid maps retain their
existing canonical ordering and Core output.

String concatenation uses checked maximum-bound addition and reports
`InvalidBound` if the sum exceeds U64. This also applies when recovery visits
an independent concatenation after another operand failed; no wrapped bound or
recovered value is constructed. A sum equal to U64's maximum remains representable.

Duplicate obstruction payload fields retain independent value/name diagnostics
in both terminal and continuing failure arms. A duplicate explicit value or
shorthand is still checked, while poisoned-name reads remain suppressed. Any
duplicate keeps the complete payload map invalid; checked values do not make
that map eligible for Core publication.
