use super::Parser;
use crate::{
    Parse,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
};

// https://drafts.csswg.org/css-conditional-5/#container-queries
//
// Spec `<container-query>` — the boolean logic over `<query-in-parens>`
// (this AST names the node `ContainerCondition`):
// <container-query> = not <query-in-parens>
//                   | <query-in-parens> [ [ and <query-in-parens> ]* | [ or <query-in-parens> ]* ]
impl<'a> Parse<'a> for ContainerCondition<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if input.cursor.peek()?.is_ident_name_eq_ignore_ascii_case(input.source, "not") {
            let container_condition_not = input.parse::<ContainerConditionNot>()?;
            let span = container_condition_not.span.clone();
            Ok(ContainerCondition {
                conditions: input.vec1(ContainerConditionKind::Not(container_condition_not)),
                span,
            })
        } else {
            let first = input.parse::<QueryInParens>()?;
            let mut span = first.span.clone();
            let mut conditions = input.vec1(ContainerConditionKind::QueryInParens(first));
            // formally `and`/`or` may not mix without parens and `not`
            // is leading-only, but real-world code (less.js) chains them
            // freely: `(a) or (b) and (c)`, `(a) not (b)`.
            loop {
                let peek = input.cursor.peek()?;
                let kind = if peek.is_ident_name_eq_ignore_ascii_case(input.source, "and") {
                    ContainerConditionKind::And(input.parse()?)
                } else if peek.is_ident_name_eq_ignore_ascii_case(input.source, "or") {
                    ContainerConditionKind::Or(input.parse()?)
                } else if peek.is_ident_name_eq_ignore_ascii_case(input.source, "not") {
                    ContainerConditionKind::Not(input.parse()?)
                } else {
                    break;
                };
                conditions.push(kind);
            }

            if let Some(last) = conditions.last() {
                span.end = last.span().end;
            }
            Ok(ContainerCondition { conditions, span })
        }
    }
}

// and <query-in-parens>
impl<'a> Parse<'a> for ContainerConditionAnd<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("and") {
            let query_in_parens = input.parse::<QueryInParens>()?;
            let span = Span { start: keyword.span.start, end: query_in_parens.span.end };
            Ok(ContainerConditionAnd { keyword, query_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectContainerConditionAnd, span: keyword.span })
        }
    }
}

// not <query-in-parens>
impl<'a> Parse<'a> for ContainerConditionNot<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("not") {
            let query_in_parens = input.parse::<QueryInParens>()?;
            let span = Span { start: keyword.span.start, end: query_in_parens.span.end };
            Ok(ContainerConditionNot { keyword, query_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectContainerConditionNot, span: keyword.span })
        }
    }
}

// or <query-in-parens>
impl<'a> Parse<'a> for ContainerConditionOr<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("or") {
            let query_in_parens = input.parse::<QueryInParens>()?;
            let span = Span { start: keyword.span.start, end: query_in_parens.span.end };
            Ok(ContainerConditionOr { keyword, query_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectContainerConditionOr, span: keyword.span })
        }
    }
}

// <query-in-parens> = ( <container-query> )
//                   | ( <size-feature> )
//                   | style( <style-query> )
//                   | scroll-state( <scroll-state-query> )
//                   | <general-enclosed>
impl<'a> Parse<'a> for QueryInParens<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if let Some((_, Span { start, .. })) = input.cursor.eat_l_paren()? {
            let kind = if let Ok(container_condition) = input.try_parse(ContainerCondition::parse) {
                QueryInParensKind::ContainerCondition(container_condition)
            } else {
                let size_feature = input.parse()?;
                QueryInParensKind::SizeFeature(input.alloc(size_feature))
            };
            let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
            Ok(QueryInParens { kind, span: Span { start, end } })
        } else {
            let (style_keyword, ident_span) = input.cursor.expect_ident()?;
            let keyword = style_keyword.name();
            if keyword.eq_ignore_ascii_case("style") {
                input.cursor.expect_l_paren_without_ws_or_comments()?;
                let kind = input.parse().map(QueryInParensKind::StyleQuery)?;
                let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
                Ok(QueryInParens { kind, span: Span { start: ident_span.start, end } })
            } else if keyword.eq_ignore_ascii_case("scroll-state") {
                // https://drafts.csswg.org/css-conditional-5/#scroll-state-container
                input.cursor.expect_l_paren_without_ws_or_comments()?;
                let media = input.parse()?;
                let kind = QueryInParensKind::ScrollState(input.alloc(media));
                let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
                Ok(QueryInParens { kind, span: Span { start: ident_span.start, end } })
            } else {
                Err(Error { kind: ErrorKind::ExpectStyleQuery, span: ident_span })
            }
        }
    }
}

// https://drafts.csswg.org/css-contain-3/#typedef-style-query
//
// <style-condition> = not <style-in-parens>
//                   | <style-in-parens> [ [ and <style-in-parens> ]* | [ or <style-in-parens> ]* ]
impl<'a> Parse<'a> for StyleCondition<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if input.cursor.peek()?.is_ident_name_eq_ignore_ascii_case(input.source, "not") {
            let style_condition_not = input.parse::<StyleConditionNot>()?;
            let span = style_condition_not.span.clone();
            Ok(StyleCondition {
                conditions: input.vec1(StyleConditionKind::Not(style_condition_not)),
                span,
            })
        } else {
            let first = input.parse::<StyleInParens>()?;
            let mut span = first.span.clone();
            let mut conditions = input.vec1(StyleConditionKind::StyleInParens(first));
            let peek = input.cursor.peek()?;
            if peek.ident(input.source).is_some() {
                if peek.is_ident_name_eq_ignore_ascii_case(input.source, "and") {
                    loop {
                        conditions.push(StyleConditionKind::And(input.parse()?));
                        if !input
                            .cursor
                            .peek()?
                            .is_ident_name_eq_ignore_ascii_case(input.source, "and")
                        {
                            break;
                        }
                    }
                } else if peek.is_ident_name_eq_ignore_ascii_case(input.source, "or") {
                    loop {
                        conditions.push(StyleConditionKind::Or(input.parse()?));
                        if !input
                            .cursor
                            .peek()?
                            .is_ident_name_eq_ignore_ascii_case(input.source, "or")
                        {
                            break;
                        }
                    }
                }
            }

            if let Some(last) = conditions.last() {
                span.end = last.span().end;
            }
            Ok(StyleCondition { conditions, span })
        }
    }
}

// and <style-in-parens>
impl<'a> Parse<'a> for StyleConditionAnd<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let ident = input.parse::<Ident>()?;
        if ident.name.eq_ignore_ascii_case("and") {
            let style_in_parens = input.parse::<StyleInParens>()?;
            let span = Span { start: ident.span.start, end: style_in_parens.span.end };
            Ok(StyleConditionAnd { keyword: ident, style_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectStyleConditionAnd, span: ident.span })
        }
    }
}

// not <style-in-parens>
impl<'a> Parse<'a> for StyleConditionNot<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("not") {
            let style_in_parens = input.parse::<StyleInParens>()?;
            let span = Span { start: keyword.span.start, end: style_in_parens.span.end };
            Ok(StyleConditionNot { keyword, style_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectStyleConditionNot, span: keyword.span })
        }
    }
}

// or <style-in-parens>
impl<'a> Parse<'a> for StyleConditionOr<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("or") {
            let style_in_parens = input.parse::<StyleInParens>()?;
            let span = Span { start: keyword.span.start, end: style_in_parens.span.end };
            Ok(StyleConditionOr { keyword, style_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectStyleConditionOr, span: keyword.span })
        }
    }
}

// <style-in-parens> = ( <style-condition> ) | ( <style-feature> ) | <general-enclosed>
impl<'a> Parse<'a> for StyleInParens<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (_, Span { start, .. }) = input.cursor.expect_l_paren()?;
        let kind = input.parse()?;
        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
        Ok(StyleInParens { kind, span: Span { start, end } })
    }
}

// <style-condition> | <style-feature>
impl<'a> Parse<'a> for StyleInParensKind<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if let Ok(style_condition) = input.try_parse(StyleCondition::parse) {
            Ok(StyleInParensKind::Condition(style_condition))
        } else {
            input.parse().map(StyleInParensKind::Feature)
        }
    }
}

// <style-query> = <style-condition> | <style-feature>
// (a bare custom-property name, e.g. `style(--theme)`, is a boolean-context <style-feature>)
impl<'a> Parse<'a> for StyleQuery<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if let Ok(condition) = input.try_parse(StyleCondition::parse) {
            Ok(StyleQuery::Condition(condition))
        } else if let Ok(name) = input.try_parse(|p| {
            // a bare custom-property existence test: `style(--theme)`
            let name = p.parse::<InterpolableIdent>()?;
            match (&name, &p.cursor.peek()?.token) {
                (InterpolableIdent::Literal(ident), Token::RParen(..))
                    if ident.name.starts_with("--") =>
                {
                    Ok(name)
                }
                _ => {
                    let span = p.cursor.peek()?.span.clone();
                    Err(Error { kind: ErrorKind::TryParseError, span })
                }
            }
        }) {
            Ok(StyleQuery::FeatureName(name))
        } else {
            let feature = input.parse().map(StyleQuery::Feature);
            input.cursor.eat_semicolon()?;
            feature
        }
    }
}

// https://drafts.csswg.org/css-contain-3/#container-rule
//
// @container <container-name>? <container-query> { <block-contents> }
// <container-name> = <custom-ident>
impl<'a> Parse<'a> for ContainerPrelude<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let name = input.try_parse(|parser| match parser.parse()? {
            InterpolableIdent::Literal(ident)
                if ident.name.eq_ignore_ascii_case("not")
                    || ident.name.eq_ignore_ascii_case("scroll-state") =>
            {
                Err(Error { kind: ErrorKind::TryParseError, span: ident.span })
            }
            InterpolableIdent::Literal(ident) if ident.name.eq_ignore_ascii_case("style") => {
                match parser.cursor.peek()? {
                    TokenWithSpan { token: Token::LParen(..), span }
                        if span.start == ident.span.end =>
                    {
                        Err(Error { kind: ErrorKind::TryParseError, span: ident.span })
                    }
                    _ => Ok(InterpolableIdent::Literal(ident)),
                }
            }
            ident => Ok(ident),
        });
        let condition = input.parse::<ContainerCondition>()?;
        let mut span = condition.span().clone();
        if let Ok(name) = &name {
            span.start = name.span().start;
        }
        Ok(ContainerPrelude { name: name.ok(), condition, span })
    }
}
