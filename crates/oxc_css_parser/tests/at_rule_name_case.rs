use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

/// Whether the first statement, an at-rule, got a typed prelude.
fn is_typed(source: &str, syntax: Syntax) -> bool {
    let allocator = Allocator::default();
    let mut parser = ParserBuilder::new(&allocator, source).syntax(syntax).build();
    let stylesheet = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    let Some(Statement::AtRule(at_rule)) = stylesheet.statements.first() else {
        panic!("expected an at-rule for {source:?}");
    };
    !matches!(at_rule.prelude, Some(AtRulePrelude::Unknown(_)))
}

// dart-sass matches at-rule names case-sensitively; css-syntax-3 and less.js do not
#[test]
fn uppercase_at_rule_is_unknown_in_scss_only() {
    let source = "@MEDIA (MIN-WIDTH: 700PX) {}";
    assert!(!is_typed(source, Syntax::Scss));
    assert!(is_typed(source, Syntax::Css));
    assert!(is_typed(source, Syntax::Less));
    assert!(is_typed("@media (MIN-WIDTH: 700PX) {}", Syntax::Scss));
}
