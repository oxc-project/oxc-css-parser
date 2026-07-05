use super::Parser;
use crate::{
    Parse,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
};

// https://drafts.csswg.org/css-conditional-3/#at-supports
//
// <supports-condition> = not <supports-in-parens>
//                      | <supports-in-parens> [ and <supports-in-parens> ]*
//                      | <supports-in-parens> [ or <supports-in-parens> ]*
impl<'a> Parse<'a> for SupportsCondition<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if input.cursor.peek()?.is_ident_name_eq_ignore_ascii_case(input.source, "not") {
            let keyword = input.parse::<Ident>()?;
            let condition = input.parse::<SupportsInParens>()?;
            let span = Span { start: keyword.span.start, end: condition.span().end };
            Ok(SupportsCondition {
                conditions: input.vec1(SupportsConditionKind::Not(SupportsNot {
                    keyword,
                    condition,
                    span: span.clone(),
                })),
                span,
            })
        } else {
            let first = input.parse::<SupportsInParens>()?;
            let mut span = first.span().clone();
            let mut conditions = input.vec1(SupportsConditionKind::SupportsInParens(first));
            while input.cursor.peek()?.ident(input.source).is_some() {
                if input.cursor.peek()?.is_ident_name_eq_ignore_ascii_case(input.source, "and") {
                    let ident = input.parse::<Ident>()?;
                    let condition = input.parse::<SupportsInParens>()?;
                    let span = Span { start: ident.span.start, end: condition.span().end };
                    conditions.push(SupportsConditionKind::And(SupportsAnd {
                        keyword: ident,
                        condition,
                        span,
                    }));
                } else if input
                    .cursor
                    .peek()?
                    .is_ident_name_eq_ignore_ascii_case(input.source, "or")
                {
                    let ident = input.parse::<Ident>()?;
                    let condition = input.parse::<SupportsInParens>()?;
                    let span = Span { start: ident.span.start, end: condition.span().end };
                    conditions.push(SupportsConditionKind::Or(SupportsOr {
                        keyword: ident,
                        condition,
                        span,
                    }));
                } else {
                    break;
                }
            }
            if let Some(last) = conditions.last() {
                span.end = last.span().end;
            }
            Ok(SupportsCondition { conditions, span })
        }
    }
}

// https://drafts.csswg.org/css-conditional-3/#typedef-supports-in-parens
//
// <supports-in-parens> = ( <supports-condition> )
//                      | <supports-feature>
//                      | <general-enclosed>
impl<'a> Parse<'a> for SupportsInParens<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        match input.cursor.peek()? {
            TokenWithSpan { token: Token::LParen(..), .. } => input
                .try_parse(|parser| {
                    parser.parse::<SupportsDecl>().map(|supports_decl| {
                        let span = supports_decl.span.clone();
                        SupportsInParens {
                            kind: SupportsInParensKind::Feature(parser.alloc(supports_decl)),
                            span,
                        }
                    })
                })
                .or_else(|_| {
                    input.try_parse(|parser| {
                        let (_, Span { start, .. }) = parser.cursor.expect_l_paren()?;
                        let condition = parser.parse::<SupportsCondition>()?;
                        let (_, Span { end, .. }) = parser.cursor.expect_r_paren()?;
                        Ok(SupportsInParens {
                            kind: SupportsInParensKind::SupportsCondition(condition),
                            span: Span { start, end },
                        })
                    })
                })
                .or_else(|_| {
                    // <general-enclosed>: MQ L4 catch-all (referenced from <supports-condition>),
                    // evaluates false at runtime.
                    let (_, Span { start, .. }) = input.cursor.expect_l_paren()?;
                    let tokens = input.parse_tokens_in_parens()?;
                    let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
                    Ok(SupportsInParens {
                        kind: SupportsInParensKind::GeneralEnclosed(tokens),
                        span: Span { start, end },
                    })
                }),
            // Sass: an interpolation may stand for a whole condition operand
            // (`@supports #{"(a: b)"} and (c: d)`) or splice into a function
            // name (`@supports a#{"b"}c(d)`).
            TokenWithSpan { token: Token::Ident(..) | Token::HashLBrace(..), .. } => {
                let name = input.parse::<InterpolableIdent>()?;
                let name_end = name.span().end;
                match name {
                    InterpolableIdent::Literal(function_ident)
                        if function_ident.name.eq_ignore_ascii_case("selector") =>
                    {
                        input.cursor.expect_l_paren()?;
                        let selector_list = input.parse::<SelectorList>()?;
                        input.cursor.expect_r_paren()?;
                        let span = selector_list.span.clone();
                        Ok(SupportsInParens {
                            kind: SupportsInParensKind::Selector(selector_list),
                            span,
                        })
                    }
                    name => {
                        let glued_lparen = matches!(
                            input.cursor.peek()?,
                            TokenWithSpan { token: Token::LParen(..), span }
                                if span.start == name_end
                        );
                        // Only a pure interpolation may stand alone
                        // (`#{"(a: b)"}`); a mixed ident like `a#{b}` still
                        // needs parens or a function call, as in dart-sass.
                        let pure_interpolation = matches!(
                            &name,
                            InterpolableIdent::SassInterpolated(interpolation)
                                if matches!(
                                    interpolation.elements.as_slice(),
                                    [SassInterpolatedIdentElement::Expression(..)]
                                )
                        );
                        if glued_lparen {
                            // An unknown function here is `<general-enclosed>`
                            // (css-conditional): its contents are raw tokens.
                            let function = input.parse_raw_function(name)?;
                            let span = function.span.clone();
                            Ok(SupportsInParens {
                                kind: SupportsInParensKind::Function(function),
                                span,
                            })
                        } else if pure_interpolation {
                            let span = name.span().clone();
                            Ok(SupportsInParens {
                                kind: SupportsInParensKind::Interpolation(name),
                                span,
                            })
                        } else {
                            let TokenWithSpan { token, span } = input.cursor.peek()?;
                            Err(Error {
                                kind: ErrorKind::Unexpected("'('", token.symbol()),
                                span: span.clone(),
                            })
                        }
                    }
                }
            }
            TokenWithSpan { token, span } => Err(Error {
                kind: ErrorKind::Unexpected("'('", token.symbol()),
                span: span.clone(),
            }),
        }
    }
}

// https://drafts.csswg.org/css-conditional-3/#typedef-supports-feature
//
// <supports-feature> = <supports-decl>
// <supports-decl>    = ( <declaration> )
impl<'a> Parse<'a> for SupportsDecl<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let start = input.cursor.expect_l_paren()?.1.start;
        let decl = input.parse()?;
        let end = input.cursor.expect_r_paren()?.1.end;
        Ok(SupportsDecl { decl, span: Span { start, end } })
    }
}
