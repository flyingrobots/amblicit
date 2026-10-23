# Amblicit Editor Integrations

This directory contains first-party editor integration artifacts for Amblicit.

## Supported In This Repository

- `vscode/`: a thin VS Code/Cursor extension package that registers `.amblicit`
  files and uses the canonical TextMate grammar for syntax highlighting.

## Grammar Artifacts

- `../grammars/textmate/amblicit.tmLanguage.json` is the canonical TextMate
  grammar source.
- `../grammars/tree-sitter-amblicit/` contains the Tree-sitter grammar source,
  generated parser source, highlight query, and current-subset corpus.

## Current Limits

The VS Code package is source-controlled and locally installable, but this
repository does not publish a Marketplace package yet. Vim, Zed, and jedit
extension packages remain future work; those consumers should use the grammar
artifacts directly until first-party packages land.
