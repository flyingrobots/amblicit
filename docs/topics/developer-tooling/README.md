# Developer Tooling

Status: current HEAD contract.

The developer-tooling alpha now has these editor-facing surfaces:

- `amblicit_syntax::highlight_source`, which classifies source spans into stable
  `HighlightRole` values for editor adapters.
- [Tree-sitter grammar source](../../../grammars/tree-sitter-amblicit/grammar.js),
  generated parser source, a
  [highlight query](../../../grammars/tree-sitter-amblicit/queries/highlights.scm),
  and a
  [current-subset corpus](../../../grammars/tree-sitter-amblicit/test/corpus/current-subset.txt)
  for the accepted fixture families.
- [TextMate grammar](../../../grammars/textmate/amblicit.tmLanguage.json) for
  `.amblicit` lexical scopes in TextMate-compatible editors.
- [VS Code/Cursor extension package](../../../editors/vscode/package.json) for
  `.amblicit` language registration and TextMate-backed syntax highlighting.
- CLI `project` JSONL records for editor-facing projection over dirty source
  text: syntax spans, diagnostics, Core review JSON plus canonical digest, and
  Target IR review JSON plus canonical digest.

Highlighting is intentionally lexical. It does not parse, resolve, type-check,
lower to Core, or evaluate admission policy. It keeps comments visible to
editors even though the parser treats comments as trivia and excludes them from
semantic identity.

Authored unsigned-byte literals such as `255u8` are one Number span in the
reference highlighter and TextMate grammar. The Tree-sitter corpus includes a
U8 declaration and suffixed literal. Its generated-parser check uses a separate
`tree-sitter test` invocation; `cargo xtask verify` checks reference-parser and
TextMate agreement but does not run Tree-sitter.

Supported roles are:

- `Comment`
- `Identifier`
- `Keyword`
- `Number`
- `Operator`
- `Punctuation`
- `String`
- `TypeIdentifier`

The Tree-sitter grammar is an editor syntax-tree artifact for the current
accepted source subset. Its corpus covers bounded hello, branch-yield effects,
read obstruction handling, and enum/variant match syntax, and the corpus source
examples must keep parsing through `amblicit_syntax::parse_module`.

The TextMate grammar is a lexical artifact for `.amblicit` files. It exposes scopes
for comments, strings, numbers, keywords, type identifiers, operators,
punctuation, and bare identifiers, aligned with the public highlighter roles.

The VS Code/Cursor package is intentionally thin: it contributes the Amblicit
language id, `.amblicit` extension mapping, comment and bracket metadata, and the
canonical TextMate grammar. It does not ship diagnostics, a formatter, semantic
tokens, a compiler command, or a language server.

The CLI projection records are the current Amblicit-side bridge surface for future
Graft and jedit adapters. The projection review JSON is not canonical
hash input; canonical digest fields still come from the Core and Target IR
canonical encoders.

Generated Tree-sitter packages plus Vim, Zed, Graft, and jedit integration
packages remain future work. This shelf owns the behavior those adapters must
preserve.
