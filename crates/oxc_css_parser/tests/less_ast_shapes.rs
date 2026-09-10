//! The AST shapes behind less.js-specific syntax: calc parens, `./`, slashed combinators,
//! the variable-declaration boundary.

use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

fn parse_less(code: &'static str) -> Stylesheet<'static> {
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Less).build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(
        parser.recoverable_errors().is_empty(),
        "recoverable errors: {:?}",
        parser.recoverable_errors()
    );
    ss
}

fn first_rule<'a>(ss: &'a Stylesheet<'static>) -> &'a QualifiedRule<'static> {
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    rule
}

fn first_declaration_value<'a>(ss: &'a Stylesheet<'static>) -> &'a [ComponentValue<'static>] {
    let Statement::Declaration(decl) = &first_rule(ss).block.statements[0] else {
        panic!("expected declaration");
    };
    &decl.value
}

fn text<'a>(code: &'a str, span: &oxc_css_parser::pos::Span) -> &'a str {
    &code[span.start..span.end]
}

// A parenthesized calc operand keeps its parens, redundant pairs included.
#[test]
fn calc_parens_are_kept() {
    let code = "a { h: calc(50% + ((@var - 20px))); }";
    let ss = parse_less(code);
    let [ComponentValue::Function(func)] = first_declaration_value(&ss) else {
        panic!("expected function");
    };
    let [ComponentValue::Calc(calc)] = &func.args[..] else {
        panic!("expected calc, got {:?}", func.args);
    };
    let ComponentValue::CalcParenthesized(outer) = &*calc.right else {
        panic!("expected parenthesized operand, got {:?}", calc.right);
    };
    assert_eq!(text(code, &outer.span), "((@var - 20px))");
    let ComponentValue::CalcParenthesized(inner) = &*outer.expr else {
        panic!("expected the inner parens, got {:?}", outer.expr);
    };
    assert_eq!(text(code, &inner.span), "(@var - 20px)");
    assert!(matches!(&*inner.expr, ComponentValue::Calc(..)));
}

// `./` is its own operator: `math=parens-division` divides on it where a plain `/` would not.
#[test]
fn dot_division_is_its_own_operator() {
    let code = "a { c: 2px ./ 2; }";
    let ss = parse_less(code);
    let [ComponentValue::LessBinaryOperation(op)] = first_declaration_value(&ss) else {
        panic!("expected a binary operation, got {:?}", first_declaration_value(&ss));
    };
    assert!(matches!(op.op.kind, LessOperationOperatorKind::DotDivision));
    assert_eq!(text(code, &op.op.span), "./");
}

// `/deep/` keeps its own kind; any other less.js `/name/` is `Slashed`, its name in the span.
#[test]
fn slashed_combinators_keep_their_name() {
    let combinator = |code: &'static str| {
        let ss = parse_less(code);
        let children = &first_rule(&ss).selector.selectors[0].children;
        let ComplexSelectorChild::Combinator(combinator) = &children[1] else {
            panic!("expected combinator, got {children:?}");
        };
        (format!("{:?}", combinator.kind), text(code, &combinator.span))
    };
    assert_eq!(combinator(".a /deep/ .b {}"), ("Deep".to_owned(), "/deep/"));
    assert_eq!(combinator(".a /shadow/ .b {}"), ("Slashed".to_owned(), "/shadow/"));
}

// A variable whose typed value stops before the statement end is not a declaration:
// `@page :first { ... }` is an at-rule even when a `;` follows later.
#[test]
fn page_at_rule_is_not_a_variable_declaration() {
    let ss = parse_less("@page :first { margin: 3cm; }\n@x: 1;");
    assert!(matches!(&ss.statements[0], Statement::AtRule(at_rule) if at_rule.name.raw == "page"));
    assert!(matches!(&ss.statements[1], Statement::LessVariableDeclaration(..)));
}

// The permissive read stays for values the typed grammar cannot start on.
#[test]
fn permissive_variable_value_still_parses() {
    let ss = parse_less("@this: () => { anything; until the semi; };");
    assert!(matches!(&ss.statements[0], Statement::LessVariableDeclaration(..)));
}
