use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*, token::Token};

fn parse_scss(code: &'static str) -> Stylesheet<'static> {
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Scss).build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(
        parser.recoverable_errors().is_empty(),
        "recoverable errors: {:?}",
        parser.recoverable_errors()
    );
    ss
}

fn first_declaration_value<'a>(ss: &'a Stylesheet<'static>) -> &'a [ComponentValue<'static>] {
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    let Statement::Declaration(decl) = &rule.block.statements[0] else {
        panic!("expected declaration");
    };
    &decl.value
}

// `foo.bar` with no glued call is not Sass (dart-sass rejects a plain ident
// member at compile time), but postcss-scss lexes the dotted run as ONE word
// (xstyled / tailwind-theme tokens). It parses as ident + raw `.` + ident,
// the same shape Css mode produces.
#[test]
fn dotted_word_parses_as_raw_tokens() {
    let ss = parse_scss("a { color: foo.bar; }");
    let [
        ComponentValue::InterpolableIdent(_),
        ComponentValue::TokenWithSpan(dot),
        ComponentValue::InterpolableIdent(_),
    ] = first_declaration_value(&ss)
    else {
        panic!("expected ident/dot/ident, got {:?}", first_declaration_value(&ss));
    };
    assert!(matches!(dot.token, Token::Dot(..)));
}

#[test]
fn dotted_word_chain_parses() {
    let ss = parse_scss("a { color: colors.modes.dark; }");
    assert_eq!(first_declaration_value(&ss).len(), 5);
}

#[test]
fn dotted_word_with_number_tail_parses() {
    // `.10` lexes as a number, so the run is ident + number (no dot token).
    let ss = parse_scss("a { color: sandstone.10; }");
    let [ComponentValue::InterpolableIdent(_), ComponentValue::Number(number)] =
        first_declaration_value(&ss)
    else {
        panic!("expected ident/number, got {:?}", first_declaration_value(&ss));
    };
    assert_eq!(number.raw, ".10");
}

// The real namespaced forms keep their typed shapes.
#[test]
fn namespaced_function_call_stays_function() {
    let ss = parse_scss("@use \"sass:math\";\na { b: math.div(10, 2); }");
    let Statement::QualifiedRule(rule) = &ss.statements[1] else {
        panic!("expected qualified rule");
    };
    let Statement::Declaration(decl) = &rule.block.statements[0] else {
        panic!("expected declaration");
    };
    let [ComponentValue::Function(func)] = &decl.value[..] else {
        panic!("expected function");
    };
    assert!(matches!(func.name, FunctionName::SassQualifiedName(..)));
}

#[test]
fn namespaced_variable_stays_qualified_name() {
    let ss = parse_scss("@use \"sass:math\";\na { b: math.$pi; }");
    let Statement::QualifiedRule(rule) = &ss.statements[1] else {
        panic!("expected qualified rule");
    };
    let Statement::Declaration(decl) = &rule.block.statements[0] else {
        panic!("expected declaration");
    };
    assert!(matches!(&decl.value[..], [ComponentValue::SassQualifiedName(..)]));
}

// A `.` NOT glued to a following ident is still an error.
#[test]
fn lone_dot_is_still_an_error() {
    let allocator = Allocator::default();
    let mut parser = ParserBuilder::new(&allocator, "a { b: foo. ; }").syntax(Syntax::Scss).build();
    assert!(parser.parse::<Stylesheet>().is_err());
}
