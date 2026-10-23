# tree-sitter-edict

This directory contains the first Tree-sitter source artifacts for Edict
developer tooling:

- `grammar.js`: editor-oriented grammar source for the current accepted Edict
  fixture subset.
- `queries/highlights.scm`: highlight captures aligned with
  `amblicit_syntax::highlight_source` roles.
- `test/corpus/current-subset.txt`: corpus examples that stay accepted by
  Edict's reference parser.

The grammar is intentionally a source artifact in this slice. Generated parser
packages and npm publishing metadata remain future work. The first packaged
editor integration is the TextMate-backed VS Code/Cursor package in
`../../editors/vscode/`.

To validate the grammar when the Tree-sitter CLI is installed:

```text
cd grammars/tree-sitter-edict
tree-sitter generate
tree-sitter test
```
