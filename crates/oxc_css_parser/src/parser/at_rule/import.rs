use super::Parser;
use crate::{
    Parse, Syntax,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
};

// https://www.w3.org/TR/css-cascade-5/#at-import
//
// @import [ <url> | <string> ]
//         [ layer | layer( <layer-name> ) ]?
//         <import-conditions> ;
// <import-conditions> = [ supports( [ <supports-condition> | <declaration> ] ) ]?
//                       <media-query-list>?
impl<'a> Parse<'a> for ImportPrelude<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        // The indented syntax accepts an unquoted path (`@import other.css`),
        // but an ident glued to `(` is a function href
        // (`@import url("theme.css")`), which the `Url::parse` arm handles.
        let sass_unquoted_path = input.syntax == Syntax::Sass
            && matches!(input.cursor.peek()?, TokenWithSpan { token: Token::Ident(..), span }
                if input.source.as_bytes().get(span.end) != Some(&b'('));
        let href = match &input.cursor.peek()?.token {
            Token::Str(..) | Token::StrTemplate(..) => input.parse().map(ImportPreludeHref::Str)?,
            Token::Ident(..) if sass_unquoted_path => {
                let start = input.cursor.peek()?.span.start;
                let mut end = start;
                while matches!(
                    &input.cursor.peek()?.token,
                    Token::Ident(..) | Token::Dot(..) | Token::Minus(..) | Token::Solidus(..)
                ) && input.cursor.peek()?.span.start == end
                {
                    end = input.cursor.bump()?.span.end;
                }
                let raw = unsafe { input.source.get_unchecked(start..end) };
                let span = Span { start, end };
                ImportPreludeHref::Str(InterpolableStr::Literal(Str { value: raw, raw, span }))
            }
            _ => match input.try_parse(Url::parse) {
                Ok(url) => ImportPreludeHref::Url(url),
                // Sass only: the content of `url(...)` may be SassScript that
                // is not a parsable URL, e.g. `@import url($dir+"/path");`.
                // Mirrors the fallback in `parse_component_value_atom`.
                Err(error) if matches!(input.syntax, Syntax::Scss | Syntax::Sass) => {
                    let (function_name, function_name_span) = input.cursor.expect_ident()?;
                    let function_name = input.ident(function_name, function_name_span);
                    if !function_name.name.eq_ignore_ascii_case("url") {
                        return Err(error);
                    }
                    input
                        .parse_function(InterpolableIdent::Literal(function_name))
                        .map(ImportPreludeHref::Function)
                        .map_err(|_| error)?
                }
                Err(error) => return Err(error),
            },
        };
        let mut span = href.span().clone();

        let structured = input.try_parse(Self::parse_structured_tail);
        let (layer, supports, media, modifiers) = match structured {
            Ok((layer, supports, media)) => (layer, supports, media, None),
            // Reference compilers accept arbitrary import modifiers (idents,
            // unknown functions, media-ish parens, further comma-chained
            // imports); keep the whole tail as raw component values.
            Err(_) => {
                let start = input.cursor.peek()?.span.start;
                let values = input.parse_declaration_value_tokens(true)?;
                // A trailing comma has no import after it — reference
                // compilers reject that, so don't paper over it.
                if let Some(ComponentValue::TokenWithSpan(TokenWithSpan {
                    token: Token::Comma(..),
                    span,
                })) = values.last()
                {
                    return Err(Error { kind: ErrorKind::ExpectRule, span: span.clone() });
                }
                let end = values.last().map_or(start, |value| value.span().end);
                (None, None, None, Some(ComponentValues { values, span: Span { start, end } }))
            }
        };
        if let Some(layer) = &layer {
            span.end = layer.span().end;
        }
        if let Some(supports) = &supports {
            span.end = supports.span().end;
        }
        if let Some(media) = &media {
            span.end = media.span.end;
        }
        if let Some(modifiers) = &modifiers
            && modifiers.span.end > modifiers.span.start
        {
            span.end = modifiers.span.end;
        }

        Ok(ImportPrelude { href, layer, supports, media, modifiers, span })
    }
}

impl<'a> ImportPrelude<'a> {
    /// The standard post-URL grammar: optional `layer`/`layer(...)`, optional
    /// `supports(...)`, optional media query list — valid only if it accounts
    /// for everything up to the end of the statement.
    #[allow(clippy::type_complexity)]
    fn parse_structured_tail(
        input: &mut Parser<'a>,
    ) -> PResult<(
        Option<ImportPreludeLayer<'a>>,
        Option<ImportPreludeSupports<'a>>,
        Option<MediaQueryList<'a>>,
    )> {
        let layer = match &input.cursor.peek()?.token {
            Token::Ident(ident) if ident.name().eq_ignore_ascii_case("layer") => {
                let ident = input.parse::<Ident>()?;
                let layer = match input.cursor.peek()? {
                    TokenWithSpan { token: Token::LParen(..), span }
                        if span.start == ident.span.end =>
                    {
                        input.cursor.bump()?;
                        let layer_name = input.parse().map(ComponentValue::LayerName)?;
                        let args = input.vec1(layer_name);
                        let end = input.cursor.expect_r_paren()?.1.end;
                        let span = Span { start: ident.span.start, end };
                        ImportPreludeLayer::WithName(Function {
                            name: FunctionName::Ident(InterpolableIdent::Literal(ident)),
                            args,
                            span,
                        })
                    }
                    _ => ImportPreludeLayer::Empty(ident),
                };
                Some(layer)
            }
            _ => None,
        };

        let supports = input.try_parse(|parser| {
            // (kept as its own try so a non-`supports` ident rolls back)
            let (ident, span) = parser.cursor.expect_ident()?;
            if !ident.name().eq_ignore_ascii_case("supports") {
                return Err(Error { kind: ErrorKind::TryParseError, span });
            }

            parser.cursor.expect_l_paren_without_ws_or_comments()?;

            let kind = if let Ok(supports_condition) = parser.try_parse(SupportsCondition::parse) {
                ImportPreludeSupportsKind::SupportsCondition(supports_condition)
            } else {
                parser.parse().map(ImportPreludeSupportsKind::Declaration)?
            };
            let (_, Span { end, .. }) = parser.cursor.expect_r_paren()?;
            Ok(ImportPreludeSupports { kind, span: Span { start: span.start, end } })
        });
        // `}` ends the at-rule too, so an `@import` nested in a style rule needs no
        // trailing `;` (`a { @import "b.css" }`); it can't start a media query.
        let media = if at_import_prelude_end(&input.cursor.peek()?.token) {
            None
        } else {
            Some(input.parse::<MediaQueryList>()?)
        };

        // Anything left over means this tail isn't the standard grammar.
        if at_import_prelude_end(&input.cursor.peek()?.token) {
            Ok((layer, supports.ok(), media))
        } else {
            let span = input.cursor.peek()?.span.clone();
            Err(Error { kind: ErrorKind::TryParseError, span })
        }
    }
}

/// End of an `@import` prelude: the statement boundary tokens.
fn at_import_prelude_end(token: &Token) -> bool {
    matches!(
        token,
        Token::Semicolon(..)
            | Token::Eof(..)
            | Token::RBrace(..)
            | Token::Dedent(..)
            | Token::Linebreak(..)
    )
}
