use super::{Parser, state::QualifiedRuleContext};
use crate::{
    Parse, Syntax,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
    util,
};

const PRECEDENCE_MULTIPLY: u8 = 2;
const PRECEDENCE_PLUS: u8 = 1;

/// Strip one leading `-vendor-` prefix (`-moz-calc` -> `calc`); returns the
/// name unchanged when there is none.
fn unvendored(name: &str) -> &str {
    name.strip_prefix('-').and_then(|rest| rest.split_once('-')).map_or(name, |(_, base)| base)
}

/// dart-sass "special functions" whose contents may be raw text rather than
/// values, but which are worth a typed parse first (so plain `element(#id)`
/// or `-webkit-calc(1px + 2px)` keep their structured AST): `element(...)`
/// and `type(...)`, plus `calc(...)`/`url(...)` under an unrecognized vendor
/// prefix. (`expression(...)` and `progid:...(...)` are always raw, and an
/// unvendored `calc`/`url` is parsed as a real calculation/URL.)
fn is_special_typed_or_raw_function(name: &str) -> bool {
    let base = unvendored(name);
    let vendored = base.len() != name.len();
    base.eq_ignore_ascii_case("element")
        || (!vendored && (base.eq_ignore_ascii_case("type") || base.eq_ignore_ascii_case("if")))
        || (vendored && (base.eq_ignore_ascii_case("calc") || base.eq_ignore_ascii_case("url")))
}

impl<'a> Parser<'a> {
    pub(in crate::parser) fn parse_calc_expr(
        &mut self,
        allow_modulo: bool,
    ) -> PResult<ComponentValue<'a>> {
        self.parse_calc_expr_recursively(0, allow_modulo)
    }

    // https://drafts.csswg.org/css-values-4/#calc-syntax
    //
    // <calc-sum>     = <calc-product> [ [ '+' | '-' ] <calc-product> ]*
    // <calc-product> = <calc-value>   [ [ '*' | '/' ] <calc-value> ]*
    // <calc-value>   = <number> | <dimension> | <percentage> | ( <calc-sum> )
    // Precedence-climbing over the two operator tiers (Sass `%` modulo shares the
    // '*' tier when `allow_modulo`).
    fn parse_calc_expr_recursively(
        &mut self,
        precedence: u8,
        allow_modulo: bool,
    ) -> PResult<ComponentValue<'a>> {
        let mut left = if precedence >= PRECEDENCE_MULTIPLY {
            if self.cursor.eat_l_paren()?.is_some() {
                let expr = self.parse_calc_expr(allow_modulo)?;
                self.cursor.expect_r_paren()?;
                expr
            } else if matches!(self.syntax, Syntax::Scss | Syntax::Sass)
                && matches!(&self.cursor.peek()?.token, Token::Minus(..) | Token::Plus(..))
                && {
                    let span = &self.cursor.peek()?.span;
                    self.source.as_bytes().get(span.end) == Some(&b'(')
                }
            {
                // SassScript allows a unary sign glued to a parenthesized
                // operand inside a calculation (`round(-(1) + 2)`); a spaced
                // `calc(+ 1px)` stays invalid, as in dart-sass.
                let op = match &self.cursor.peek()?.token {
                    Token::Minus(..) => SassUnaryOperator {
                        kind: SassUnaryOperatorKind::Minus,
                        span: self.cursor.bump()?.span,
                    },
                    _ => SassUnaryOperator {
                        kind: SassUnaryOperatorKind::Plus,
                        span: self.cursor.bump()?.span,
                    },
                };
                let expr = self.parse_calc_expr_recursively(PRECEDENCE_MULTIPLY, allow_modulo)?;
                let span = Span { start: op.span.start, end: expr.span().end };
                ComponentValue::SassUnaryExpression(SassUnaryExpression {
                    expr: self.alloc(expr),
                    op,
                    span,
                })
            } else if self.syntax == Syntax::Less {
                if matches!(self.cursor.peek()?.token, Token::Minus(..)) {
                    ComponentValue::LessNegativeValue(self.parse()?)
                } else {
                    self.parse_component_value_atom()?
                }
            } else {
                self.parse_component_value_atom()?
            }
        } else {
            self.parse_calc_expr_recursively(precedence + 1, allow_modulo)?
        };

        loop {
            let operator = match &self.cursor.peek()?.token {
                Token::Asterisk(..) if precedence == PRECEDENCE_MULTIPLY => CalcOperator {
                    kind: CalcOperatorKind::Multiply,
                    span: self.cursor.bump()?.span,
                },
                Token::Solidus(..) if precedence == PRECEDENCE_MULTIPLY => CalcOperator {
                    kind: CalcOperatorKind::Division,
                    span: self.cursor.bump()?.span,
                },
                // Sass modulo (`%`) shares multiplicative precedence, but only the
                // legacy SassScript `min`/`max` accept it (`allow_modulo`); true
                // calculations (`calc`, `clamp`, `sin`, ...) reject it, as does CSS.
                Token::Percent(..) if precedence == PRECEDENCE_MULTIPLY && allow_modulo => {
                    CalcOperator { kind: CalcOperatorKind::Modulo, span: self.cursor.bump()?.span }
                }
                Token::Plus(..) if precedence == PRECEDENCE_PLUS => {
                    CalcOperator { kind: CalcOperatorKind::Plus, span: self.cursor.bump()?.span }
                }
                Token::Minus(..) if precedence == PRECEDENCE_PLUS => {
                    CalcOperator { kind: CalcOperatorKind::Minus, span: self.cursor.bump()?.span }
                }
                _ => break,
            };

            let right = self.parse_calc_expr_recursively(precedence + 1, allow_modulo)?;
            let span = Span { start: left.span().start, end: right.span().end };
            left = ComponentValue::Calc(Calc {
                left: self.alloc(left),
                op: operator,
                right: self.alloc(right),
                span,
            });
        }

        Ok(left)
    }

    // A single CSS `<component-value>`: a function, a `[]`/`()` block, or a
    // preserved token (ident, number, dimension, percentage, string, hash, url, …).
    // https://drafts.csswg.org/css-syntax-3/#component-value
    pub(super) fn parse_component_value_atom(&mut self) -> PResult<ComponentValue<'a>> {
        let token_with_span = self.cursor.peek()?;
        match &token_with_span.token {
            Token::Ident(..) => {
                let ident = token_with_span.ident(self.source).unwrap();
                if unvendored(&ident.name()).eq_ignore_ascii_case("url") {
                    match self.try_parse(Url::parse) {
                        Ok(url) => return Ok(ComponentValue::Url(self.alloc(url))),
                        Err(Error { kind: ErrorKind::TryParseError, .. }) => {}
                        Err(error) => {
                            // Not a `<url-token>` (quotes, parens, or raw
                            // whitespace inside). Reference compilers accept
                            // these as a function call with raw-ish contents
                            // (`url(fn("s"))`, multi-line data: URIs), so fall
                            // back to a function parse, keeping the original
                            // error if even that shape doesn't fit.
                            let (function_name, function_name_span) = self.cursor.expect_ident()?;
                            let function_name = self.ident(function_name, function_name_span);
                            return self
                                .parse_function_typed_or_raw(function_name)
                                .map(ComponentValue::Function)
                                .map_err(|_| error);
                        }
                    }
                }
                let ident = self.parse::<InterpolableIdent>()?;
                let ident_end = ident.span().end;
                match self.cursor.peek()? {
                    TokenWithSpan { token: Token::LParen(..), span } if span.start == ident_end => {
                        return match ident {
                            InterpolableIdent::Literal(ident)
                                if ident.name.eq_ignore_ascii_case("src") =>
                            {
                                self.parse_src_url(ident)
                                    .map(|url| ComponentValue::Url(self.alloc(url)))
                            }
                            InterpolableIdent::Literal(ident)
                                if unvendored(ident.name).eq_ignore_ascii_case("expression") =>
                            {
                                // IE `expression(...)` (any vendor prefix):
                                // contents are script, not CSS values.
                                self.parse_raw_function(InterpolableIdent::Literal(ident))
                                    .map(ComponentValue::Function)
                            }
                            InterpolableIdent::Literal(ident)
                                if is_special_typed_or_raw_function(ident.name) =>
                            {
                                self.parse_function_typed_or_raw(ident)
                                    .map(ComponentValue::Function)
                            }
                            ident => self.parse_function(ident).map(ComponentValue::Function),
                        };
                    }
                    // IE filter syntax `-c-progid:d.e(...)` — everything to
                    // the matching `)` is raw. (An unprefixed `progid:` at the
                    // start of a value takes the whole-value raw path in
                    // `Declaration::parse` instead.)
                    TokenWithSpan { token: Token::Colon(..), span }
                        if span.start == ident_end
                            && matches!(
                                &ident,
                                InterpolableIdent::Literal(id)
                                    if unvendored(id.name).eq_ignore_ascii_case("progid")
                            ) =>
                    {
                        if let InterpolableIdent::Literal(ident) = ident {
                            return self.parse_progid_function(ident).map(ComponentValue::Function);
                        }
                        unreachable!("guard matched a literal ident");
                    }
                    TokenWithSpan { token: Token::Dot(..), span }
                        if matches!(self.syntax, Syntax::Scss | Syntax::Sass)
                            && span.start == ident_end =>
                    {
                        if let InterpolableIdent::Literal(module) = &ident {
                            let module = Ident {
                                name: module.name,
                                raw: module.raw,
                                span: module.span.clone(),
                            };
                            // A namespaced member is `foo.$var` or a glued
                            // call `foo.bar(...)`.
                            let qualified = self.try_parse(|parser| {
                                let name = parser.parse_sass_qualified_name(module)?;
                                if let SassQualifiedName {
                                    member: SassModuleMemberName::Ident(..),
                                    ..
                                } = name
                                {
                                    let (_, lparen_span) = parser.cursor.expect_l_paren()?;
                                    util::assert_no_ws_or_comment(&name.span, &lparen_span)?;
                                    let args = parser.parse_function_args()?;
                                    let (_, Span { end, .. }) = parser.cursor.expect_r_paren()?;
                                    let span = Span { start: name.span.start, end };
                                    Ok(ComponentValue::Function(Function {
                                        name: FunctionName::SassQualifiedName(parser.alloc(name)),
                                        args,
                                        span,
                                    }))
                                } else {
                                    Ok(ComponentValue::SassQualifiedName(parser.alloc(name)))
                                }
                            });
                            return match qualified {
                                Ok(value) => Ok(value),
                                // `foo.bar` with no call: dart-sass rejects a
                                // plain ident member at compile time, but
                                // postcss-scss lexes the dotted run as ONE
                                // word (xstyled / tailwind-theme tokens).
                                // Keep the plain ident; the `.ident` tail
                                // parses as raw tokens (the Css-mode shape)
                                // via the `Token::Dot` atom arm.
                                Err(_) => Ok(ComponentValue::InterpolableIdent(ident)),
                            };
                        }
                    }
                    _ => {}
                }
                match ident {
                    InterpolableIdent::Literal(ident) if ident.raw.eq_ignore_ascii_case("u") => {
                        match self.cursor.peek()? {
                            TokenWithSpan { token: Token::Plus(..), span }
                                if span.start == ident_end =>
                            {
                                self.parse_unicode_range(ident).map(ComponentValue::UnicodeRange)
                            }
                            token @ TokenWithSpan { token: Token::Number(..), span }
                                if token
                                    .number_raw(self.source)
                                    .is_some_and(|raw| raw.starts_with('+'))
                                    && span.start == ident_end =>
                            {
                                self.parse_unicode_range(ident).map(ComponentValue::UnicodeRange)
                            }
                            token @ TokenWithSpan { token: Token::Dimension(..), span }
                                if token
                                    .dimension_value_raw(self.source)
                                    .is_some_and(|raw| raw.starts_with('+'))
                                    && span.start == ident_end =>
                            {
                                self.parse_unicode_range(ident).map(ComponentValue::UnicodeRange)
                            }
                            _ => Ok(ComponentValue::InterpolableIdent(InterpolableIdent::Literal(
                                ident,
                            ))),
                        }
                    }
                    _ => Ok(ComponentValue::InterpolableIdent(ident)),
                }
            }
            Token::Solidus(..) | Token::Comma(..) => self.parse().map(ComponentValue::Delimiter),
            Token::Number(..) => self.parse().map(ComponentValue::Number),
            Token::Dimension(..) => self.parse().map(ComponentValue::Dimension),
            Token::Percentage(..) => self.parse().map(ComponentValue::Percentage),
            Token::Hash(..) => {
                if self.syntax == Syntax::Less {
                    self.parse_maybe_hex_color_or_less_mixin_call()
                } else {
                    self.parse().map(ComponentValue::HexColor)
                }
            }
            Token::Str(..) => {
                self.parse().map(InterpolableStr::Literal).map(ComponentValue::InterpolableStr)
            }
            Token::LBracket(..) => self.parse().map(ComponentValue::BracketBlock),
            Token::DollarVar(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                self.parse().map(ComponentValue::SassVariable)
            }
            Token::DollarVar(..)
                if self.syntax == Syntax::Css && self.options.allow_postcss_simple_vars =>
            {
                self.parse().map(ComponentValue::PostcssSimpleVar)
            }
            Token::LParen(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                match self.try_parse(SassParenthesizedExpression::parse) {
                    Ok(expr) => Ok(ComponentValue::SassParenthesizedExpression(expr)),
                    Err(err) => self.parse().map(ComponentValue::SassMap).map_err(|_| err),
                }
            }
            Token::HashLBrace(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                let ident = self.parse_sass_interpolated_ident()?;
                match self.cursor.peek()? {
                    TokenWithSpan { token: Token::LParen(..), span }
                        if span.start == ident.span().end =>
                    {
                        self.parse_function(ident).map(ComponentValue::Function)
                    }
                    _ => Ok(ComponentValue::InterpolableIdent(ident)),
                }
            }
            Token::StrTemplate(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => self
                .parse()
                .map(InterpolableStr::SassInterpolated)
                .map(ComponentValue::InterpolableStr),
            Token::Ampersand(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                self.parse().map(ComponentValue::SassParentSelector)
            }
            Token::LBrace(..)
                if self.syntax == Syntax::Scss
                    && matches!(
                        self.state.qualified_rule_ctx,
                        Some(QualifiedRuleContext::DeclarationValue)
                    ) =>
            {
                self.parse().map(ComponentValue::SassNestingDeclaration)
            }
            Token::Indent(..)
                if self.syntax == Syntax::Sass
                    && matches!(
                        self.state.qualified_rule_ctx,
                        Some(QualifiedRuleContext::DeclarationValue)
                    ) =>
            {
                self.parse().map(ComponentValue::SassNestingDeclaration)
            }
            Token::AtKeyword(..) if self.syntax == Syntax::Less => {
                self.parse_less_maybe_variable_or_with_lookups()
            }
            Token::Dot(..) if self.syntax == Syntax::Less => {
                self.parse_less_maybe_mixin_call_or_with_lookups()
            }
            // Not Sass on its own — dart-sass has no bare `.` in values — but
            // the ident atom declines the namespaced parse for postcss-word
            // runs like `foo.bar.baz` (xstyled / tailwind-theme tokens),
            // whose `.` then lands here. Accept it when glued to a following
            // ident, keeping the Css-mode raw-token shape.
            Token::Dot(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                let dot = self.cursor.bump()?;
                match self.cursor.peek()? {
                    TokenWithSpan { token: Token::Ident(..), span }
                        if span.start == dot.span.end =>
                    {
                        Ok(ComponentValue::TokenWithSpan(dot))
                    }
                    _ => Err(Error { kind: ErrorKind::ExpectComponentValue, span: dot.span }),
                }
            }
            Token::StrTemplate(..) if self.syntax == Syntax::Less => self
                .parse()
                .map(InterpolableStr::LessInterpolated)
                .map(ComponentValue::InterpolableStr),
            Token::At(..) if self.syntax == Syntax::Less => {
                self.parse().map(ComponentValue::LessVariableVariable)
            }
            Token::DollarVar(..) if self.syntax == Syntax::Less => {
                self.parse().map(ComponentValue::LessPropertyVariable)
            }
            Token::Tilde(..) if self.syntax == Syntax::Less => {
                if let Ok(list_function_call) = self.try_parse(Function::parse) {
                    Ok(ComponentValue::Function(list_function_call))
                } else if let Ok(less_escaped_str) = self.try_parse(LessEscapedStr::parse) {
                    Ok(ComponentValue::LessEscapedStr(less_escaped_str))
                } else {
                    self.parse().map(ComponentValue::LessJavaScriptSnippet)
                }
            }
            Token::Percent(..) if self.syntax == Syntax::Less => self
                .try_parse(Function::parse)
                .map(ComponentValue::Function)
                .or_else(|_| self.parse().map(ComponentValue::LessPercentKeyword)),
            Token::BacktickCode(..) if self.syntax == Syntax::Less => {
                self.parse().map(ComponentValue::LessJavaScriptSnippet)
            }
            Token::Placeholder(..) => {
                let (placeholder, span) = self.cursor.expect_placeholder()?;
                Ok(ComponentValue::Placeholder((placeholder, span).into()))
            }
            _ => Err(Error {
                kind: ErrorKind::ExpectComponentValue,
                span: token_with_span.span.clone(),
            }),
        }
    }

    // <dashed-ident> = <ident-token> whose name starts with '--'
    pub(super) fn parse_dashed_ident(&mut self) -> PResult<InterpolableIdent<'a>> {
        let ident = self.parse()?;
        match &ident {
            InterpolableIdent::Literal(ident) if !ident.name.starts_with("--") => {
                self.recoverable_errors
                    .push(Error { kind: ErrorKind::ExpectDashedIdent, span: ident.span.clone() });
            }
            _ => {}
        }
        Ok(ident)
    }

    // Build a `<function>` from an already-parsed name: '(' <function-args> ')'.
    pub(super) fn parse_function(&mut self, name: InterpolableIdent<'a>) -> PResult<Function<'a>> {
        self.cursor.expect_l_paren()?;
        let args = if let Token::RParen(..) = &self.cursor.peek()?.token {
            self.vec()
        } else {
            match &name {
                InterpolableIdent::Literal(ident)
                    if ident.name.eq_ignore_ascii_case("calc")
                        || ident.name.eq_ignore_ascii_case("-webkit-calc")
                        || ident.name.eq_ignore_ascii_case("-moz-calc")
                        || ident.name.eq_ignore_ascii_case("min")
                        || ident.name.eq_ignore_ascii_case("max")
                        || ident.name.eq_ignore_ascii_case("clamp")
                        || ident.name.eq_ignore_ascii_case("sin")
                        || ident.name.eq_ignore_ascii_case("cos")
                        || ident.name.eq_ignore_ascii_case("tan")
                        || ident.name.eq_ignore_ascii_case("asin")
                        || ident.name.eq_ignore_ascii_case("acos")
                        || ident.name.eq_ignore_ascii_case("atan")
                        || ident.name.eq_ignore_ascii_case("sqrt")
                        || ident.name.eq_ignore_ascii_case("exp")
                        || ident.name.eq_ignore_ascii_case("abs")
                        || ident.name.eq_ignore_ascii_case("sign")
                        || ident.name.eq_ignore_ascii_case("hypot")
                        || ident.name.eq_ignore_ascii_case("round")
                        || ident.name.eq_ignore_ascii_case("mod")
                        || ident.name.eq_ignore_ascii_case("rem")
                        || ident.name.eq_ignore_ascii_case("atan2")
                        || ident.name.eq_ignore_ascii_case("pow")
                        || ident.name.eq_ignore_ascii_case("log") =>
                {
                    // Only the legacy SassScript `min`/`max` accept the Sass `%` modulo
                    // operator; true calculations (`calc`, `clamp`, `sin`, ...) reject it.
                    let allow_modulo = matches!(self.syntax, Syntax::Scss | Syntax::Sass)
                        && (ident.name.eq_ignore_ascii_case("min")
                            || ident.name.eq_ignore_ascii_case("max"));
                    // The calc grammar doesn't cover SassScript uses of these
                    // names (`abs(\$number: -3)`, `max(1 2 3...)`,
                    // `round(-(1) + 2)`); Scss/Sass fall back to a strict
                    // SassScript call — but only there, so invalid calc stays
                    // invalid. Other syntaxes have no fallback, so they skip
                    // the rollback snapshot entirely.
                    if !matches!(self.syntax, Syntax::Scss | Syntax::Sass) {
                        self.parse_calc_args(allow_modulo)?
                    } else {
                        let typed = self.try_parse(|p| {
                            let values = p.parse_calc_args(allow_modulo)?;
                            if matches!(&p.cursor.peek()?.token, Token::RParen(..)) {
                                Ok(values)
                            } else {
                                let span = p.cursor.peek()?.span.clone();
                                Err(Error { kind: ErrorKind::TryParseError, span })
                            }
                        });
                        match typed {
                            Ok(values) => values,
                            Err(error) => {
                                let (args, comma_spans) = self.parse_sass_invocation_args()?;
                                // Only a keyword argument justifies the fallback
                                // (`abs(\$number: -3)` is a SassScript call); plain
                                // expressions the calc grammar rejected
                                // (`calc(1px % 2px)`, double spreads) stay invalid.
                                if !args.iter().any(|arg| {
                                    matches!(arg, ComponentValue::SassKeywordArgument(..))
                                }) {
                                    return Err(error);
                                }
                                let mut values = self.vec_with_capacity(args.len() * 2);
                                let mut comma_spans = comma_spans.into_iter();
                                for (i, arg) in args.into_iter().enumerate() {
                                    if i > 0
                                        && let Some(span) = comma_spans.next()
                                    {
                                        values.push(ComponentValue::Delimiter(Delimiter {
                                            kind: DelimiterKind::Comma,
                                            span,
                                        }));
                                    }
                                    values.push(arg);
                                }
                                values
                            }
                        }
                    }
                }
                InterpolableIdent::Literal(ident) if ident.name.eq_ignore_ascii_case("element") => {
                    let id_selector = self.parse().map(ComponentValue::IdSelector)?;
                    self.vec1(id_selector)
                }
                InterpolableIdent::Literal(Ident { raw: "boolean" | "if", .. })
                    if self.syntax == Syntax::Less =>
                {
                    let less_condition = self.parse_less_condition(false)?;
                    let condition = ComponentValue::LessCondition(self.alloc(less_condition));
                    let mut args = self.parse_function_args()?;
                    args.insert(0, condition);
                    args
                }
                _ => self.parse_function_args()?,
            }
        };
        let end = self.cursor.expect_r_paren()?.1.end;
        let span = Span { start: name.span().start, end };
        Ok(Function { name: FunctionName::Ident(name), args, span })
    }

    /// The `calc()`-family argument list: comma-delimited calc expressions,
    /// with the SassScript spread (`max(1 2 3...)`) wrapping the preceding
    /// value. Stops before the closing `)`.
    fn parse_calc_args(
        &mut self,
        allow_modulo: bool,
    ) -> PResult<oxc_allocator::Vec<'a, ComponentValue<'a>>> {
        let mut values = self.vec_with_capacity(1);
        loop {
            match self.cursor.peek()? {
                TokenWithSpan { token: Token::RParen(..), .. } => break,
                TokenWithSpan { token: Token::Comma(..), .. } => {
                    values.push(ComponentValue::Delimiter(self.parse()?));
                }
                // a spread is SassScript, so only the legacy `min`/`max`
                // accept it (`clamp(1px 2px 3px...)` errors), and only once
                TokenWithSpan { token: Token::DotDotDot(..), .. }
                    if allow_modulo
                        && matches!(self.syntax, Syntax::Scss | Syntax::Sass)
                        && !values.is_empty()
                        && !values
                            .iter()
                            .any(|v| matches!(v, ComponentValue::SassArbitraryArgument(..))) =>
                {
                    let TokenWithSpan { span: Span { end, .. }, .. } = self.cursor.bump()?;
                    let value = values.pop().unwrap();
                    let span = Span { start: value.span().start, end };
                    values.push(ComponentValue::SassArbitraryArgument(SassArbitraryArgument {
                        value: self.alloc(value),
                        span,
                    }));
                }
                _ => values.push(self.parse_calc_expr(allow_modulo)?),
            }
        }
        Ok(values)
    }

    /// Parse a function with the typed grammar; when its arguments don't fit
    /// (dart-sass special functions carry raw text: `element(/**/ c)`,
    /// `-c-calc(@#$)`, `url(fn("s"))`), re-parse the contents as raw tokens.
    pub(super) fn parse_function_typed_or_raw(&mut self, name: Ident<'a>) -> PResult<Function<'a>> {
        let name_copy = Ident { name: name.name, raw: name.raw, span: name.span.clone() };
        match self.try_parse(|p| p.parse_function(InterpolableIdent::Literal(name))) {
            Ok(function) => Ok(function),
            Err(_) => self.parse_raw_function(InterpolableIdent::Literal(name_copy)),
        }
    }

    /// Parse `name(<raw contents>)` where the contents are preserved tokens
    /// balanced to the matching `)` (IE `expression(...)`, unknown
    /// `@supports` functions, and friends).
    pub(in crate::parser) fn parse_raw_function(
        &mut self,
        name: InterpolableIdent<'a>,
    ) -> PResult<Function<'a>> {
        self.cursor.expect_l_paren()?;
        let mut args = self.vec_with_capacity(4);
        self.parse_raw_function_args_into(&mut args)?;
        let end = self.cursor.expect_r_paren()?.1.end;
        let span = Span { start: name.span().start, end };
        Ok(Function { name: FunctionName::Ident(name), args, span })
    }

    /// IE filter syntax `progid:DXImageTransform.Microsoft.f(...)`, optionally
    /// vendor prefixed: the `:dotted.path` prefix and the parenthesized
    /// contents are all preserved tokens.
    fn parse_progid_function(&mut self, name: Ident<'a>) -> PResult<Function<'a>> {
        let mut args = self.vec_with_capacity(4);
        loop {
            match &self.cursor.peek()?.token {
                Token::LParen(..)
                | Token::Semicolon(..)
                | Token::RBrace(..)
                | Token::RParen(..)
                | Token::Eof(..)
                | Token::Indent(..)
                | Token::Dedent(..)
                | Token::Linebreak(..) => break,
                _ => args.push(ComponentValue::TokenWithSpan(self.cursor.bump()?)),
            }
        }
        self.cursor.expect_l_paren()?;
        self.parse_raw_function_args_into(&mut args)?;
        let end = self.cursor.expect_r_paren()?.1.end;
        let span = Span { start: name.span.start, end };
        Ok(Function { name: FunctionName::Ident(InterpolableIdent::Literal(name)), args, span })
    }

    /// Consume function contents as preserved tokens, balancing pairs, until
    /// the function's own `)`. Semicolons and stray delimiters are legal here
    /// (`expression(a;b)`, `url(data:...;base64,...)`).
    fn parse_raw_function_args_into(
        &mut self,
        values: &mut oxc_allocator::Vec<'a, ComponentValue<'a>>,
    ) -> PResult<()> {
        let mut pairs: Vec<util::PairedToken> = Vec::with_capacity(1);
        loop {
            match &self.cursor.peek()?.token {
                Token::Eof(..) => break,
                // Interpolated strings must be parsed structurally so the
                // tokenizer resumes the string after each `#{...}`.
                Token::StrTemplate(..) => {
                    values.push(ComponentValue::InterpolableStr(self.parse()?));
                    continue;
                }
                token => {
                    if !util::track_paired_token(token, &mut pairs) {
                        break;
                    }
                }
            }
            values.push(ComponentValue::TokenWithSpan(self.cursor.bump()?));
        }
        Ok(())
    }

    // A function's argument list: a run of `<component-value>` up to the closing
    // `)` (commas/`/` are preserved Delimiters).
    pub(super) fn parse_function_args(
        &mut self,
    ) -> PResult<oxc_allocator::Vec<'a, ComponentValue<'a>>> {
        let mut values = self.vec_with_capacity(4);
        loop {
            match &self.cursor.peek()?.token {
                Token::RParen(..) | Token::Eof(..) => break,
                Token::Semicolon(..) => {
                    values.push(self.parse().map(ComponentValue::Delimiter)?);
                }
                Token::Exclamation(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                    // while this syntax is weird, Bootstrap is actually using it
                    values.push(self.parse().map(ComponentValue::ImportantAnnotation)?);
                }
                Token::LBrace(..) if self.syntax == Syntax::Less => {
                    values.push(self.parse().map(ComponentValue::LessDetachedRuleset)?);
                }
                Token::Dot(..) | Token::NumberSign(..) if self.syntax == Syntax::Less => {
                    if let Ok(mixin) = self.try_parse(Parser::parse_less_anonymous_mixin) {
                        values.push(ComponentValue::LessAnonymousMixin(mixin));
                    } else if let Ok(value) = self.try_parse(ComponentValue::parse) {
                        values.push(value);
                    } else {
                        values.push(ComponentValue::TokenWithSpan(self.cursor.bump()?));
                    }
                }
                Token::Indent(..) | Token::Dedent(..) | Token::Linebreak(..) => {
                    self.cursor.bump()?;
                }
                // A stray delimiter is a plain token in CSS, but the
                // preprocessor dialects give it real syntax and their
                // reference compilers reject it in function arguments.
                Token::Unknown(..) if self.syntax != Syntax::Css => {
                    let span = self.cursor.peek()?.span.clone();
                    return Err(Error { kind: ErrorKind::UnknownToken, span });
                }
                _ => {
                    let value = if let Ok(value) = self.try_parse(ComponentValue::parse) {
                        value
                    } else {
                        values.push(ComponentValue::TokenWithSpan(self.cursor.bump()?));
                        continue;
                    };
                    if matches!(self.syntax, Syntax::Scss | Syntax::Sass) {
                        if let Some((_, mut span)) = self.cursor.eat_dot_dot_dot()? {
                            span.start = value.span().start;
                            values.push(ComponentValue::SassArbitraryArgument(
                                SassArbitraryArgument { value: self.alloc(value), span },
                            ));
                        } else if let ComponentValue::SassVariable(sass_var) = value {
                            if let Some((_, colon_span)) = self.cursor.eat_colon()? {
                                let value = self.parse::<ComponentValue>()?;
                                let span =
                                    Span { start: sass_var.span.start, end: value.span().end };
                                values.push(ComponentValue::SassKeywordArgument(
                                    SassKeywordArgument {
                                        name: sass_var,
                                        colon_span,
                                        value: self.alloc(value),
                                        span,
                                    },
                                ));
                            } else {
                                values.push(ComponentValue::SassVariable(sass_var));
                            }
                        } else {
                            values.push(value);
                        }
                    } else {
                        values.push(value);
                    }
                }
            }
        }
        Ok(values)
    }

    // <ratio> = <number [0,∞]> [ '/' <number [0,∞]> ]?
    // https://drafts.csswg.org/css-values-4/#ratios
    pub(super) fn parse_ratio(&mut self, numerator: Number<'a>) -> PResult<Ratio<'a>> {
        let (_, solidus_span) = self.cursor.expect_solidus()?;
        let denominator = self.parse::<Number>()?;
        if denominator.value <= 0.0 {
            self.recoverable_errors.push(Error {
                kind: ErrorKind::InvalidRatioDenominator,
                span: denominator.span.clone(),
            });
        }

        let span = Span { start: numerator.span.start, end: denominator.span.end };
        Ok(Ratio { numerator, solidus_span, denominator, span })
    }

    // The `src()` value function: src( [ <string> ]? <url-modifier>* )
    // https://drafts.csswg.org/css-values-4/#funcdef-src
    /// Parse the trailing modifier list inside `url(...)`, shared by
    /// [`parse_src_url`](Self::parse_src_url) and the `Url` `Parse` impl.
    fn parse_url_modifiers(&mut self) -> PResult<oxc_allocator::Vec<'a, UrlModifier<'a>>> {
        Ok(match &self.cursor.peek()?.token {
            Token::Ident(..) | Token::HashLBrace(..) | Token::AtLBraceVar(..) => {
                let mut modifiers = self.vec_with_capacity(1);
                loop {
                    modifiers.push(self.parse()?);
                    if let Token::RParen(..) = &self.cursor.peek()?.token {
                        break;
                    }
                }
                modifiers
            }
            _ => self.vec(),
        })
    }

    fn parse_src_url(&mut self, name: Ident<'a>) -> PResult<Url<'a>> {
        // caller of `parse_src_url` should make sure there're no whitespaces before paren
        self.cursor.expect_l_paren()?;
        let value = match &self.cursor.peek()?.token {
            Token::Str(..) | Token::StrTemplate(..) => {
                Some(UrlValue::Str(self.parse::<InterpolableStr>()?))
            }
            _ => None,
        };
        let modifiers = self.parse_url_modifiers()?;
        let end = self.cursor.expect_r_paren()?.1.end;
        let span = Span { start: name.span.start, end };
        Ok(Url { name, value, modifiers, span })
    }

    // <urange> = u '+' <ident-token> '?'* | u <dimension-token> '?'* | u <number-token> …
    // Written `U+0-10FFFF`, `U+4??`, etc. https://drafts.csswg.org/css-syntax-3/#urange-syntax
    fn parse_unicode_range(&mut self, prefix_ident: Ident<'a>) -> PResult<UnicodeRange<'a>> {
        let prefix = prefix_ident.raw.chars().next().unwrap();
        let (span_start, span_end) = match self.cursor.bump()? {
            TokenWithSpan { token: Token::Plus(..), span: plus_token_span } => {
                let start = plus_token_span.start;
                let mut end = match self.cursor.tokenizer.bump_without_ws_or_comments()? {
                    TokenWithSpan { token: Token::Ident(..) | Token::Question(..), span } => {
                        span.end
                    }
                    TokenWithSpan { token, span } => {
                        return Err(Error {
                            kind: ErrorKind::Unexpected("?", token.symbol()),
                            span,
                        });
                    }
                };
                loop {
                    match self.cursor.peek()? {
                        TokenWithSpan { token: Token::Question(..), span } if span.start == end => {
                            end = self.cursor.bump()?.span.end;
                        }
                        _ => break,
                    }
                }
                (start, end)
            }
            TokenWithSpan { token: Token::Dimension(..), span: dimension_token_span } => {
                let start = dimension_token_span.start;
                let mut end = dimension_token_span.end;
                loop {
                    match self.cursor.peek()? {
                        TokenWithSpan { token: Token::Question(..), span } if span.start == end => {
                            end = self.cursor.bump()?.span.end;
                        }
                        _ => break,
                    }
                }
                (start, end)
            }
            TokenWithSpan { token: Token::Number(..), span: number_token_span } => {
                let start = number_token_span.start;
                let mut end = number_token_span.end;
                match &self.cursor.peek()?.token {
                    Token::Question(..) => {
                        end = self.cursor.bump()?.span.end;
                        loop {
                            match self.cursor.peek()? {
                                TokenWithSpan { token: Token::Question(..), span }
                                    if span.start == end =>
                                {
                                    end = self.cursor.bump()?.span.end;
                                }
                                _ => break,
                            }
                        }
                    }
                    Token::Dimension(..) | Token::Number(..) => {
                        end = self.cursor.bump()?.span.end;
                    }
                    _ => {}
                }
                (start, end)
            }
            TokenWithSpan { span, .. } => {
                return Err(Error { kind: ErrorKind::InvalidUnicodeRange, span });
            }
        };

        let source = self.source.get(span_start + 1..span_end).ok_or(Error {
            kind: ErrorKind::InvalidUnicodeRange,
            span: Span { start: span_start + 1, end: span_end },
        })?;
        let span = Span { start: prefix_ident.span.start, end: span_end };
        let unicode_range = if let Some((left, right)) = source.split_once('-') {
            if left.len() > 6 || !left.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(Error { kind: ErrorKind::InvalidUnicodeRange, span });
            }
            if right.len() > 6
                || !right.trim_end_matches('?').chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(Error { kind: ErrorKind::InvalidUnicodeRange, span });
            }
            let start = u32::from_str_radix(left, 16)
                .map_err(|_| Error { kind: ErrorKind::InvalidUnicodeRange, span: span.clone() })?;
            let end = u32::from_str_radix(&replace_unicode_range_wildcards(right, 'F'), 16)
                .map_err(|_| Error { kind: ErrorKind::InvalidUnicodeRange, span: span.clone() })?;
            UnicodeRange { prefix, start, start_raw: left, end, end_raw: Some(right), span }
        } else {
            if source.len() > 6
                || !source.trim_end_matches('?').chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(Error { kind: ErrorKind::InvalidUnicodeRange, span });
            }
            let start = u32::from_str_radix(&replace_unicode_range_wildcards(source, '0'), 16)
                .map_err(|_| Error { kind: ErrorKind::InvalidUnicodeRange, span: span.clone() })?;
            let end = u32::from_str_radix(&replace_unicode_range_wildcards(source, 'F'), 16)
                .map_err(|_| Error { kind: ErrorKind::InvalidUnicodeRange, span: span.clone() })?;
            UnicodeRange { prefix, start, start_raw: source, end, end_raw: None, span }
        };
        // Value-level checks (end > U+10FFFF, start > end) are deliberately
        // NOT errors: reference compilers pass such ranges through and
        // browsers clamp/ignore them at used-value time (`U+??????`,
        // `U+123456`, `U+1A2B3C-10FFFF` all appear in real-world corpora).
        Ok(unicode_range)
    }
}

fn replace_unicode_range_wildcards(source: &str, replacement: char) -> String {
    source.chars().map(|c| if c == '?' { replacement } else { c }).collect()
}

// A `[]`-block of component values (a `<simple-block>` opened by `[`).
// https://drafts.csswg.org/css-syntax-3/#simple-block
impl<'a> Parse<'a> for BracketBlock<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let start = input.cursor.expect_l_bracket()?.1.start;
        let mut value = input.vec_with_capacity(3);
        loop {
            match &input.cursor.peek()?.token {
                Token::RBracket(..) => break,
                _ => value.push(input.parse()?),
            }
        }
        let end = input.cursor.expect_r_bracket()?.1.end;
        Ok(BracketBlock { value, span: Span { start, end } })
    }
}

// https://drafts.csswg.org/css-syntax-3/#component-value
//
// <component-value> = <preserved-token> | <function> | <simple-block>
// (Scss/Sass and Less parse a full operator expression at this position instead.)
impl<'a> Parse<'a> for ComponentValue<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        match input.syntax {
            Syntax::Css => input.parse_component_value_atom(),
            Syntax::Scss | Syntax::Sass => {
                input.parse_sass_bin_expr(/* allow_comparison */ true)
            }
            Syntax::Less => input.parse_less_operation(/* allow_mixin_call */ true),
        }
    }
}

// A list of `<component-value>` (public entry point; a `;` is kept as a Delimiter).
impl<'a> Parse<'a> for ComponentValues<'a> {
    /// This is for public-use only. For internal code of oxc-css-parser, **DO NOT** use.
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let first = input.parse::<ComponentValue>()?;
        let mut span = first.span().clone();

        let mut values = input.vec_with_capacity(4);
        values.push(first);
        loop {
            match &input.cursor.peek()?.token {
                Token::Eof(..) => break,
                Token::Semicolon(..) => {
                    values.push(input.parse().map(ComponentValue::Delimiter)?);
                }
                _ => values.push(input.parse()?),
            }
        }

        if let Some(value) = values.last() {
            span.end = value.span().end;
        }
        Ok(ComponentValues { values, span })
    }
}

// A preserved delimiter token: '/' | ',' | ';'
impl<'a> Parse<'a> for Delimiter {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        use crate::tokenizer::token::*;
        match input.cursor.bump()? {
            TokenWithSpan { token: Token::Solidus(..), span } => {
                Ok(Delimiter { kind: DelimiterKind::Solidus, span })
            }
            TokenWithSpan { token: Token::Comma(..), span } => {
                Ok(Delimiter { kind: DelimiterKind::Comma, span })
            }
            TokenWithSpan { token: Token::Semicolon(..), span } => {
                Ok(Delimiter { kind: DelimiterKind::Semicolon, span })
            }
            _ => unreachable!(),
        }
    }
}

// <dimension> = <number> <unit>   (a <dimension-token>: length, angle, time, …)
impl<'a> Parse<'a> for Dimension<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (dimension, span) = input.cursor.expect_dimension()?;
        input.dimension(dimension, span)
    }
}

// https://drafts.csswg.org/css-syntax-3/#function
//
// <function> = <function-token> <component-value>* ')'
impl<'a> Parse<'a> for Function<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let name = input.parse::<FunctionName>()?;
        match input.cursor.peek()? {
            TokenWithSpan { token: Token::LParen(..), span } => {
                util::assert_no_ws_or_comment(name.span(), span)?;
                match name {
                    FunctionName::Ident(name) => input.parse_function(name),
                    name => {
                        input.cursor.bump()?;
                        let args = input.parse_function_args()?;
                        let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
                        let span = Span { start: name.span().start, end };
                        Ok(Function { name, args, span })
                    }
                }
            }
            TokenWithSpan { token, span } => {
                Err(Error { kind: ErrorKind::Unexpected("(", token.symbol()), span: span.clone() })
            }
        }
    }
}

// The name before a function's `(`: an <ident-token>. Sass also allows a
// module-qualified `module.member`; Less adds `%`/`~` function forms.
impl<'a> Parse<'a> for FunctionName<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        match input.cursor.peek()?.token {
            Token::Ident(..) => {
                let ident = input.parse::<Ident>()?;
                match (&input.cursor.peek()?.token, input.syntax) {
                    (Token::Dot(..), Syntax::Scss | Syntax::Sass) => {
                        input.cursor.bump()?;
                        let member = input.parse::<Ident>()?;
                        let span = Span { start: ident.span.start, end: member.span.end };
                        Ok(FunctionName::SassQualifiedName(input.alloc(SassQualifiedName {
                            module: ident,
                            member: SassModuleMemberName::Ident(member),
                            span,
                        })))
                    }
                    _ => Ok(FunctionName::Ident(InterpolableIdent::Literal(ident))),
                }
            }
            Token::Percent(..) if input.syntax == Syntax::Less => {
                input.parse().map(FunctionName::LessFormatFunction)
            }
            Token::Tilde(..) if input.syntax == Syntax::Less => {
                input.parse().map(FunctionName::LessListFunction)
            }
            _ => {
                let TokenWithSpan { token, span } = input.cursor.bump()?;
                Err(Error { kind: ErrorKind::Unexpected("<ident>", token.symbol()), span })
            }
        }
    }
}

// <hex-color> = '#' [ 3 | 4 | 6 | 8 hex digits ]   (a <hash-token>)
impl<'a> Parse<'a> for HexColor<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (token, span) = input.cursor.expect_hash()?;
        let raw = token.raw;
        let value = if token.escaped { util::handle_escape_in(raw, input.allocator) } else { raw };
        Ok(HexColor { value, raw, span })
    }
}

// <ident-token>
impl<'a> Parse<'a> for Ident<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (ident, span) = input.cursor.expect_ident()?;
        Ok(input.ident(ident, span))
    }
}

// An <ident-token>, or a preprocessor-interpolated ident (Sass `#{}`, Less `@{}`)
// / css-in-js placeholder standing in for one.
impl<'a> Parse<'a> for InterpolableIdent<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        // A css-in-js placeholder stands in for an interpolated ident anywhere one
        // is expected (id selector `#${x}`, attribute value `[a=${x}]`, ...).
        if let Token::Placeholder(..) = input.cursor.peek()?.token {
            let (placeholder, span) = input.cursor.expect_placeholder()?;
            return Ok(InterpolableIdent::Placeholder((placeholder, span).into()));
        }
        match input.syntax {
            Syntax::Css => input.parse().map(InterpolableIdent::Literal),
            Syntax::Scss | Syntax::Sass => input.parse_sass_interpolated_ident(),
            Syntax::Less => {
                // Less variable interpolation is disallowed in declaration value
                if matches!(
                    input.state.qualified_rule_ctx,
                    Some(QualifiedRuleContext::DeclarationValue)
                ) {
                    input.parse().map(InterpolableIdent::Literal)
                } else {
                    input.parse_less_interpolated_ident()
                }
            }
        }
    }
}

// A <string-token>, or a Sass/Less interpolated string template.
impl<'a> Parse<'a> for InterpolableStr<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        match input.cursor.peek()? {
            TokenWithSpan { token: Token::Str(..), .. } => {
                input.parse().map(InterpolableStr::Literal)
            }
            TokenWithSpan { token: Token::StrTemplate(..), span } => match input.syntax {
                Syntax::Scss | Syntax::Sass => input.parse().map(InterpolableStr::SassInterpolated),
                Syntax::Less => input.parse().map(InterpolableStr::LessInterpolated),
                Syntax::Css => {
                    Err(Error { kind: ErrorKind::UnexpectedTemplateInCss, span: span.clone() })
                }
            },
            TokenWithSpan { span, .. } => {
                Err(Error { kind: ErrorKind::ExpectString, span: span.clone() })
            }
        }
    }
}

// <number-token>
impl<'a> Parse<'a> for Number<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (number, span) = input.cursor.expect_number()?;
        number
            .raw
            .parse()
            .map_err(|_| Error { kind: ErrorKind::InvalidNumber, span: span.clone() })
            .map(|value| Self { value, raw: number.raw, span })
    }
}

// <percentage> = <percentage-token>   (a <number> immediately followed by '%')
impl<'a> Parse<'a> for Percentage<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (token, span) = input.cursor.expect_percentage()?;
        Ok(Percentage {
            value: (token.value, Span { start: span.start, end: span.end - 1 }).try_into()?,
            span,
        })
    }
}

// <string-token>
impl<'a> Parse<'a> for Str<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (str, span) = input.cursor.expect_str()?;
        Ok(input.str(str, span))
    }
}

// https://drafts.csswg.org/css-values-4/#urls
//
// <url> = url( <string> <url-modifier>* ) | <url-token>
// (also accepts the Gecko `url-prefix(…)` / `domain(…)` @document matchers)
impl<'a> Parse<'a> for Url<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (prefix, prefix_span) = input.cursor.expect_ident()?;
        // `url-prefix(...)` and `domain(...)` (Gecko `@document` matchers)
        // take the same unquoted-URL contents as `url(...)` — token-level
        // scanning would mis-lex `//` in `https://` as a comment.
        let prefix_name = prefix.name();
        let base_name = unvendored(&prefix_name);
        if !base_name.eq_ignore_ascii_case("url")
            && !base_name.eq_ignore_ascii_case("url-prefix")
            && !base_name.eq_ignore_ascii_case("domain")
        {
            return Err(Error { kind: ErrorKind::ExpectUrl, span: prefix_span });
        }
        let prefix_start = prefix_span.start;
        let name = input.ident(prefix, prefix_span.clone());

        match input.cursor.peek()? {
            TokenWithSpan { token: Token::LParen(..), span } if prefix_span.end == span.start => {
                input.cursor.bump()?;
            }
            TokenWithSpan { span, .. } => {
                return Err(Error { kind: ErrorKind::TryParseError, span: span.clone() });
            }
        }

        if input.cursor.tokenizer.is_start_of_url_string() {
            let value = input.parse()?;
            let modifiers = input.parse_url_modifiers()?;
            let end = input.cursor.expect_r_paren()?.1.end;
            let span = Span { start: prefix_start, end };
            Ok(Url { name, value: Some(UrlValue::Str(value)), modifiers, span })
        } else if let Ok(value) = input.try_parse(UrlRaw::parse) {
            let span = Span {
                start: prefix_start,
                end: value.span.end + 1, // `)` is consumed, but span excludes it
            };
            Ok(Url { name, value: Some(UrlValue::Raw(value)), modifiers: input.vec(), span })
        } else {
            match input.syntax {
                Syntax::Css => {
                    Err(Error { kind: ErrorKind::InvalidUrl, span: input.cursor.bump()?.span })
                }
                Syntax::Scss | Syntax::Sass => {
                    let value = input.parse::<SassInterpolatedUrl>()?;
                    let span = Span {
                        start: prefix_start,
                        end: value.span.end + 1, // `)` is consumed, but span excludes it
                    };
                    Ok(Url {
                        name,
                        value: Some(UrlValue::SassInterpolated(value)),
                        modifiers: input.vec(),
                        span,
                    })
                }
                Syntax::Less => {
                    let value = UrlValue::LessEscapedStr(input.parse()?);
                    let (_, Span { end, .. }) = input.cursor.expect_r_paren()?;
                    let span = Span { start: prefix_start, end };
                    Ok(Url { name, value: Some(value), modifiers: input.vec(), span })
                }
            }
        }
    }
}

// <url-modifier> = <ident> | <function>
impl<'a> Parse<'a> for UrlModifier<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let ident = input.parse::<InterpolableIdent>()?;
        match input.cursor.peek()? {
            TokenWithSpan { token: Token::LParen(..), span } if ident.span().end == span.start => {
                input.parse_function(ident).map(UrlModifier::Function)
            }
            _ => Ok(UrlModifier::Ident(ident)),
        }
    }
}

// The unquoted URL body of a <url-token> (raw text up to the closing `)`).
impl<'a> Parse<'a> for UrlRaw<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let token = input.cursor.tokenizer.scan_url_raw_or_template()?;
        match token.url_raw(input.source) {
            Some(url) => {
                let span = token.span;
                let value = if url.escaped {
                    util::handle_escape_in(url.raw, input.allocator)
                } else {
                    url.raw
                };
                Ok(UrlRaw { value, raw: url.raw, span })
            }
            None => Err(Error {
                kind: ErrorKind::Unexpected("<url>", token.token.symbol()),
                span: token.span,
            }),
        }
    }
}
