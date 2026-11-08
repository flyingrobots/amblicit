# Syntax Topic

Status: current HEAD contract.

This chapter describes what the Phase 1 Edict syntax front end does today. It is
not a proposal document. Future syntax and semantic behavior live in
[test-plan.md](./test-plan.md) as planned or open gaps until the code lands.

## Public Surface

The parser crate is `amblicit-syntax`. Its public boundary is:

```rust
use amblicit_syntax::{parse_module, ParseErrorKind};

let module = parse_module(
    "package examples.hello@1;\n\
     type HelloInput = { name: String<max=256>, };\n",
)
.expect("valid module parses");

assert_eq!(module.package.version, "1");
assert_eq!(module.decls.len(), 1);

let err = parse_module("package a.b@1;\nuse lawpack x.y@1 as return;")
    .expect_err("reserved import aliases reject");
assert_eq!(err.kind, ParseErrorKind::ReservedKeyword);
```

`parse_module` returns a source AST. It preserves source order and surface
spelling needed by later lowering. It does not resolve imports, type-check
programs, prove bounds, or itself lower to Core IR. [SYNTAX-REQ-001]

## Current Contract

- Byte literals such as `b"\r\0\xFF"` preserve raw octets. Both string and byte
  literals accept `\"`, `\\`, `\n`, `\t`, `\r`, `\0` and `\xHH`; the latter
  consumes exactly two ASCII hexadecimal digits. Byte source is ASCII only,
  while string source may contain Unicode and its decoded bytes must be valid
  UTF-8. Malformed escapes and `\u{...}` are rejected. [SYNTAX-REQ-016]

- A module starts with one `package` declaration, then zero or more imports, then
  declarations. Package and import versions preserve source-significant version
  spelling, including `_beta` style labels. [SYNTAX-REQ-002]
- Supported imports are `shape`, `lawpack`, `target`, `core`, `capability`, and
  `module`. Source-module imports require a digest clause and preserve the exact
  package, pin, alias and source span. Standalone compilation rejects unresolved
  source modules, including unused imports. [SYNTAX-REQ-017]
  A capability import names a digest-bound external operation family for
  `request`; it is not a callable effect import. Digest clauses accept only
  `sha256:` plus 64 hex characters. [SYNTAX-REQ-003]
- Type declarations parse record types, refined `String`, max-only `Bytes`,
  `Option`, `CapabilityRef`, bounded `List`, bounded `Map`, enum declarations,
  and payload-carrying `variant` types. Empty enums reject. [SYNTAX-REQ-004]
- Integer literal suffixes are preserved where syntax carries them, including
  static bounds. [SYNTAX-REQ-005]
- Pure `fn name(parameters) -> ReturnType { ... }` declarations preserve typed
  parameter order, the return type, body statements, and source spans. Purity,
  terminal returns and recursion are checked after parsing. [SYNTAX-REQ-015]
- Intent declarations parse parameters, return type, clause surface, statement
  blocks, and expression bodies. Clause requiredness is semantic validation, not
  parser validation. [SYNTAX-REQ-006]
- Statements parse `let`, `request`, `return`, `require`, `guarantee`, `assert`,
  effect call statements, `if` / `else if` / `else`, and bounded `for`.
  External-action requests carry exact schemas, scope, basis, budgets, and
  reconciliation law. Effect and request operation positions must be calls.
  [SYNTAX-REQ-007]
- Expressions parse the full Phase 1 precedence chain, calls, type-calls, field
  access, record literals, booleans, digest literals, pure ternary
  `if ... then ... else`, branch-yield conditional effects in `let` right-hand
  sides, variant literals, and `match`. [SYNTAX-REQ-008]
- Keywords are reserved as bare identifiers, binders, shorthand fields, import
  aliases, and coordinate roots. Keywords remain legal after `.` as member
  names. [SYNTAX-REQ-009] [EDICT-LANG-RECORD-SHORTHAND-001]
- Negative coverage asserts stable `ParseErrorKind` values rather than message
  prose or stdout/stderr. [SYNTAX-REQ-010]
- Source nesting is bounded by the public `MAX_SOURCE_NESTING_DEPTH` (256,
  also re-exported as `amblicit::diagnostic::MAX_SOURCE_NESTING_DEPTH`). Past
  it, parsing stops with `NestingTooDeep`, including inside type-call
  arguments. The parser counts both its recursion depth and the height of the
  tree it builds, so loop-built chains are bounded too. For a source nested in
  a `let` statement, the first row below spends two levels outside its own
  nesting (the statement and the `let` expression) and the second row one
  (chains are bounded by height, which only the statement adds; ternaries and
  nested `if` nest without the `let` expression's level):

  | Shape | Deepest accepted | First refused |
  | --- | --- | --- |
  | parentheses, `!`, `-`, nested calls, lists, records, types, `else if` | 254 | 255 |
  | operator chains, field chains, call chains, ternaries, nested `if` | 255 | 256 |

  The counts assume a leaf that adds nothing of its own: in the first row an
  identifier, a literal or a comparison; in the second row an identifier or a
  literal (a field access or a comparison is itself one level of height). A
  leaf that nests, such as `-1` or `f(x)`, spends its own levels. Every row is
  a committed test (SYNTAX-TP-028). A comparison is never refused because the
  parser first tries to read its `<` as the start of type-call arguments: that
  speculative reading reports a nesting refusal only when the tokens really
  form type-call arguments. [SYNTAX-REQ-018]
- The limit sits above the compiler's source-function depth bound (128 nested
  levels, refused as `InvalidBound` during type checking), so a source
  function between the two depths is still refused by that more specific
  diagnostic under `project` and `build`. `check` stops after parsing and
  surface validation, so it accepts such a function: a source can pass
  `check` and still be refused by `project`. (Implementation: the
  nesting-limit rule above the helpers in
  `crates/amblicit-syntax/src/parser.rs`.) [SYNTAX-REQ-018]

The limit bounds recursion, not the thread. #277 measured the parser before
this limit at about 4.3 KiB of stack per level in a release build and several
times that in debug. From that measurement, reaching the limit needs an
estimated 1.1 MiB in release, so a 1 MiB thread is expected to overflow before
the parser reaches the limit in either profile, and a 2 MiB spawned thread in
a debug build. The `amblicit` binary runs each request on a 64 MiB thread, and
this crate's tests parse to the limit on a 64 MiB thread; a library caller
parsing untrusted sources should give the parsing thread a comparable stack
(tracked in issue #301).

## Deferred

These are deliberately not part of the syntax parser contract:

- semantic checks beyond parsing. The landed source/surface validation stage is
  documented in [semantic-validation](../semantic-validation/); import
  resolution, resolved type checking, and initial in-memory Core lowering are
  documented in [compiler-spine](../compiler-spine/). Contextual bound proofs
  and target/lawpack-dependent checks remain deferred;
- canonical encoding and golden Core fixtures. The landed Core semantic schema
  is documented in [core-ir](../core-ir/);
- `const` declarations;
- `record` semantic-effect statements;
- list, map, and unit expression literals;
- exhaustive source fixture coverage for every `docs/REQUIREMENTS.md` language
  row.

The current verification strategy, planned cases, fixtures, oracles, and open
gaps are tracked in [test-plan.md](./test-plan.md).
