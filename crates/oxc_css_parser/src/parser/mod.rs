use self::state::ParserState;
use crate::{
    ParserOptions,
    ast::{
        Dimension, Ident, InterpolableIdentStaticPart, InterpolableStrStaticPart,
        InterpolableUrlStaticPart, Str,
    },
    config::Syntax,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan, Tokenizer, token},
    util,
};
pub use builder::ParserBuilder;
use oxc_allocator::{Allocator, Vec as ArenaVec};

mod at_rule;
mod builder;
mod convert;
mod less;
mod postcss_simple_vars;
mod sass;
mod selector;
mod state;
mod stmt;
mod token_seq;
mod value;

pub trait Parse<'a>: Sized {
    fn parse(input: &mut Parser<'a>) -> PResult<Self>;
}

pub(in crate::parser) struct ParserCursor<'a> {
    tokenizer: Tokenizer<'a>,
    cached_token: Option<TokenWithSpan<'a>>,
    source: &'a str,
}

impl<'a> ParserCursor<'a> {
    #[inline]
    fn new(tokenizer: Tokenizer<'a>, source: &'a str) -> Self {
        Self { tokenizer, cached_token: None, source }
    }

    #[inline]
    fn bump(&mut self) -> PResult<TokenWithSpan<'a>> {
        match self.cached_token.take() {
            Some(token_with_span) => Ok(token_with_span),
            None => self.tokenizer.bump(),
        }
    }

    #[inline]
    fn peek(&mut self) -> PResult<&TokenWithSpan<'a>> {
        if self.cached_token.is_none() {
            let token = self.tokenizer.bump()?;
            self.cached_token = Some(token);
        }
        match self.cached_token.as_ref() {
            Some(token_with_span) => Ok(token_with_span),
            None => unreachable!(),
        }
    }

    #[inline]
    fn eat_token<T>(
        &mut self,
        extract: impl FnOnce(&TokenWithSpan<'a>, &'a str) -> Option<T>,
    ) -> PResult<Option<(T, Span)>> {
        let token_with_span = self.bump()?;
        if let Some(token) = extract(&token_with_span, self.source) {
            Ok(Some((token, token_with_span.span)))
        } else {
            self.cached_token = Some(token_with_span);
            Ok(None)
        }
    }

    #[inline]
    fn expect_token<T>(
        &mut self,
        expected: &'static str,
        extract: impl FnOnce(&TokenWithSpan<'a>, &'a str) -> Option<T>,
    ) -> PResult<(T, Span)> {
        let token_with_span = self.bump()?;
        if let Some(token) = extract(&token_with_span, self.source) {
            Ok((token, token_with_span.span))
        } else {
            Err(Error {
                kind: ErrorKind::Unexpected(expected, token_with_span.token.symbol()),
                span: token_with_span.span,
            })
        }
    }

    #[inline]
    fn eat_kind<T>(
        &mut self,
        extract: impl FnOnce(Token<'a>) -> Option<T>,
    ) -> PResult<Option<(T, Span)>> {
        self.eat_token(|token, _| extract(token.token))
    }

    #[inline]
    fn expect_kind<T>(
        &mut self,
        expected: &'static str,
        extract: impl FnOnce(Token<'a>) -> Option<T>,
    ) -> PResult<(T, Span)> {
        self.expect_token(expected, |token, _| extract(token.token))
    }

    #[inline]
    fn expect_ampersand(&mut self) -> PResult<(token::Ampersand, Span)> {
        self.expect_kind("&", |token| match token {
            Token::Ampersand(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_at(&mut self) -> PResult<(token::At, Span)> {
        self.expect_kind("@", |token| match token {
            Token::At(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_at_keyword(&mut self) -> PResult<(token::AtKeyword<'a>, Span)> {
        self.expect_token("<at-keyword>", |token, source| token.at_keyword(source))
    }

    #[inline]
    fn expect_at_l_brace_var(&mut self) -> PResult<(token::AtLBraceVar<'a>, Span)> {
        self.expect_token("@{", |token, source| token.at_l_brace_var(source))
    }

    #[inline]
    fn expect_backtick_code(&mut self) -> PResult<(token::BacktickCode<'a>, Span)> {
        self.expect_token("<backtick code>", |token, source| token.backtick_code(source))
    }

    #[inline]
    fn expect_bar(&mut self) -> PResult<(token::Bar, Span)> {
        self.expect_kind("|", |token| match token {
            Token::Bar(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_colon(&mut self) -> PResult<(token::Colon, Span)> {
        self.expect_kind(":", |token| match token {
            Token::Colon(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_colon_colon(&mut self) -> PResult<(token::ColonColon, Span)> {
        self.expect_kind("::", |token| match token {
            Token::ColonColon(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_comma(&mut self) -> PResult<(token::Comma, Span)> {
        self.expect_kind(",", |token| match token {
            Token::Comma(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_dimension(&mut self) -> PResult<(token::Dimension<'a>, Span)> {
        self.expect_token("<dimension>", |token, source| token.dimension(source))
    }

    #[inline]
    fn expect_dollar_l_brace_var(&mut self) -> PResult<(token::DollarLBraceVar<'a>, Span)> {
        self.expect_token("${", |token, source| token.dollar_l_brace_var(source))
    }

    #[inline]
    fn expect_dollar_var(&mut self) -> PResult<(token::DollarVar<'a>, Span)> {
        self.expect_token("$var", |token, source| token.dollar_var(source))
    }

    #[inline]
    fn expect_dot(&mut self) -> PResult<(token::Dot, Span)> {
        self.expect_kind(".", |token| match token {
            Token::Dot(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_eof(&mut self) -> PResult<(token::Eof, Span)> {
        self.expect_kind("<eof>", |token| match token {
            Token::Eof(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_exclamation(&mut self) -> PResult<(token::Exclamation, Span)> {
        self.expect_kind("!", |token| match token {
            Token::Exclamation(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_hash(&mut self) -> PResult<(token::Hash<'a>, Span)> {
        self.expect_token("<hash>", |token, source| token.hash(source))
    }

    #[inline]
    fn expect_hash_l_brace(&mut self) -> PResult<(token::HashLBrace, Span)> {
        self.expect_kind("#{", |token| match token {
            Token::HashLBrace(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_ident(&mut self) -> PResult<(token::Ident<'a>, Span)> {
        self.expect_token("<ident>", |token, source| token.ident(source))
    }

    #[inline]
    fn expect_l_brace(&mut self) -> PResult<(token::LBrace, Span)> {
        self.expect_kind("{", |token| match token {
            Token::LBrace(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_l_bracket(&mut self) -> PResult<(token::LBracket, Span)> {
        self.expect_kind("[", |token| match token {
            Token::LBracket(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_linebreak(&mut self) -> PResult<(token::Linebreak, Span)> {
        self.expect_kind("<linebreak>", |token| match token {
            Token::Linebreak(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_l_paren(&mut self) -> PResult<(token::LParen, Span)> {
        self.expect_kind("(", |token| match token {
            Token::LParen(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_minus(&mut self) -> PResult<(token::Minus, Span)> {
        self.expect_kind("-", |token| match token {
            Token::Minus(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_number(&mut self) -> PResult<(token::Number<'a>, Span)> {
        self.expect_token("<number>", |token, source| token.number(source))
    }

    #[inline]
    fn expect_percent(&mut self) -> PResult<(token::Percent, Span)> {
        self.expect_kind("%", |token| match token {
            Token::Percent(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_percentage(&mut self) -> PResult<(token::Percentage<'a>, Span)> {
        self.expect_token("<percentage>", |token, source| token.percentage(source))
    }

    #[inline]
    fn expect_placeholder(&mut self) -> PResult<(token::Placeholder<'a>, Span)> {
        self.expect_token("<placeholder>", |token, source| token.placeholder(source))
    }

    #[inline]
    fn expect_r_brace(&mut self) -> PResult<(token::RBrace, Span)> {
        self.expect_kind("}", |token| match token {
            Token::RBrace(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_r_bracket(&mut self) -> PResult<(token::RBracket, Span)> {
        self.expect_kind("]", |token| match token {
            Token::RBracket(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_r_paren(&mut self) -> PResult<(token::RParen, Span)> {
        self.expect_kind(")", |token| match token {
            Token::RParen(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_semicolon(&mut self) -> PResult<(token::Semicolon, Span)> {
        self.expect_kind(";", |token| match token {
            Token::Semicolon(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_solidus(&mut self) -> PResult<(token::Solidus, Span)> {
        self.expect_kind("/", |token| match token {
            Token::Solidus(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_str(&mut self) -> PResult<(token::Str<'a>, Span)> {
        self.expect_token("<string>", |token, source| token.str(source))
    }

    #[inline]
    fn expect_str_template(&mut self) -> PResult<(token::StrTemplate<'a>, Span)> {
        self.expect_token("<string template>", |token, source| token.str_template(source))
    }

    #[inline]
    fn expect_tilde(&mut self) -> PResult<(token::Tilde, Span)> {
        self.expect_kind("~", |token| match token {
            Token::Tilde(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_without_ws_or_comments<T>(
        &mut self,
        expected: &'static str,
        extract: impl FnOnce(Token<'a>) -> Option<T>,
    ) -> PResult<(T, Span)> {
        debug_assert!(self.cached_token.is_none());
        let TokenWithSpan { token, span } = self.tokenizer.bump_without_ws_or_comments()?;
        if let Some(token) = extract(token) {
            Ok((token, span))
        } else {
            Err(Error { kind: ErrorKind::Unexpected(expected, token.symbol()), span })
        }
    }

    #[inline]
    fn expect_ident_without_ws_or_comments(
        &mut self,
        allow_leading_digit: bool,
    ) -> PResult<(token::Ident<'a>, Span)> {
        debug_assert!(self.cached_token.is_none());
        if self.tokenizer.is_start_of_ident()
            || (allow_leading_digit && self.tokenizer.is_start_of_digit())
        {
            self.tokenizer.scan_ident_sequence(allow_leading_digit)
        } else {
            let TokenWithSpan { token, span } = self.tokenizer.bump_without_ws_or_comments()?;
            Err(Error { kind: ErrorKind::Unexpected("<ident>", token.symbol()), span })
        }
    }

    #[inline]
    fn expect_asterisk_without_ws_or_comments(&mut self) -> PResult<(token::Asterisk, Span)> {
        self.expect_without_ws_or_comments("*", |token| match token {
            Token::Asterisk(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_l_paren_without_ws_or_comments(&mut self) -> PResult<(token::LParen, Span)> {
        self.expect_without_ws_or_comments("(", |token| match token {
            Token::LParen(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn expect_solidus_without_ws_or_comments(&mut self) -> PResult<(token::Solidus, Span)> {
        self.expect_without_ws_or_comments("/", |token| match token {
            Token::Solidus(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_bar(&mut self) -> PResult<Option<(token::Bar, Span)>> {
        self.eat_kind(|token| match token {
            Token::Bar(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_colon(&mut self) -> PResult<Option<(token::Colon, Span)>> {
        self.eat_kind(|token| match token {
            Token::Colon(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_comma(&mut self) -> PResult<Option<(token::Comma, Span)>> {
        self.eat_kind(|token| match token {
            Token::Comma(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_dot_dot_dot(&mut self) -> PResult<Option<(token::DotDotDot, Span)>> {
        self.eat_kind(|token| match token {
            Token::DotDotDot(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_exclamation(&mut self) -> PResult<Option<(token::Exclamation, Span)>> {
        self.eat_kind(|token| match token {
            Token::Exclamation(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_greater_than(&mut self) -> PResult<Option<(token::GreaterThan, Span)>> {
        self.eat_kind(|token| match token {
            Token::GreaterThan(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_ident(&mut self) -> PResult<Option<(token::Ident<'a>, Span)>> {
        self.eat_token(|token, source| token.ident(source))
    }

    #[inline]
    fn eat_indent(&mut self) -> PResult<Option<(token::Indent, Span)>> {
        self.eat_kind(|token| match token {
            Token::Indent(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_linebreak(&mut self) -> PResult<Option<(token::Linebreak, Span)>> {
        self.eat_kind(|token| match token {
            Token::Linebreak(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_l_paren(&mut self) -> PResult<Option<(token::LParen, Span)>> {
        self.eat_kind(|token| match token {
            Token::LParen(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_r_paren(&mut self) -> PResult<Option<(token::RParen, Span)>> {
        self.eat_kind(|token| match token {
            Token::RParen(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_semicolon(&mut self) -> PResult<Option<(token::Semicolon, Span)>> {
        self.eat_kind(|token| match token {
            Token::Semicolon(token) => Some(token),
            _ => None,
        })
    }

    #[inline]
    fn eat_tilde(&mut self) -> PResult<Option<(token::Tilde, Span)>> {
        self.eat_kind(|token| match token {
            Token::Tilde(token) => Some(token),
            _ => None,
        })
    }
}

/// Create a parser with some source code, then parse it.
pub struct Parser<'a> {
    allocator: &'a Allocator,
    source: &'a str,
    syntax: Syntax,
    options: ParserOptions,
    cursor: ParserCursor<'a>,
    state: ParserState,
    recoverable_errors: Vec<Error>,
    /// Indented syntax only: `Indent` tokens consumed as mid-statement line
    /// continuations (`@for $i\n  from 1...`). The statement's own block then
    /// starts "virtually" at that depth (see [`SimpleBlock`]'s parse), and any
    /// unconsumed levels are drained against `Dedent` tokens after the
    /// statement.
    sass_pending_indents: u32,
}

impl<'a> Parser<'a> {
    /// Create a parser with the given source code and specified syntax.
    /// If you need to control more options, please use [`ParserBuilder`].
    pub fn new(allocator: &'a Allocator, source: &'a str, syntax: Syntax) -> Self {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        Parser {
            allocator,
            source,
            syntax,
            options: Default::default(),
            cursor: ParserCursor::new(
                Tokenizer::new(allocator, source, syntax, None, false),
                source,
            ),
            state: Default::default(),
            recoverable_errors: vec![],
            sass_pending_indents: 0,
        }
    }

    /// Start to parse.
    pub fn parse<T>(&mut self) -> PResult<T>
    where
        T: Parse<'a>,
    {
        T::parse(self)
    }

    /// Retrieve recoverable errors.
    #[inline]
    pub fn recoverable_errors(&self) -> &[Error] {
        &self.recoverable_errors
    }

    /// Retrieve collected comments.
    #[inline]
    pub fn comments(&self) -> &[crate::tokenizer::token::Comment<'a>] {
        self.cursor.tokenizer.comments()
    }

    #[inline]
    pub(crate) fn alloc<T>(&self, value: T) -> oxc_allocator::Box<'a, T> {
        oxc_allocator::Box::new_in(value, &self.allocator)
    }

    #[inline]
    pub(crate) fn vec<T>(&self) -> ArenaVec<'a, T> {
        ArenaVec::new_in(&self.allocator)
    }

    #[inline]
    pub(crate) fn vec1<T>(&self, value: T) -> ArenaVec<'a, T> {
        let mut vec = self.vec_with_capacity(1);
        vec.push(value);
        vec
    }

    #[inline]
    pub(crate) fn vec_with_capacity<T>(&self, capacity: usize) -> ArenaVec<'a, T> {
        ArenaVec::with_capacity_in(capacity, &self.allocator)
    }

    #[inline]
    pub(crate) fn ident(&self, token: token::Ident<'a>, span: crate::Span) -> Ident<'a> {
        Ident { name: self.ident_name(&token), raw: token.raw, span }
    }

    // A `$`-prefixed variable name: '$' <ident>  (Sass variables, Less property
    // accessors, postcss-simple-vars).
    pub(super) fn parse_dollar_var_ident(&mut self) -> PResult<(Ident<'a>, Span)> {
        let (dollar_var, span) = self.cursor.expect_dollar_var()?;
        let name = self.ident(dollar_var.ident, Span { start: span.start + 1, end: span.end });
        Ok((name, span))
    }

    pub(crate) fn dimension(
        &self,
        token: token::Dimension<'a>,
        span: crate::Span,
    ) -> PResult<Dimension<'a>> {
        let value_span = crate::Span { start: span.start, end: span.start + token.value.raw.len() };
        let unit_span = crate::Span { start: span.start + token.value.raw.len(), end: span.end };
        let value = (token.value, value_span).try_into()?;
        let unit = self.ident(token.unit, unit_span);
        let kind = convert::dimension_kind(unit.name);
        Ok(Dimension { value, unit, kind, span })
    }

    #[inline]
    pub(crate) fn interpolable_ident_static_part(
        &self,
        token: token::Ident<'a>,
        span: crate::Span,
    ) -> InterpolableIdentStaticPart<'a> {
        InterpolableIdentStaticPart { value: self.ident_name(&token), raw: token.raw, span }
    }

    #[inline]
    pub(crate) fn str(&self, token: token::Str<'a>, span: crate::Span) -> Str<'a> {
        let raw_without_quotes = unsafe { token.raw.get_unchecked(1..token.raw.len() - 1) };
        let value = if token.escaped {
            util::handle_escape_in(raw_without_quotes, self.allocator)
        } else {
            raw_without_quotes
        };
        Str { value, raw: token.raw, span }
    }

    #[inline]
    pub(crate) fn interpolable_str_static_part(
        &self,
        token: token::StrTemplate<'a>,
        span: crate::Span,
    ) -> InterpolableStrStaticPart<'a> {
        let raw_without_quotes = if token.tail {
            unsafe { token.raw.get_unchecked(0..token.raw.len() - 1) }
        } else if token.head {
            unsafe { token.raw.get_unchecked(1..token.raw.len()) }
        } else {
            token.raw
        };
        let value = if token.escaped {
            util::handle_escape_in(raw_without_quotes, self.allocator)
        } else {
            raw_without_quotes
        };
        InterpolableStrStaticPart { value, raw: token.raw, span }
    }

    #[inline]
    pub(crate) fn interpolable_url_static_part(
        &self,
        token: token::UrlTemplate<'a>,
        span: crate::Span,
    ) -> InterpolableUrlStaticPart<'a> {
        let value = if token.escaped {
            util::handle_escape_in(token.raw, self.allocator)
        } else {
            token.raw
        };
        InterpolableUrlStaticPart { value, raw: token.raw, span }
    }

    #[inline]
    fn ident_name(&self, token: &token::Ident<'a>) -> &'a str {
        if token.escaped { util::handle_escape_in(token.raw, self.allocator) } else { token.raw }
    }

    fn try_parse<R, F: FnOnce(&mut Self) -> PResult<R>>(&mut self, f: F) -> PResult<R> {
        let tokenizer_state = self.cursor.tokenizer.state.clone();
        let comments_count = self.cursor.tokenizer.comments_count();
        let recoverable_errors_count = self.recoverable_errors.len();
        let cached_token = self.cursor.cached_token.clone();
        let sass_pending_indents = self.sass_pending_indents;
        let result = f(self);
        if result.is_err() {
            self.cursor.tokenizer.state = tokenizer_state;
            self.cursor.tokenizer.truncate_comments(comments_count);
            self.recoverable_errors.truncate(recoverable_errors_count);
            self.cursor.cached_token = cached_token;
            self.sass_pending_indents = sass_pending_indents;
        }
        result
    }
}
