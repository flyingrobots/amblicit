const PREC = {
  or: 1,
  and: 2,
  equality: 3,
  relational: 4,
  additive: 5,
  multiplicative: 6,
  unary: 7,
  call: 8,
  member: 9,
};

const commaSep = rule => optional(commaSep1(rule));
const commaSep1 = rule => seq(rule, repeat(seq(',', rule)), optional(','));

module.exports = grammar({
  name: 'amblicit',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$.binary_expression, $.unary_expression, $.call_expression],
    [$.binary_expression, $.call_expression],
    [$.type_reference, $.expression],
  ],

  rules: {
    source_file: $ => seq(
      $.package_declaration,
      repeat($.use_declaration),
      repeat($._declaration),
    ),

    package_declaration: $ => seq(
      'package',
      field('name', $.package_reference),
      ';',
    ),

    package_reference: $ => seq(
      field('path', $.qualified_identifier),
      '@',
      field('version', $.version),
    ),

    use_declaration: $ => seq(
      'use',
      field('kind', $.import_kind),
      choice(
        field('shape_path', $.string),
        field('package', $.package_reference),
      ),
      optional(seq('digest', field('digest', $.digest_literal))),
      'as',
      field('alias', $.identifier),
      ';',
    ),

    import_kind: $ => choice('shape', 'lawpack', 'target', 'core'),

    _declaration: $ => choice(
      $.type_declaration,
      $.enum_declaration,
      $.intent_declaration,
    ),

    enum_declaration: $ => seq(
      'enum',
      field('name', $.type_identifier),
      '{',
      commaSep1(field('case', $.type_identifier)),
      '}',
    ),

    type_declaration: $ => seq(
      'type',
      field('name', $.type_identifier),
      optional($.type_parameters),
      '=',
      field('body', choice($.record_type, $.variant_type, $.type_reference)),
      ';',
    ),

    type_parameters: $ => seq(
      '<',
      commaSep1($.identifier),
      '>',
    ),

    record_type: $ => seq(
      '{',
      commaSep($.field_declaration),
      '}',
    ),

    field_declaration: $ => seq(
      field('name', $._name),
      ':',
      field('type', $.type_reference),
      repeat($.field_constraint),
    ),

    field_constraint: $ => seq(
      choice('max', 'min', 'pattern', 'canonical'),
      '=',
      choice($.number, $.string, $.qualified_identifier),
    ),

    variant_type: $ => seq(
      'variant',
      '{',
      commaSep1($.variant_case),
      '}',
    ),

    variant_case: $ => seq(
      field('name', $.type_identifier),
      optional(seq('(', field('payload', $.type_reference), ')')),
    ),

    type_reference: $ => seq(
      $.qualified_identifier,
      optional($.type_arguments),
    ),

    type_arguments: $ => seq(
      '<',
      commaSep1(choice($.type_reference, $.type_constraint_argument)),
      '>',
    ),

    type_constraint_argument: $ => seq(
      choice('max', 'min', 'pattern', 'canonical'),
      '=',
      choice($.number, $.string, $.qualified_identifier),
    ),

    intent_declaration: $ => seq(
      'intent',
      field('name', $.identifier),
      $.parameter_list,
      'returns',
      field('returns', $.type_reference),
      repeat($.intent_clause),
      field('body', $.block),
    ),

    parameter_list: $ => seq(
      '(',
      commaSep($.parameter),
      ')',
    ),

    parameter: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $.type_reference),
    ),

    intent_clause: $ => choice(
      seq('profile', $.qualified_identifier),
      seq('implements', $.qualified_identifier),
      seq('basis', choice('none', $.expression)),
      seq('footprint', '<=', $.qualified_identifier),
      seq('budget', '<=', $.qualified_identifier),
      seq('where', $.expression, repeat(seq(',', $.expression))),
    ),

    block: $ => seq(
      '{',
      repeat($._statement),
      '}',
    ),

    _statement: $ => choice(
      $.let_statement,
      $.return_statement,
      $.require_statement,
      $.guarantee_statement,
      $.assert_statement,
      $.if_statement,
      $.for_statement,
      $.expression_statement,
    ),

    let_statement: $ => seq(
      'let',
      field('name', $.identifier),
      optional(seq(':', field('type', $.type_reference))),
      '=',
      field('value', choice($.if_yield_expression, $.expression)),
      optional(seq('else', $.obstruction_handler)),
      ';',
    ),

    return_statement: $ => seq('return', $.expression, ';'),

    require_statement: $ => seq(
      'require',
      $.expression,
      'else',
      $.obstruction_target,
      ';',
    ),

    guarantee_statement: $ => seq(
      'guarantee',
      $.expression,
      optional(seq('else', $.obstruction_target)),
      ';',
    ),

    assert_statement: $ => seq('assert', $.expression, ';'),

    if_statement: $ => seq(
      'if',
      field('condition', $.expression),
      field('then', $.block),
      optional(seq('else', choice($.if_statement, $.block))),
    ),

    for_statement: $ => seq(
      'for',
      field('name', $.identifier),
      'in',
      field('iterable', $.expression),
      'bounded',
      field('bound', choice($.qualified_identifier, $.number)),
      field('body', $.block),
    ),

    if_yield_expression: $ => seq(
      'if',
      field('condition', $.expression),
      field('then', $.yield_block),
      'else',
      field('else', $.yield_block),
    ),

    yield_block: $ => seq(
      '{',
      repeat($._statement),
      'yield',
      $.expression,
      ';',
      '}',
    ),

    obstruction_handler: $ => choice($.obstruction_target, $.obstruction_map),

    obstruction_target: $ => seq(
      $.qualified_identifier,
      optional(seq('(', $.expression, ')')),
    ),

    obstruction_map: $ => seq(
      '{',
      commaSep1($.obstruction_arm),
      '}',
    ),

    obstruction_arm: $ => seq(
      field('failure', $.identifier),
      optional(seq('(', field('binding', $.identifier), ')')),
      '=>',
      field('target', $.obstruction_target),
    ),

    expression_statement: $ => seq(
      $.expression,
      optional(seq('else', $.obstruction_handler)),
      ';',
    ),

    expression: $ => choice(
      $.if_expression,
      $.match_expression,
      $.binary_expression,
      $.unary_expression,
      $.call_expression,
      $.field_expression,
      $.variant_literal,
      $.record_literal,
      $.parenthesized_expression,
      $.digest_expression,
      $.boolean,
      $.number,
      $.string,
      $.qualified_identifier,
    ),

    if_expression: $ => seq(
      'if',
      field('condition', $.expression),
      'then',
      field('then', $.expression),
      'else',
      field('else', $.expression),
    ),

    match_expression: $ => seq(
      'match',
      field('scrutinee', $.expression),
      '{',
      commaSep1($.match_arm),
      '}',
    ),

    match_arm: $ => seq(
      field('case', $.type_identifier),
      optional(seq('(', field('binding', $.identifier), ')')),
      '=>',
      field('body', $.expression),
    ),

    binary_expression: $ => choice(
      ...[
        [PREC.or, '||'],
        [PREC.and, '&&'],
        [PREC.equality, '=='],
        [PREC.equality, '!='],
        [PREC.relational, '<'],
        [PREC.relational, '<='],
        [PREC.relational, '>'],
        [PREC.relational, '>='],
        [PREC.additive, '+'],
        [PREC.additive, '-'],
        [PREC.multiplicative, '*'],
        [PREC.multiplicative, '/'],
        [PREC.multiplicative, '%'],
      ].map(([precedence, operator]) => prec.left(precedence, seq(
        field('left', $.expression),
        field('operator', operator),
        field('right', $.expression),
      ))),
    ),

    unary_expression: $ => prec(PREC.unary, seq(
      field('operator', choice('!', '-')),
      field('operand', $.expression),
    )),

    call_expression: $ => prec(PREC.call, choice(
      seq(
        field('function', $.qualified_identifier),
        field('type_arguments', $.call_type_arguments),
        field('arguments', $.call_argument_list),
      ),
      seq(
        field('function', $.expression),
        field('type_arguments', $.call_type_arguments),
        field('arguments', $.call_argument_list),
      ),
      seq(
        field('function', $.expression),
        field('arguments', $.argument_list),
      ),
    )),

    call_type_arguments: $ => prec(PREC.call, seq(
      '<',
      commaSep1(choice($.type_reference, $.type_constraint_argument)),
      '>',
    )),

    call_argument_list: $ => seq(
      token.immediate('('),
      commaSep($.expression),
      ')',
    ),

    field_expression: $ => prec.left(PREC.member, seq(
      field('object', $.expression),
      '.',
      field('field', $._name),
    )),

    variant_literal: $ => prec.right(PREC.member, seq(
      field('type', $.qualified_identifier),
      '::',
      field('case', $.type_identifier),
      optional(field('arguments', $.argument_list)),
    )),

    parenthesized_expression: $ => seq('(', $.expression, ')'),

    digest_expression: $ => seq('digest', '(', $.digest_literal, ')'),

    record_literal: $ => seq(
      '{',
      commaSep(choice($.spread_entry, $.field_entry, $.shorthand_entry)),
      '}',
    ),

    spread_entry: $ => seq('...', $.expression),

    field_entry: $ => seq(
      field('name', $._name),
      ':',
      field('value', $.expression),
    ),

    shorthand_entry: $ => field('name', $.identifier),

    argument_list: $ => seq(
      '(',
      commaSep($.expression),
      ')',
    ),

    boolean: $ => choice('true', 'false'),

    digest_literal: $ => $.string,

    qualified_identifier: $ => prec.right(seq(
      $._name,
      repeat(seq('.', $._name)),
    )),

    _name: $ => choice($.identifier, $.type_identifier),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    type_identifier: $ => /[A-Z][A-Za-z0-9_]*/,

    version: $ => token(/[0-9][A-Za-z0-9_.-]*/),

    number: $ => token(seq(
      /[0-9]+(_[0-9]+)*/,
      optional(choice('i32', 'i64', 'u32', 'u64')),
    )),

    string: $ => token(seq(
      '"',
      repeat(choice(/[^"\\\n]/, /\\./)),
      '"',
    )),

    comment: $ => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),
  },
});
