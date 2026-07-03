use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

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

// `50....` is `50.` + `...`: the number owns the first dot (postcss lexes the
// word `50.`) and a spread marker still follows — no stray `.` token.
#[test]
fn four_dots_after_number_split_as_trailing_dot_plus_spread() {
    let ss = parse_scss("a { color: rgba(50 50 50 50....); }");
    let [ComponentValue::Function(func)] = first_declaration_value(&ss) else {
        panic!("expected function value");
    };
    let Some(ComponentValue::SassArbitraryArgument(arg)) = func.args.last() else {
        panic!("expected arbitrary argument, got {:?}", func.args.last());
    };
    let ComponentValue::Number(number) = &*arg.value else {
        panic!("expected number");
    };
    assert_eq!(number.raw, "50.");
    // The spread consumed the remaining three dots; nothing is left over.
    assert_eq!(func.args.len(), 4);
}

// The 1-3 dot runs keep their existing split.
#[test]
fn spread_after_number_is_untouched() {
    let ss = parse_scss("a { width: min(50px 20px 30px...); }");
    let [ComponentValue::Function(func)] = first_declaration_value(&ss) else {
        panic!("expected function value");
    };
    assert!(matches!(func.args.last(), Some(ComponentValue::SassArbitraryArgument(_))));
}

#[test]
fn fraction_still_parses() {
    let ss = parse_scss("a { width: 50.5px; }");
    let [ComponentValue::Dimension(dimension)] = first_declaration_value(&ss) else {
        panic!("expected dimension value");
    };
    assert_eq!(dimension.value.raw, "50.5");
}
