# Amblicit VS Code Extension

This package registers `.amblicit` files for VS Code-compatible editors and maps
them to the Amblicit TextMate grammar. The language id is `amblicit` and the
grammar scope is `source.amblicit`.

## Local Install

From this directory:

```text
npx @vscode/vsce package
code --install-extension amblicit-vscode-0.6.0-alpha.1.vsix
```

Cursor can install the same VSIX package from its extensions view.

The package was previously named `edict-vscode` with the language id `edict`.
Uninstall `flyingrobots.edict-vscode` before you install this package, and
rename `.edict` files to `.amblicit`.

## Scope

The extension only provides language registration and lexical syntax
highlighting. It does not ship a language server, compiler command, formatter,
semantic tokens, diagnostics, or Tree-sitter runtime integration.
