use super::Parser;
use crate::{
    Parse,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
};

impl<'a> Parse<'a> for ScopeEnd<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let to_span = match input.cursor.bump()? {
            TokenWithSpan { token: Token::Ident(ident), span }
                if ident.name().eq_ignore_ascii_case("to") =>
            {
                span
            }
            TokenWithSpan { span, .. } => {
                return Err(Error { kind: ErrorKind::ExpectScopeTo, span });
            }
        };

        let (_, lparen_span) = input.cursor.expect_l_paren()?;
        let selector = input.parse()?;
        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;

        let span = Span { start: to_span.start, end };
        Ok(ScopeEnd { to_span, lparen_span, selector, span })
    }
}

// https://drafts.csswg.org/css-cascade-6/#scope-syntax
impl<'a> Parse<'a> for ScopePrelude<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let start = if let Token::LParen(..) = input.cursor.peek()?.token {
            Some(input.parse::<ScopeStart>()?)
        } else {
            None
        };
        let end = match &input.cursor.peek()?.token {
            Token::Ident(ident) if ident.name().eq_ignore_ascii_case("to") => {
                Some(input.parse::<ScopeEnd>()?)
            }
            _ => None,
        };

        match (start, end) {
            (Some(start), Some(end)) => {
                let span = Span { start: start.span.start, end: end.span.end };
                Ok(ScopePrelude::Both(ScopeStartWithEnd { start, end, span }))
            }
            (Some(start), None) => Ok(ScopePrelude::StartOnly(start)),
            (None, Some(end)) => Ok(ScopePrelude::EndOnly(end)),
            (None, None) => {
                let TokenWithSpan { token, span } = input.cursor.bump()?;
                Err(Error { kind: ErrorKind::Unexpected("(", token.symbol()), span })
            }
        }
    }
}

impl<'a> Parse<'a> for ScopeStart<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (_, Span { start, .. }) = input.cursor.expect_l_paren()?;
        let selector = input.parse()?;
        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;

        Ok(ScopeStart { selector, span: Span { start, end } })
    }
}
