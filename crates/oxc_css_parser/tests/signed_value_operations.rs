use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

fn parse(code: &'static str, syntax: Syntax) -> Stylesheet<'static> {
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(syntax).build();
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

/// The left operand, operator and right operand of the only value, a binary operation.
fn less_binary(code: &'static str) -> (&'static str, &'static str, &'static str) {
    let ss = parse(code, Syntax::Less);
    let [ComponentValue::LessBinaryOperation(op)] = first_declaration_value(&ss) else {
        panic!("expected a binary operation for {code:?}, got {:?}", first_declaration_value(&ss));
    };
    (op.left.span().slice(code), op.op.span.slice(code), op.right.span().slice(code))
}

fn first_function_args<'a>(ss: &'a Stylesheet<'static>) -> &'a [ComponentValue<'static>] {
    let [ComponentValue::Function(function)] = first_declaration_value(ss) else {
        panic!("expected a single function value");
    };
    &function.args
}

// lessc: a comma can never be an operand, `func(20px, +20px)` is two args
// (`+20px` is a signed value); previously this ASTed as a comma-left
// `LessBinaryOperation`.
#[test]
fn less_comma_is_never_an_operand() {
    for code in
        ["a { p: func(20px, +20px); }", "a { p: func(20px,+20px); }", "a { p: func(20px,-20px); }"]
    {
        let ss = parse(code, Syntax::Less);
        let [
            ComponentValue::Dimension(_),
            ComponentValue::Delimiter(comma),
            ComponentValue::Dimension(_),
        ] = first_function_args(&ss)
        else {
            panic!("expected dim/comma/dim for {code:?}, got {:?}", first_function_args(&ss));
        };
        assert!(matches!(comma.kind, DelimiterKind::Comma));
    }
}

// lessc glue rules are symmetric for `+` and `-`: a sign folded into the
// number token is a binary operator only when glued to the left operand
// (`10px+20px` is `30px`, `10px +20px` is the list `10px 20px`).
#[test]
fn less_folded_plus_requires_glue() {
    let ss = parse("a { p: 10px +20px; }", Syntax::Less);
    let [ComponentValue::Dimension(_), ComponentValue::Dimension(_)] = first_declaration_value(&ss)
    else {
        panic!("expected two values, got {:?}", first_declaration_value(&ss));
    };

    let ss = parse("a { p: 10px+20px; }", Syntax::Less);
    let [ComponentValue::LessBinaryOperation(op)] = first_declaration_value(&ss) else {
        panic!("expected a binary operation, got {:?}", first_declaration_value(&ss));
    };
    assert!(matches!(op.op.kind, LessOperationOperatorKind::Plus));
}

// SassScript: a sign glued to a function-call operand inside a calculation is
// a folded binary operator (`max(map-get($m, a)-1, 0)` is `... - 1`), same as
// the value-position path; previously the `-1` split off as a signed value.
#[test]
fn scss_calc_arg_glued_sign_is_subtraction() {
    let ss = parse("a { w: max(map-get($m, a)-1, 0); }", Syntax::Scss);
    let [ComponentValue::Calc(calc), ComponentValue::Delimiter(_), ComponentValue::Number(_)] =
        first_function_args(&ss)
    else {
        panic!("expected calc/comma/number, got {:?}", first_function_args(&ss));
    };
    assert!(matches!(calc.op.kind, CalcOperatorKind::Minus));
    assert!(matches!(&*calc.left, ComponentValue::Function(_)));
    assert!(matches!(&*calc.right, ComponentValue::Number(_)));
}

// Dimension right operand + the multiplicative tail: `*` binds tighter than
// the split-off sign, `max(f()-1px*2, ...)` is `f() - (1px * 2)`.
#[test]
fn scss_calc_arg_glued_dimension_and_mul_tail() {
    let ss = parse("a { w: max(map-get($m, a)-1px*2, 0px); }", Syntax::Scss);
    let [ComponentValue::Calc(calc), ComponentValue::Delimiter(_), ComponentValue::Dimension(_)] =
        first_function_args(&ss)
    else {
        panic!("expected calc/comma/dim, got {:?}", first_function_args(&ss));
    };
    assert!(matches!(calc.op.kind, CalcOperatorKind::Minus));
    assert!(matches!(&*calc.left, ComponentValue::Function(_)));
    let ComponentValue::Calc(mul) = &*calc.right else {
        panic!("expected mul-bound right, got {:?}", calc.right);
    };
    assert!(matches!(mul.op.kind, CalcOperatorKind::Multiply));
}

// A spaced `-1` stays a signed value (dart-sass reads `x -1` as two list
// elements, not subtraction).
#[test]
fn scss_calc_arg_spaced_sign_stays_a_value() {
    let ss = parse("a { w: max(map-get($m, a) -1, 0); }", Syntax::Scss);
    let [
        ComponentValue::Function(_),
        ComponentValue::Number(_),
        ComponentValue::Delimiter(_),
        ComponentValue::Number(_),
    ] = first_function_args(&ss)
    else {
        panic!("expected fn/number/comma/number, got {:?}", first_function_args(&ss));
    };
}

// Word-glued runs stay separate values (they print verbatim as one postcss
// word); only a function-call left operand folds the sign into an operator.
#[test]
fn scss_calc_word_glued_run_stays_values() {
    let ss = parse("a { w: max(100%-20px, 0px); }", Syntax::Scss);
    let [
        ComponentValue::Percentage(_),
        ComponentValue::Dimension(_),
        ComponentValue::Delimiter(_),
        ComponentValue::Dimension(_),
    ] = first_function_args(&ss)
    else {
        panic!("expected pct/dim/comma/dim, got {:?}", first_function_args(&ss));
    };
}

// less.js `addition` takes a `+`/`-` glued to its left operand as a binary operator
// (`!isSpaced && ($char('+') || $char('-'))`), when an operand follows it.
#[test]
fn less_glued_sign_delimiter_is_binary_operator() {
    for (code, expected) in [
        ("a { p: 1+-1; }", ("1", "+", "-1")),
        ("a { p: 1+-(1); }", ("1", "+", "-(1)")),
        ("a { p: (-1)+-1; }", ("(-1)", "+", "-1")),
        ("a { p: 1+(1); }", ("1", "+", "(1)")),
        ("a { p: 1-(1); }", ("1", "-", "(1)")),
        ("a { p: 2+round(1.5); }", ("2", "+", "round(1.5)")),
        ("a { p: @a+@b; }", ("@a", "+", "@b")),
        ("a { p: 1px+@a; }", ("1px", "+", "@a")),
    ] {
        assert_eq!(less_binary(code), expected, "{code:?}");
    }
}

// Spaced before but not after, a `-` is no operator, and less.js negates only a glued `@var`, `$prop` or `(`:
// its keyword regex (`[\w-]+`) reads the `-` alone.
#[test]
fn less_lone_minus_before_non_negatable_is_keyword() {
    for (code, rest) in [("a { p: 1 -\"t\"; }", "\"t\""), ("a { p: 1 -#fff; }", "#fff")] {
        let ss = parse(code, Syntax::Less);
        let [ComponentValue::Number(_), ComponentValue::InterpolableIdent(minus), value] =
            first_declaration_value(&ss)
        else {
            panic!(
                "expected number/`-`/value for {code:?}, got {:?}",
                first_declaration_value(&ss)
            );
        };
        assert_eq!(minus.span().slice(code), "-");
        assert_eq!(value.span().slice(code), rest);
    }

    // No operand after a glued `-` either: still the keyword, as before
    for code in ["a { p: 1-; }", "a { p: (1)-; }"] {
        let ss = parse(code, Syntax::Less);
        let [_, ComponentValue::InterpolableIdent(minus)] = first_declaration_value(&ss) else {
            panic!("expected value/`-` for {code:?}, got {:?}", first_declaration_value(&ss));
        };
        assert_eq!(minus.span().slice(code), "-");
    }

    let code = "a { p: 1 -(1); }";
    let ss = parse(code, Syntax::Less);
    let [ComponentValue::Number(_), ComponentValue::LessNegativeValue(_)] =
        first_declaration_value(&ss)
    else {
        panic!("expected number/negation, got {:?}", first_declaration_value(&ss));
    };
}

// lessc does no math in a custom property value, so a glued sign stays as written there
#[test]
fn less_custom_property_glued_sign_is_no_operator() {
    for code in ["a { --x: a+b; }", "a { --x: 1+(1); }"] {
        let ss = parse(code, Syntax::Less);
        assert!(
            !first_declaration_value(&ss)
                .iter()
                .any(|value| matches!(value, ComponentValue::LessBinaryOperation(_))),
            "expected no binary operation for {code:?}, got {:?}",
            first_declaration_value(&ss)
        );
    }
}
