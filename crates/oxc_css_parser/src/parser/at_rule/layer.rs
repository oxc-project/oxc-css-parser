use super::Parser;
use crate::{
    Parse,
    ast::*,
    error::{Error, PResult},
    pos::{Span, Spanned},
    tokenizer::{Token, TokenWithSpan},
    util,
};

// https://drafts.csswg.org/css-cascade-5/#layering
impl<'a> Parse<'a> for LayerNames<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let first = input.parse::<LayerName>()?;
        let mut span = first.span.clone();

        let mut names = input.vec1(first);
        let mut comma_spans = input.vec();
        while let Some((_, comma_span)) = input.cursor.eat_comma()? {
            comma_spans.push(comma_span);
            names.push(input.parse()?);
        }

        if let Some(last) = names.last() {
            span.end = last.span.end;
        }
        Ok(LayerNames { names, comma_spans, span })
    }
}

// https://drafts.csswg.org/css-cascade-5/#layer-names
impl<'a> Parse<'a> for LayerName<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let first = input.parse::<InterpolableIdent>()?;
        let start = first.span().start;
        let mut end = first.span().end;

        let mut idents = input.vec1(first);
        while let TokenWithSpan { token: Token::Dot(..), span } = input.cursor.peek()? {
            if span.start == end {
                let span = input.cursor.bump()?.span;
                let ident = input.parse::<InterpolableIdent>()?;
                util::assert_no_ws_or_comment(&span, ident.span())?;
                end = ident.span().end;
                idents.push(ident);
            } else {
                break;
            }
        }

        let invalid_ident = idents.iter().find(|ident| match &ident {
            InterpolableIdent::Literal(ident) => util::is_css_wide_keyword(ident.name),
            _ => false,
        });
        if let Some(invalid_ident) = invalid_ident {
            input.recoverable_errors.push(Error {
                kind: crate::error::ErrorKind::CSSWideKeywordDisallowed,
                span: invalid_ident.span().clone(),
            });
        }

        let span = Span { start, end };
        Ok(LayerName { idents, span })
    }
}
