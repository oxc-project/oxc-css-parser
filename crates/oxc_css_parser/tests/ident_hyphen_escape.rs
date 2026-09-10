use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

fn first_declaration_value(code: &'static str) -> &'static [ComponentValue<'static>] {
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Css).build();
    let ss: &'static Stylesheet<'static> =
        Box::leak(Box::new(parser.parse::<Stylesheet>().unwrap()));
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    let Statement::QualifiedRule(rule) = &ss.statements[0] else { panic!("expected rule") };
    let Statement::Declaration(decl) = &rule.block.statements[0] else { panic!("expected decl") };
    &decl.value
}

// A leading hyphen followed by an escape is one ident: the escape is scanned
// (its terminating space included) and the ident is flagged escaped.
#[test]
fn escape_after_leading_hyphen_is_one_ident() {
    let [ComponentValue::InterpolableIdent(InterpolableIdent::Literal(ident))] =
        first_declaration_value("a { b: -\\31 x; }")
    else {
        panic!("expected one ident, got {:?}", first_declaration_value("a { b: -\\31 x; }"));
    };
    assert_eq!(ident.raw, "-\\31 x");
    assert_eq!(ident.name, "-1x");
}
