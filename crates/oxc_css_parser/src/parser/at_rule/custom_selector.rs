use super::Parser;
use crate::{Parse, ast::*, error::PResult, pos::Span, tokenizer::Token, util};

impl<'a> Parse<'a> for CustomSelector<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let prefix_arg = if matches!(input.cursor.peek()?.token, Token::DollarVar(..)) {
            Some(input.parse::<CustomSelectorArg>()?)
        } else {
            None
        };

        let (_, colon_span) = input.cursor.expect_colon()?;
        if let Some(prefix_arg) = &prefix_arg {
            util::assert_no_ws_or_comment(&prefix_arg.span, &colon_span)?;
        }
        let name = input.parse::<Ident>()?;
        util::assert_no_ws_or_comment(&colon_span, &name.span)?;

        let args = if matches!(input.cursor.peek()?.token, Token::LParen(..)) {
            Some(input.parse::<CustomSelectorArgs>()?)
        } else {
            None
        };

        let span = Span {
            start: prefix_arg
                .as_ref()
                .map(|prefix_arg| prefix_arg.span.start)
                .unwrap_or(name.span.start),
            end: args.as_ref().map(|args| args.span.end).unwrap_or(name.span.end),
        };
        Ok(CustomSelector { prefix_arg, name, args, span })
    }
}

impl<'a> Parse<'a> for CustomSelectorArg<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (dollar_var, dollar_var_span) = input.cursor.expect_dollar_var()?;
        Ok(CustomSelectorArg {
            name: input.ident(
                dollar_var.ident,
                Span { start: dollar_var_span.start + 1, end: dollar_var_span.end },
            ),
            span: dollar_var_span,
        })
    }
}

impl<'a> Parse<'a> for CustomSelectorArgs<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (_, Span { start, .. }) = input.cursor.expect_l_paren()?;

        let mut args = input.vec();
        let mut comma_spans = input.vec();
        while !matches!(input.cursor.peek()?.token, Token::RParen(..)) {
            args.push(input.parse()?);
            if !matches!(input.cursor.peek()?.token, Token::RParen(..)) {
                comma_spans.push(input.cursor.expect_comma()?.1);
            }
        }

        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
        Ok(CustomSelectorArgs { args, comma_spans, span: Span { start, end } })
    }
}

// https://drafts.csswg.org/css-extensions/#custom-selectors
impl<'a> Parse<'a> for CustomSelectorPrelude<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let custom_selector = input.parse::<CustomSelector>()?;
        let selector = input.parse::<SelectorList>()?;
        let span = Span { start: custom_selector.span.start, end: selector.span.end };
        Ok(CustomSelectorPrelude { custom_selector, selector, span })
    }
}
