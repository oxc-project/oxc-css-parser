use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

#[test]
fn raw_prelude_keeps_semicolons_inside_balanced_groups() {
    let allocator = Allocator::default();
    let source = "10(foo;bar) {}\nx: fn(a;b) {}";
    let mut parser = ParserBuilder::new(&allocator, source).syntax(Syntax::Css).build();
    let stylesheet = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    assert_eq!(stylesheet.statements.len(), 2);

    let expected = ["10(foo;bar)", "x: fn(a;b)"];
    for (statement, expected) in stylesheet.statements.iter().zip(expected) {
        let Statement::UnknownQualifiedRule(rule) = statement else {
            panic!("expected raw-prelude rule, got {statement:?}");
        };
        assert_eq!(&source[rule.prelude.span.start..rule.prelude.span.end], expected);
    }
}
