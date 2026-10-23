[
  "package"
  "use"
  "type"
  "enum"
  "variant"
  "intent"
  "returns"
  "profile"
  "implements"
  "basis"
  "footprint"
  "budget"
  "where"
  "let"
  "return"
  "require"
  "guarantee"
  "assert"
  "if"
  "then"
  "else"
  "for"
  "in"
  "bounded"
  "yield"
  "match"
  "shape"
  "lawpack"
  "target"
  "core"
  "as"
  "digest"
  "true"
  "false"
] @keyword

((identifier) @keyword
  (#any-of? @keyword "capability" "fn" "const"))

(comment) @comment
(string) @string
(number) @number
(version) @number

(type_identifier) @type

(intent_declaration
  name: (identifier) @function)

(call_expression
  function: (expression
    (qualified_identifier) @function))

(call_expression
  function: (qualified_identifier) @function)

(field_expression
  field: (_) @property)

[
  "="
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "+"
  "-"
  "*"
  "/"
  "%"
  "!"
  "&&"
  "||"
  "=>"
] @operator

[
  ";"
  ":"
  "::"
  ","
  "."
  "..."
  "@"
  "("
  ")"
  "{"
  "}"
] @punctuation
