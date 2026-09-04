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

fn declarations<'a>(ss: &'a Stylesheet<'static>) -> Vec<&'a Declaration<'static>> {
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    rule.block
        .statements
        .iter()
        .map(|stmt| match stmt {
            Statement::Declaration(decl) => decl,
            other => panic!("expected declaration, got {other:?}"),
        })
        .collect()
}

// A custom property value runs to the top-level `;` (§5.5.6); dart-sass reads it
// as text. `{ ... }` followed by more text is ONE declaration, whatever the
// typed value parse makes of the block.
const SEMILESS_BLOCK: &str = ":root {
  --like-a-apply-rule: {
  color:red;} /* no semi here*/
  --another-prop: blue;
}";

#[test]
fn scss_semiless_block_stays_one_declaration() {
    let ss = parse(SEMILESS_BLOCK, Syntax::Scss);
    let decls = declarations(&ss);
    assert_eq!(decls.len(), 1, "{decls:#?}");
    let InterpolableIdent::Literal(name) = &decls[0].name else {
        panic!("expected literal name");
    };
    assert_eq!(name.name, "--like-a-apply-rule");
    // Raw tokens, not a `SassNestingDeclaration`.
    assert!(decls[0].value_is_raw);
    assert!(decls[0].value.iter().all(|value| matches!(value, ComponentValue::TokenWithSpan(..))));
}

// A terminated block still takes the typed path.
#[test]
fn scss_terminated_block_stays_typed() {
    let ss =
        parse(":root {\n  --centered: {\n    display: flex;\n  };\n  --x: 1;\n}", Syntax::Scss);
    let decls = declarations(&ss);
    assert_eq!(decls.len(), 2);
    assert!(!decls[0].value_is_raw);
    assert!(matches!(decls[0].value.last(), Some(ComponentValue::SassNestingDeclaration(..))));
}

// A trailing `!important` belongs to the typed value.
#[test]
fn scss_typed_value_with_important() {
    let ss = parse(":root { --x: 1px !important; --y: 2; }", Syntax::Scss);
    let decls = declarations(&ss);
    assert_eq!(decls.len(), 2);
    assert!(matches!(decls[0].value[..], [ComponentValue::Dimension(..)]));
    assert!(decls[0].important.is_some());
}

// Anything the typed parse cannot account for keeps the whole value raw.
#[test]
fn scss_bad_important_falls_back_to_raw() {
    let ss = parse(":root { --x: 1px !foo; --y: 2; }", Syntax::Scss);
    let decls = declarations(&ss);
    assert_eq!(decls.len(), 2);
    assert!(decls[0].important.is_none());
    assert!(decls[0].value_is_raw);
}

// CSS Syntax removes a trailing top-level `!important` even when the value is
// represented as preserved raw tokens. Comments and escapes between the two
// pieces do not change the annotation; a different ident remains raw.
#[test]
fn raw_value_extracts_trailing_important() {
    let ss = parse(
        ":root { x: .foo > bar ! /* gap */ ImPoRtAnT; --x: */ !\\69mportant; y: .foo !nope; }",
        Syntax::Css,
    );
    let decls = declarations(&ss);
    assert_eq!(decls.len(), 3, "{decls:#?}");

    for decl in &decls[..2] {
        assert!(decl.value_is_raw);
        let important = decl.important.as_ref().expect("expected structural !important");
        assert!(important.ident.name.eq_ignore_ascii_case("important"));
        assert!(decl.value.iter().all(|value| {
            !matches!(value, ComponentValue::TokenWithSpan(token) if matches!(token.token, oxc_css_parser::token::Token::Exclamation(..)))
        }));
    }

    assert!(decls[2].value_is_raw);
    assert!(decls[2].important.is_none());
    assert!(decls[2].value.iter().any(|value| {
        matches!(value, ComponentValue::TokenWithSpan(token) if matches!(token.token, oxc_css_parser::token::Token::Exclamation(..)))
    }));
}

#[test]
fn raw_value_does_not_extract_important_from_unclosed_group() {
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser =
        ParserBuilder::new(allocator, "a { x: (.foo !important").syntax(Syntax::Css).build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(!parser.recoverable_errors().is_empty());

    let decls = declarations(&ss);
    assert_eq!(decls.len(), 1);
    assert!(decls[0].value_is_raw);
    assert!(decls[0].important.is_none());
}

// dart-sass keeps a Scss custom property value as text, where `//` is not a
// comment. A value containing it is therefore tokenized with comments off from
// the start, whether or not a comments-on typed parse could reach `;`.
#[test]
fn scss_custom_property_reads_line_comment_syntax_as_text() {
    let code = ":root {\n  --a: // (\n    );\n  --b: url(http://foo.com/bar);\n  --c: 1 // note;\n  --d: */ // after;\n}";
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Scss).comments().build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    assert!(parser.comments().is_empty(), "`//` in the value is not a comment");

    let decls = declarations(&ss);
    assert_eq!(decls.len(), 4, "{decls:#?}");
    assert!(decls[0].value_is_raw);
    assert!(!decls[1].value_is_raw);
    assert!(matches!(decls[1].value[..], [ComponentValue::Url(..)]));
    assert!(decls[2].value_is_raw);
    let value = &decls[2].value;
    let value_span = value.first().unwrap().span().start..value.last().unwrap().span().end;
    assert_eq!(&code[value_span], "1 // note");
    assert!(decls[3].value_is_raw);
    let value = &decls[3].value;
    let value_span = value.first().unwrap().span().start..value.last().unwrap().span().end;
    assert_eq!(&code[value_span], "*/ // after");
}

// Inside `#{...}` the grammar switches back to SassScript, where `//` is a
// comment. Delimiters in that comment must not close the interpolation or the
// surrounding style rule.
#[test]
fn scss_custom_property_interpolation_keeps_line_comment_syntax() {
    let code = ":root {\n  --x: #{1 // }\n  };\n  color: red;\n}";
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Scss).comments().build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    assert_eq!(parser.comments().len(), 1, "the SassScript comment must be collected");

    let decls = declarations(&ss);
    assert_eq!(decls.len(), 2, "{decls:#?}");
    assert!(decls[0].value_is_raw);
    assert!(decls[0].value.iter().all(|value| matches!(value, ComponentValue::TokenWithSpan(..))));
}

// The string scanner consumes the `#` of `#{` before the tokenizer sees the
// interpolation's `{`. It must still re-enable SassScript comment syntax: the
// `}` in the comment is text, and the next line's `}` closes the interpolation.
#[test]
fn scss_custom_property_string_interpolation_keeps_line_comment_syntax() {
    let code = ":root {\n  --x: \"#{1 // }\n  }\";\n  color: red;\n}";
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Scss).comments().build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    assert_eq!(parser.comments().len(), 1, "the SassScript comment must be collected");

    let decls = declarations(&ss);
    assert_eq!(decls.len(), 2, "{decls:#?}");
    assert!(decls[0].value_is_raw);
    assert!(matches!(decls[0].value[..], [ComponentValue::InterpolableStr(..)]));
}

// `\#` is an escaped hash, not the `#` of a Sass interpolation. The following
// `{` is therefore an ordinary custom-property value block, and `//` remains
// text whose `}` closes that block.
#[test]
fn scss_custom_property_escaped_hash_does_not_start_interpolation() {
    let code = ".a {\n  --x: \\#{ // }\n  ;\n  color: red;\n}";
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Scss).comments().build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    assert!(parser.comments().is_empty(), "`//` in the custom-property value is text");

    let decls = declarations(&ss);
    assert_eq!(decls.len(), 2, "{decls:#?}");
    assert!(decls[0].value_is_raw);
    let value = &decls[0].value;
    let value_span = value.first().unwrap().span().start..value.last().unwrap().span().end;
    assert_eq!(&code[value_span], "\\#{ // }");
}

// A static `--` prefix still makes the name a custom property when Sass
// interpolation supplies the rest of it. Its value therefore follows the
// same text grammar as a literal custom-property name.
#[test]
fn scss_interpolated_custom_property_name_reads_value_as_text() {
    let code = ".a { --#{$name}: // note; color: red; --theme-#{$name}: */; }";
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(Syntax::Scss).comments().build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(parser.recoverable_errors().is_empty(), "{:?}", parser.recoverable_errors());
    assert!(parser.comments().is_empty(), "`//` in the custom-property value is text");

    let decls = declarations(&ss);
    assert_eq!(decls.len(), 3, "{decls:#?}");
    assert!(matches!(decls[0].name, InterpolableIdent::SassInterpolated(..)));
    assert!(decls[0].value_is_raw);
    assert!(matches!(decls[1].name, InterpolableIdent::Literal(..)));
    assert!(matches!(decls[2].name, InterpolableIdent::SassInterpolated(..)));
    assert!(decls[2].value_is_raw);
}
