use super::Parser;
use crate::{Parse, Spanned, ast::*, error::PResult, tokenizer::Token};

// https://drafts.csswg.org/css-fonts/Overview.bs
impl<'a> Parse<'a> for FontFamilyName<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        match &input.cursor.peek()?.token {
            Token::Str(..) | Token::StrTemplate(..) => input.parse().map(FontFamilyName::Str),
            _ => {
                let first = input.parse::<InterpolableIdent>()?;
                let mut span = first.span().clone();

                let mut idents = input.vec1(first);
                while let Token::Ident(..) | Token::HashLBrace(..) | Token::AtLBraceVar(..) =
                    &input.cursor.peek()?.token
                {
                    idents.push(input.parse()?);
                }
                if let Some(last) = idents.last() {
                    span.end = last.span().end;
                }
                Ok(FontFamilyName::Unquoted(UnquotedFontFamilyName { idents, span }))
            }
        }
    }
}
