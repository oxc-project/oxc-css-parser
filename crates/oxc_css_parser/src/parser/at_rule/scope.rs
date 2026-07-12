use super::Parser;
use crate::{
    Parse,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
};

// to ( <scope-end> ) where <scope-end> = <selector-list>
impl<'a> Parse<'a> for ScopeEnd<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let token = input.cursor.bump()?;
        let to_span = if token.is_ident_name_eq_ignore_ascii_case(input.source, "to") {
            token.span
        } else {
            return Err(Error { kind: ErrorKind::ExpectScopeTo, span: token.span });
        };

        let (_, lparen_span) = input.cursor.expect_l_paren()?;
        // An empty scope root/limit (`@scope ()`, `to ()`) is accepted:
        // <scope-start>/<scope-end> are forgiving selector lists, and
        // lightningcss emits this form when every selector is dropped.
        let selector = if let Token::RParen(..) = input.cursor.peek()?.token {
            None
        } else {
            Some(input.parse()?)
        };
        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;

        let span = Span { start: to_span.start, end };
        Ok(ScopeEnd { to_span, lparen_span, selector, span })
    }
}

// https://drafts.csswg.org/css-cascade-6/#scope-syntax
//
// @scope [ ( <scope-start> ) ]? [ to ( <scope-end> ) ]? { <block-contents> }
// <scope-start> = <selector-list>
// <scope-end>   = <selector-list>
impl<'a> Parse<'a> for ScopePrelude<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let start = if let Token::LParen(..) = input.cursor.peek()?.token {
            Some(input.parse::<ScopeStart>()?)
        } else {
            None
        };
        let end = if input.cursor.peek()?.is_ident_name_eq_ignore_ascii_case(input.source, "to") {
            Some(input.parse::<ScopeEnd>()?)
        } else {
            None
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

// ( <scope-start> ) where <scope-start> = <selector-list>
impl<'a> Parse<'a> for ScopeStart<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (_, Span { start, .. }) = input.cursor.expect_l_paren()?;
        // See `ScopeEnd`: an empty `@scope ()` root is accepted.
        let selector = if let Token::RParen(..) = input.cursor.peek()?.token {
            None
        } else {
            Some(input.parse()?)
        };
        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;

        Ok(ScopeStart { selector, span: Span { start, end } })
    }
}
