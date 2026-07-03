use super::Parser;
use crate::{
    Parse, Syntax,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
};

// <media-and> = and <media-in-parens>
impl<'a> Parse<'a> for MediaAnd<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("and") {
            let media_in_parens = input.parse_media_in_parens_after_logic()?;
            let span = Span { start: keyword.span.start, end: media_in_parens.span.end };
            Ok(MediaAnd { keyword, media_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectMediaAnd, span: keyword.span })
        }
    }
}

// The `and`-led tail after a <media-type> (top-level `or` not allowed here):
// [ and <media-condition-without-or> ]
impl<'a> Parse<'a> for MediaConditionAfterMediaType<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let and: Ident = match input.cursor.bump()? {
            TokenWithSpan { token: Token::Ident(ident), span }
                if ident.name().eq_ignore_ascii_case("and") =>
            {
                input.ident(ident, span)
            }
            TokenWithSpan { span, .. } => {
                return Err(Error { kind: ErrorKind::ExpectMediaAnd, span });
            }
        };

        let condition = input.parse_media_condition(
            /* allow_or */ false, /* after_logic_keyword */ true,
        )?;

        let span = Span { start: and.span.start, end: condition.span.end };
        Ok(MediaConditionAfterMediaType { and, condition, span })
    }
}

// https://www.w3.org/TR/mediaqueries-4/#mq-features
//
// <media-feature> = ( [ <mf-plain> | <mf-boolean> | <mf-range> ] )
// <mf-plain>   = <mf-name> : <mf-value>
// <mf-boolean> = <mf-name>
impl<'a> Parse<'a> for MediaFeature<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        match input.parse_media_feature_value()? {
            ComponentValue::InterpolableIdent(ident) => match &input.cursor.peek()?.token {
                Token::Colon(..) => input.parse_media_feature_plain(ident).map(MediaFeature::Plain),
                Token::LessThan(..)
                | Token::LessThanEqual(..)
                | Token::GreaterThan(..)
                | Token::GreaterThanEqual(..)
                | Token::Equal(..) => input.parse_media_feature_range_or_range_interval(
                    ComponentValue::InterpolableIdent(ident),
                ),
                _ => {
                    let span = ident.span().clone();
                    Ok(MediaFeature::Boolean(MediaFeatureBoolean {
                        name: MediaFeatureName::Ident(ident),
                        span,
                    }))
                }
            },
            ComponentValue::SassVariable(variable) => {
                let span = variable.span.clone();
                Ok(MediaFeature::Boolean(MediaFeatureBoolean {
                    name: MediaFeatureName::SassVariable(variable),
                    span,
                }))
            }
            ComponentValue::PostcssSimpleVar(variable) => {
                let span = variable.span.clone();
                Ok(MediaFeature::Boolean(MediaFeatureBoolean {
                    name: MediaFeatureName::PostcssSimpleVar(variable),
                    span,
                }))
            }
            value => input.parse_media_feature_range_or_range_interval(value),
        }
    }
}

// <mf-comparison> = '<' | '>' | '<=' | '>=' | '='
impl<'a> Parse<'a> for MediaFeatureComparison {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        match input.cursor.bump()? {
            TokenWithSpan { token: Token::LessThan(..), span } => {
                Ok(MediaFeatureComparison { kind: MediaFeatureComparisonKind::LessThan, span })
            }
            TokenWithSpan { token: Token::LessThanEqual(..), span } => Ok(MediaFeatureComparison {
                kind: MediaFeatureComparisonKind::LessThanOrEqual,
                span,
            }),
            TokenWithSpan { token: Token::GreaterThan(..), span } => {
                Ok(MediaFeatureComparison { kind: MediaFeatureComparisonKind::GreaterThan, span })
            }
            TokenWithSpan { token: Token::GreaterThanEqual(..), span } => {
                Ok(MediaFeatureComparison {
                    kind: MediaFeatureComparisonKind::GreaterThanOrEqual,
                    span,
                })
            }
            TokenWithSpan { token: Token::Equal(..), span } => {
                Ok(MediaFeatureComparison { kind: MediaFeatureComparisonKind::Equal, span })
            }
            TokenWithSpan { span, .. } => {
                Err(Error { kind: ErrorKind::ExpectMediaFeatureComparison, span })
            }
        }
    }
}

// <media-in-parens> = ( <media-condition> ) | <media-feature> | <general-enclosed>
impl<'a> Parse<'a> for MediaInParens<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        // Sass allows an interpolation wherever `<media-in-parens>` is expected,
        // e.g. `@media screen and #{$query} {}`.
        if matches!(input.syntax, Syntax::Scss | Syntax::Sass)
            && matches!(&input.cursor.peek()?.token, Token::HashLBrace(..))
            && let InterpolableIdent::SassInterpolated(interpolation) =
                input.parse_sass_interpolated_ident()?
        {
            let span = interpolation.span.clone();
            return Ok(MediaInParens {
                kind: MediaInParensKind::SassInterpolation(interpolation),
                span,
            });
        }
        let (_, Span { start, .. }) = input.cursor.expect_l_paren()?;
        let kind = input.parse()?;
        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
        Ok(MediaInParens { kind, span: Span { start, end } })
    }
}

// The contents inside the parens: ( <media-condition> ) | <media-feature> | <general-enclosed>
impl<'a> Parse<'a> for MediaInParensKind<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if let Ok(media_condition) = input.try_parse(|parser| {
            let media_condition = parser.parse_media_condition(
                /* allow_or */ true, /* after_logic_keyword */ false,
            )?;
            // `(#{$x}-width: 1px)`: the interpolation parses as a
            // `SassInterpolation` media condition, but a trailing `:` means it is
            // really a media feature name. Require the closing `)` here so such
            // cases fall through to the media-feature branch below.
            if matches!(&parser.cursor.peek()?.token, Token::RParen(..)) {
                Ok(media_condition)
            } else {
                let span = parser.cursor.peek()?.span.clone();
                Err(Error { kind: ErrorKind::ExpectMediaFeatureName, span })
            }
        }) {
            Ok(MediaInParensKind::MediaCondition(media_condition))
        } else if let Ok(media_feature) = input.try_parse(|parser| {
            let media_feature = parser.parse::<MediaFeature>()?;
            if matches!(&parser.cursor.peek()?.token, Token::RParen(..)) {
                Ok(media_feature)
            } else {
                let span = parser.cursor.peek()?.span.clone();
                Err(Error { kind: ErrorKind::ExpectMediaFeatureName, span })
            }
        }) {
            Ok(MediaInParensKind::MediaFeature(input.alloc(media_feature)))
        } else {
            // <general-enclosed>: MQ L4 forward-compat catch-all, evaluates false at runtime.
            let tokens = input.parse_tokens_in_parens()?;
            Ok(MediaInParensKind::GeneralEnclosed(tokens))
        }
    }
}

// <media-not> = not <media-in-parens>
impl<'a> Parse<'a> for MediaNot<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("not") {
            let media_in_parens = input.parse::<MediaInParens>()?;
            let span = Span { start: keyword.span.start, end: media_in_parens.span.end };
            Ok(MediaNot { keyword, media_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectMediaNot, span: keyword.span })
        }
    }
}

// <media-or> = or <media-in-parens>
impl<'a> Parse<'a> for MediaOr<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let keyword = input.parse::<Ident>()?;
        if keyword.name.eq_ignore_ascii_case("or") {
            let media_in_parens = input.parse_media_in_parens_after_logic()?;
            let span = Span { start: keyword.span.start, end: media_in_parens.span.end };
            Ok(MediaOr { keyword, media_in_parens, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectMediaOr, span: keyword.span })
        }
    }
}

// https://www.w3.org/TR/mediaqueries-4/#mq-syntax
//
// <media-query> = <media-condition>
//               | [ not | only ]? <media-type> [ and <media-condition-without-or> ]?
impl<'a> Parse<'a> for MediaQuery<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        if let Ok(condition_only) = input.try_parse(|parser| {
            parser.parse_media_condition(
                /* allow_or */ true, /* after_logic_keyword */ false,
            )
        }) {
            Ok(MediaQuery::ConditionOnly(condition_only))
        } else if input.syntax == Syntax::Less {
            match input.cursor.peek()?.token {
                Token::AtKeyword(..) => {
                    input.parse_less_maybe_variable_or_with_lookups().map(|value| match value {
                        ComponentValue::LessVariable(variable) => {
                            MediaQuery::LessVariable(variable)
                        }
                        ComponentValue::LessNamespaceValue(namespace_value) => {
                            MediaQuery::LessNamespaceValue(namespace_value)
                        }
                        _ => unreachable!(),
                    })
                }
                Token::Dot(..) | Token::Hash(..) => {
                    let less_namespace_value = input.parse()?;
                    Ok(MediaQuery::LessNamespaceValue(input.alloc(less_namespace_value)))
                }
                _ => input.parse_media_query_with_type_or_function(),
            }
        } else {
            input.parse_media_query_with_type_or_function()
        }
    }
}

// https://www.w3.org/TR/mediaqueries-4/#mq-syntax
//
// <media-query-list> = <media-query>#
impl<'a> Parse<'a> for MediaQueryList<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let first = input.parse::<MediaQuery>()?;
        let mut span = first.span().clone();

        let mut queries = input.vec1(first);
        let mut comma_spans = input.vec();
        while let Some((_, comma_span)) = input.cursor.eat_comma()? {
            comma_spans.push(comma_span);
            queries.push(input.parse()?);
        }
        debug_assert_eq!(comma_spans.len() + 1, queries.len());

        // SAFETY: it has at least one element.
        span.end = unsafe {
            let index = queries.len() - 1;
            queries.get_unchecked(index).span().end
        };
        Ok(MediaQueryList { queries, comma_spans, span })
    }
}

// [ not | only ]? <media-type> [ and <media-condition-without-or> ]?
// <media-type> = <ident>   (not `only` / `not` / `and` / `or` / `layer`)
impl<'a> Parse<'a> for MediaQueryWithType<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let modifier = if let Token::Ident(ident) = &input.cursor.peek()?.token {
            let name = ident.name();
            if name.eq_ignore_ascii_case("not") || name.eq_ignore_ascii_case("only") {
                Some(input.parse::<Ident>()?)
            } else {
                None
            }
        } else {
            None
        };
        let media_type = input.parse::<InterpolableIdent>()?;
        if let InterpolableIdent::Literal(Ident { name, span, .. }) = &media_type
            && (name.eq_ignore_ascii_case("only")
                || name.eq_ignore_ascii_case("not")
                || name.eq_ignore_ascii_case("and")
                || name.eq_ignore_ascii_case("or")
                || name.eq_ignore_ascii_case("layer"))
        {
            input.recoverable_errors.push(Error {
                kind: ErrorKind::MediaTypeKeywordDisallowed(name.to_string()),
                span: span.clone(),
            });
        }
        let condition = match &input.cursor.peek()?.token {
            Token::Ident(ident) if ident.name().eq_ignore_ascii_case("and") => {
                input.parse::<MediaConditionAfterMediaType>().map(Some)?
            }
            _ => None,
        };

        let mut span = media_type.span().clone();
        if let Some(modifier) = &modifier {
            span.start = modifier.span.start;
        }
        if let Some(condition) = &condition {
            span.end = condition.span.end;
        }
        Ok(MediaQueryWithType { modifier, media_type, condition, span })
    }
}

impl<'a> Parser<'a> {
    /// `<media-in-parens>` right after `and`/`or`. A bare ident there
    /// (`@media screen and print`) is not valid MQ syntax, but browsers still
    /// parse the rule — the query merely evaluates to "not all" — so preserve
    /// it like `<general-enclosed>` raw tokens. Only this position is safe to
    /// relax: elsewhere an ident is a media type (`@media print`).
    fn parse_media_in_parens_after_logic(&mut self) -> PResult<MediaInParens<'a>> {
        if self.syntax == Syntax::Css
            && let TokenWithSpan { token: Token::Ident(ident), span } = self.cursor.peek()?
            && !ident.name().eq_ignore_ascii_case("not")
            && self.source.as_bytes().get(span.end) != Some(&b'(')
        {
            let token = self.cursor.bump()?;
            let span = token.span.clone();
            return Ok(MediaInParens {
                kind: MediaInParensKind::GeneralEnclosed(TokenSeq {
                    tokens: self.vec1(token),
                    span: span.clone(),
                }),
                span,
            });
        }
        self.parse()
    }

    // <media-condition>            = <media-not> | <media-in-parens> [ <media-and>* | <media-or>* ]
    // <media-condition-without-or> = <media-not> | <media-in-parens> <media-and>*   (allow_or = false)
    fn parse_media_condition(
        &mut self,
        allow_or: bool,
        after_logic_keyword: bool,
    ) -> PResult<MediaCondition<'a>> {
        match &self.cursor.peek()?.token {
            Token::Ident(ident) if ident.name().eq_ignore_ascii_case("not") => {
                let media_not = self.parse::<MediaNot>()?;
                let span = media_not.span.clone();
                Ok(MediaCondition {
                    conditions: self.vec1(MediaConditionKind::Not(media_not)),
                    span,
                })
            }
            _ => {
                let first = if after_logic_keyword {
                    self.parse_media_in_parens_after_logic()?
                } else {
                    self.parse::<MediaInParens>()?
                };
                let mut span = first.span.clone();
                let mut conditions = self.vec1(MediaConditionKind::MediaInParens(first));
                if let Token::Ident(ident) = &self.cursor.peek()?.token {
                    let name = ident.name();
                    if name.eq_ignore_ascii_case("and") {
                        loop {
                            conditions.push(MediaConditionKind::And(self.parse()?));
                            match &self.cursor.peek()?.token {
                                Token::Ident(ident) if ident.name().eq_ignore_ascii_case("and") => {
                                }
                                _ => break,
                            }
                        }
                    } else if allow_or && name.eq_ignore_ascii_case("or") {
                        loop {
                            conditions.push(MediaConditionKind::Or(self.parse()?));
                            match &self.cursor.peek()?.token {
                                Token::Ident(ident) if ident.name().eq_ignore_ascii_case("or") => {}
                                _ => break,
                            }
                        }
                    }
                }

                if let Some(last) = conditions.last() {
                    span.end = last.span().end;
                }
                Ok(MediaCondition { conditions, span })
            }
        }
    }

    // <mf-plain> = <mf-name> : <mf-value>
    fn parse_media_feature_plain(
        &mut self,
        ident: InterpolableIdent<'a>,
    ) -> PResult<MediaFeaturePlain<'a>> {
        let (_, colon_span) = self.cursor.expect_colon()?;
        let value = self.parse_media_feature_value()?;
        let span = Span { start: ident.span().start, end: value.span().end };
        Ok(MediaFeaturePlain { name: MediaFeatureName::Ident(ident), colon_span, value, span })
    }

    // <mf-range> = <mf-name>  <mf-comparison> <mf-value>                             (range)
    //            | <mf-value> <mf-comparison> <mf-name>                             (range)
    //            | <mf-value> <mf-comparison> <mf-name> <mf-comparison> <mf-value>  (interval)
    fn parse_media_feature_range_or_range_interval(
        &mut self,
        left: ComponentValue<'a>,
    ) -> PResult<MediaFeature<'a>> {
        let comparison = self.parse()?;
        let name_or_right = self.parse_media_feature_value()?;
        if let ComponentValue::InterpolableIdent(ident) = name_or_right {
            match &self.cursor.peek()?.token {
                Token::LessThan(..)
                | Token::LessThanEqual(..)
                | Token::GreaterThan(..)
                | Token::GreaterThanEqual(..)
                | Token::Equal(..) => {
                    let right_comparison = self.parse()?;
                    let right = self.parse_media_feature_value()?;
                    let span = Span { start: left.span().start, end: right.span().end };
                    Ok(MediaFeature::RangeInterval(MediaFeatureRangeInterval {
                        left,
                        left_comparison: comparison,
                        name: MediaFeatureName::Ident(ident),
                        right_comparison,
                        right,
                        span,
                    }))
                }
                _ => {
                    let span = Span { start: left.span().start, end: ident.span().end };
                    Ok(MediaFeature::Range(MediaFeatureRange {
                        left,
                        comparison,
                        right: ComponentValue::InterpolableIdent(ident),
                        span,
                    }))
                }
            }
        } else {
            if !matches!(left, ComponentValue::InterpolableIdent(..))
                && !matches!(name_or_right, ComponentValue::InterpolableIdent(..))
            {
                self.recoverable_errors.push(Error {
                    kind: ErrorKind::ExpectMediaFeatureName,
                    span: name_or_right.span().clone(),
                });
            }
            let span = Span { start: left.span().start, end: name_or_right.span().end };
            Ok(MediaFeature::Range(MediaFeatureRange {
                left,
                comparison,
                right: name_or_right,
                span,
            }))
        }
    }

    // <mf-value> = <number> | <dimension> | <ident> | <ratio>
    fn parse_media_feature_value(&mut self) -> PResult<ComponentValue<'a>> {
        let value = match self.syntax {
            Syntax::Css => self.parse_component_value_atom()?,
            Syntax::Scss | Syntax::Sass => {
                self.parse_sass_bin_expr(/* allow_comparison */ false)?
            }
            Syntax::Less => self.parse_less_operation(/* allow_mixin_call */ true)?,
        };
        match value {
            ComponentValue::Number(number)
                if number.value >= 0.0
                    && matches!(self.cursor.peek()?.token, Token::Solidus(..)) =>
            {
                self.parse_ratio(number).map(ComponentValue::Ratio)
            }
            value => Ok(value),
        }
    }

    // The `[ not | only ]? <media-type> …` branch of <media-query>; a media type
    // glued to `(` is also accepted as a function form (`screen(...)`, used by Less).
    fn parse_media_query_with_type_or_function(&mut self) -> PResult<MediaQuery<'a>> {
        let media_query_with_type = self.parse::<MediaQueryWithType>()?;
        match (media_query_with_type, self.cursor.peek()?) {
            (
                MediaQueryWithType {
                    modifier: None,
                    media_type: name,
                    condition: None,
                    span: mq_span,
                },
                TokenWithSpan { token: crate::token::Token::LParen(..), span: lparen_span },
            ) if mq_span.end == lparen_span.start => {
                self.cursor.bump()?;
                let args = self.parse_function_args()?;
                let (_, Span { end, .. }) = self.cursor.expect_r_paren()?;
                Ok(MediaQuery::Function(Function {
                    name: FunctionName::Ident(name),
                    args,
                    span: Span { start: mq_span.start, end },
                }))
            }
            (media_query_with_type, _) => Ok(MediaQuery::WithType(media_query_with_type)),
        }
    }
}
