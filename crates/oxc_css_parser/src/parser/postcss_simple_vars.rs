use super::{
    Parser,
    state::{ParserState, QualifiedRuleContext},
};
use crate::{Parse, ast::*, config::Syntax, error::PResult, pos::Span};

// postcss-simple-vars variable reference: `$` <ident>
// https://github.com/postcss/postcss-simple-vars
impl<'a> Parse<'a> for PostcssSimpleVar<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        debug_assert!(input.syntax == Syntax::Css);

        let (name, span) = input.parse_dollar_var_ident()?;
        Ok(PostcssSimpleVar { name, span })
    }
}

// postcss-simple-vars declaration: `$` <ident> ':' <declaration-value>
// (textual substitution; a trailing `!important` stays part of the value)
impl<'a> Parse<'a> for PostcssSimpleVarDeclaration<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        debug_assert!(input.syntax == Syntax::Css);

        let name = input.parse::<PostcssSimpleVar>()?;
        let (_, colon_span) = input.cursor.expect_colon()?;
        // `$var: value` is already a postcss declaration shape,
        // so the typed node keeps the CSS `<any-value>` acceptance
        // (a top-level `{}` still rejects it as a raw-prelude rule for the statement disambiguation path).
        let (mut value, important, value_is_raw) = input
            .with_state(ParserState {
                qualified_rule_ctx: Some(QualifiedRuleContext::DeclarationValue),
                in_statement: true,
                ..input.state
            })
            .parse_css_any_value()?;
        // postcss-simple-vars is textual substitution;
        // `!important` is part of the value, not a structural declaration modifier
        // (unlike CSS's `Declaration.important`).
        // Keep a valid trailing annotation in the value stream;
        // a non-`important` bang is already in the raw fallback.
        if let Some(important) = important {
            value.push(ComponentValue::ImportantAnnotation(important));
        }

        let end = value.last().map(|v| v.span().end).unwrap_or(colon_span.end);
        let span = Span { start: name.span.start, end };

        Ok(PostcssSimpleVarDeclaration { name, colon_span, value, value_is_raw, span })
    }
}
