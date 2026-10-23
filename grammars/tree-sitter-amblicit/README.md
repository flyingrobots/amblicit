# tree-sitter-amblicit

This directory contains the first Tree-sitter source artifacts for Amblicit
developer tooling:

- `grammar.js`: editor-oriented grammar source for the current accepted Amblicit
  fixture subset.
- `queries/highlights.scm`: highlight captures aligned with
  `amblicit_syntax::highlight_source` roles.
- `test/corpus/current-subset.txt`: corpus examples that stay accepted by
  Amblicit's reference parser.

The grammar is intentionally a source artifact in this slice. Generated parser
packages and npm publishing metadata remain future work. The first packaged
editor integration is the TextMate-backed VS Code/Cursor package in
`../../editors/vscode/`.

The grammar name is `amblicit`, its scope is `source.amblicit`, and it
registers the `amblicit` file type. The checked-in `src/parser.c` uses
Tree-sitter language ABI 15.

To validate the grammar when the Tree-sitter CLI is installed:

```text
cd grammars/tree-sitter-amblicit
tree-sitter generate
tree-sitter test
```
