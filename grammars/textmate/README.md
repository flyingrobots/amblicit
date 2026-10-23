# Edict TextMate Grammar

`edict.tmLanguage.json` is the TextMate grammar artifact for `.edict` source
files. It assigns lexical scopes for the same editor-facing token families as
`amblicit_syntax::highlight_source`: comments, strings, numbers, keywords, type
identifiers, operators, punctuation, and bare identifiers.

This is a grammar artifact, not an editor extension package. VS Code and other
TextMate-compatible editors can consume it through their normal grammar
registration mechanisms, but packaged VS Code, Vim, Zed, and jedit integrations
remain separate release work.
