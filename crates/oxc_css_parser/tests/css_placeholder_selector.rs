use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

fn parse_css(code: &'static str) -> Stylesheet<'static> {
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Css).build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(
        parser.recoverable_errors().is_empty(),
        "recoverable errors: {:?}",
        parser.recoverable_errors()
    );
    ss
}

// postcss-extend-rule: Sass-style placeholders in plain CSS
// (`%thick-border {}` + `@extend %thick-border;`).
#[test]
fn css_placeholder_selector_parses() {
    let ss = parse_css("%thick-border { border: thick dotted red; }");
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    let ComplexSelectorChild::CompoundSelector(compound) = &rule.selector.selectors[0].children[0]
    else {
        panic!("expected compound selector");
    };
    assert!(matches!(compound.children[0], SimpleSelector::SassPlaceholder(..)));
}

#[test]
fn css_placeholder_in_selector_list_and_compound() {
    let ss = parse_css("%a, .b %c:hover { x: y; }");
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    assert_eq!(rule.selector.selectors.len(), 2);
}

// `@extend %x;` is an unknown at-rule in Css; its prelude stays verbatim.
#[test]
fn css_extend_at_rule_parses() {
    let ss = parse_css(".modal { @extend %thick-border; color: red; }");
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    assert!(matches!(rule.block.statements[0], Statement::AtRule(..)));
}

// A `%` NOT glued to an ident is still an error.
#[test]
fn css_bare_percent_selector_is_still_an_error() {
    let allocator = Allocator::default();
    let mut parser = ParserBuilder::new(&allocator, "% { x: y; }").syntax(Syntax::Css).build();
    assert!(parser.parse::<Stylesheet>().is_err());
}
