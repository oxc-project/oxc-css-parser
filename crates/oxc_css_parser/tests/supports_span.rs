use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

/// Parses `source` as CSS and passes its first `@supports` condition to `check`.
fn with_supports(source: &str, check: impl FnOnce(&SupportsCondition<'_>)) {
    let allocator = Allocator::default();
    let mut parser = ParserBuilder::new(&allocator, source).syntax(Syntax::Css).build();
    let stylesheet = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    let Some(Statement::AtRule(at_rule)) = stylesheet.statements.first() else {
        panic!("expected an at-rule");
    };
    let Some(AtRulePrelude::Supports(condition)) = &at_rule.prelude else {
        panic!("expected a `@supports` prelude");
    };
    check(condition);
}

#[test]
fn selector_term_spans_its_name_and_parens() {
    let source = "@supports selector(/* c */ a) {}";
    with_supports(source, |condition| {
        let [SupportsConditionKind::SupportsInParens(term)] = condition.conditions.as_slice()
        else {
            panic!("expected one term");
        };
        let SupportsInParensKind::Selector(list) = &term.kind else {
            panic!("expected a `selector()` term");
        };
        assert_eq!(term.span.slice(source), "selector(/* c */ a)");
        assert_eq!(list.span.slice(source), "a");
    });
}

// A condition ends where its last term does
#[test]
fn selector_term_after_keyword_spans_to_its_paren() {
    for (source, condition_text) in [
        ("@supports not selector(a) {}", "not selector(a)"),
        ("@supports (a: b) and selector(a) {}", "(a: b) and selector(a)"),
    ] {
        with_supports(source, |condition| {
            assert_eq!(condition.span.slice(source), condition_text);
        });
    }
}
