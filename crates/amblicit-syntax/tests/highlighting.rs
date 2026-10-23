use amblicit_syntax::{highlight_source, HighlightRole, HighlightToken};

fn lexeme<'a>(src: &'a str, token: &HighlightToken) -> &'a str {
    token.lexeme(src)
}

fn find_role(src: &str, tokens: &[HighlightToken], needle: &str) -> HighlightRole {
    tokens
        .iter()
        .find(|token| lexeme(src, token) == needle)
        .unwrap_or_else(|| panic!("missing highlighted lexeme `{needle}`"))
        .role
}

#[test]
fn highlight_source_emits_editor_roles_for_fixture() {
    let src = include_str!("../../../fixtures/lang/tooling/highlight-smoke.edict");

    let tokens = highlight_source(src).expect("highlight fixture");

    assert!(
        tokens
            .iter()
            .all(|token| !lexeme(src, token).chars().all(char::is_whitespace)),
        "highlighting must not emit whitespace-only tokens"
    );
    assert_eq!(
        find_role(src, &tokens, "// editor tooling fixture"),
        HighlightRole::Comment
    );
    assert_eq!(
        find_role(src, &tokens, "/* comments remain editor-visible */"),
        HighlightRole::Comment
    );
    assert_eq!(find_role(src, &tokens, "package"), HighlightRole::Keyword);
    assert_eq!(find_role(src, &tokens, "lawpack"), HighlightRole::Keyword);
    assert_eq!(
        find_role(src, &tokens, "examples"),
        HighlightRole::Identifier
    );
    assert_eq!(
        find_role(src, &tokens, "HelloInput"),
        HighlightRole::TypeIdentifier
    );
    assert_eq!(
        find_role(src, &tokens, "\"hello, \""),
        HighlightRole::String
    );
    assert_eq!(find_role(src, &tokens, "256"), HighlightRole::Number);
    assert_eq!(find_role(src, &tokens, "<="), HighlightRole::Operator);
    assert_eq!(find_role(src, &tokens, ";"), HighlightRole::Punctuation);
}

#[test]
fn request_statement_introducer_is_highlighted_as_a_keyword() {
    let src = "request pending: ExternalActionRequest<Bytes<max=1>> = snapshot(input);";
    let tokens = highlight_source(src).expect("request source highlights");

    assert_eq!(find_role(src, &tokens, "request"), HighlightRole::Keyword);
}
