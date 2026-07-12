use super::Parser;
use crate::{
    Parse,
    ast::*,
    error::PResult,
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
    util,
};

// https://www.w3.org/TR/css-page-3/#syntax-page-selector
//
// <page-selector> = [ <ident-token>? <pseudo-page>* ]!
impl<'a> Parse<'a> for PageSelector<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let mut name = None;
        let mut pseudo = input.vec();
        let start;
        let mut end;

        if let Token::Colon(..) | Token::ColonColon(..) = &input.cursor.peek()?.token {
            let first = input.parse::<PseudoPage>()?;
            start = first.span.start;
            end = first.span.end;
            pseudo.push(first);
        } else {
            let ident = input.parse::<InterpolableIdent>()?;
            let ident_span = ident.span();
            start = ident_span.start;
            end = ident_span.end;
            name = Some(ident)
        }

        loop {
            match input.cursor.peek()? {
                TokenWithSpan { token: Token::Colon(..) | Token::ColonColon(..), span }
                    if span.start == end =>
                {
                    let item = input.parse::<PseudoPage>()?;
                    end = item.span.end;
                    pseudo.push(item);
                }
                _ => break,
            }
        }

        Ok(PageSelector { name, pseudo, span: Span { start, end } })
    }
}

// <page-selector-list> = <page-selector>#
impl<'a> Parse<'a> for PageSelectorList<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let first = input.parse::<PageSelector>()?;
        let mut span = first.span;

        let mut selectors = input.vec1(first);
        let mut comma_spans = input.vec();
        while let Some((_, comma_span)) = input.cursor.eat_comma()? {
            comma_spans.push(comma_span);
            selectors.push(input.parse()?);
        }

        if let Some(last) = selectors.last() {
            span.end = last.span.end;
        }
        Ok(PageSelectorList { selectors, comma_spans, span })
    }
}

// <pseudo-page> = ':' [ left | right | first | blank ]
//
// CSS Template Layout additionally uses a pseudo-element-style functional
// form: `@page::slot(g)`.
impl<'a> Parse<'a> for PseudoPage<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let colon_span = match &input.cursor.peek()?.token {
            Token::ColonColon(..) => input.cursor.expect_colon_colon()?.1,
            _ => input.cursor.expect_colon()?.1,
        };
        let name = input.parse::<InterpolableIdent>()?;

        let name_span = name.span();
        util::assert_no_ws_or_comment(&colon_span, name_span)?;
        let mut end = name_span.end;

        let arg = match input.cursor.peek()? {
            TokenWithSpan { token: Token::LParen(..), span } if span.start == end => {
                input.cursor.bump()?;
                let tokens = input.parse_tokens_in_parens()?;
                end = input.cursor.expect_r_paren()?.1.end;
                Some(tokens)
            }
            _ => None,
        };

        let span = Span { start: colon_span.start, end };
        Ok(PseudoPage { name, arg, span })
    }
}
