use super::Parser;
use crate::{Parse, ast::*, error::PResult, tokenizer::Token};

// https://www.w3.org/TR/css-namespaces-3/#syntax
//
// @namespace <namespace-prefix>? [ <string> | <url> ] ;
// <namespace-prefix> = <ident>
impl<'a> Parse<'a> for NamespacePrelude<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let prefix = match input.cursor.peek()? {
            token if token.ident(input.source).is_some() => {
                if token.is_ident_name_eq_ignore_ascii_case(input.source, "url") {
                    None
                } else {
                    Some(InterpolableIdent::Literal(input.parse::<Ident>()?))
                }
            }
            token if matches!(token.token, Token::HashLBrace(..) | Token::AtLBraceVar(..)) => {
                input.parse::<InterpolableIdent>().map(Some)?
            }
            _ => None,
        };
        let uri = match &input.cursor.peek()?.token {
            Token::Str(..) | Token::StrTemplate(..) => {
                input.parse().map(NamespacePreludeUri::Str)?
            }
            _ => input.parse().map(NamespacePreludeUri::Url)?,
        };

        let mut span = *uri.span();
        if let Some(prefix) = &prefix {
            span.start = prefix.span().start;
        }
        Ok(NamespacePrelude { prefix, uri, span })
    }
}
