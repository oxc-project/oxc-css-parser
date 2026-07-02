use self::state::ParserState;
use crate::{
    ParserOptions,
    ast::{
        Dimension, Ident, InterpolableIdentStaticPart, InterpolableStrStaticPart,
        InterpolableUrlStaticPart, Str,
    },
    config::Syntax,
    error::{Error, PResult},
    expect,
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
mod macros;
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
}

impl<'a> ParserCursor<'a> {
    #[inline]
    fn new(tokenizer: Tokenizer<'a>) -> Self {
        Self { tokenizer, cached_token: None }
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
        extract: impl FnOnce(Token<'a>) -> Result<T, Token<'a>>,
    ) -> PResult<Option<(T, Span)>> {
        let TokenWithSpan { token, span } = self.bump()?;
        match extract(token) {
            Ok(token) => Ok(Some((token, span))),
            Err(token) => {
                self.cached_token = Some(TokenWithSpan { token, span });
                Ok(None)
            }
        }
    }

    #[inline]
    fn eat_bar(&mut self) -> PResult<Option<(token::Bar, Span)>> {
        self.eat_token(|token| match token {
            Token::Bar(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_colon(&mut self) -> PResult<Option<(token::Colon, Span)>> {
        self.eat_token(|token| match token {
            Token::Colon(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_comma(&mut self) -> PResult<Option<(token::Comma, Span)>> {
        self.eat_token(|token| match token {
            Token::Comma(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_dot_dot_dot(&mut self) -> PResult<Option<(token::DotDotDot, Span)>> {
        self.eat_token(|token| match token {
            Token::DotDotDot(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_exclamation(&mut self) -> PResult<Option<(token::Exclamation, Span)>> {
        self.eat_token(|token| match token {
            Token::Exclamation(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_greater_than(&mut self) -> PResult<Option<(token::GreaterThan, Span)>> {
        self.eat_token(|token| match token {
            Token::GreaterThan(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_ident(&mut self) -> PResult<Option<(token::Ident<'a>, Span)>> {
        self.eat_token(|token| match token {
            Token::Ident(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_indent(&mut self) -> PResult<Option<(token::Indent, Span)>> {
        self.eat_token(|token| match token {
            Token::Indent(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_linebreak(&mut self) -> PResult<Option<(token::Linebreak, Span)>> {
        self.eat_token(|token| match token {
            Token::Linebreak(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_l_paren(&mut self) -> PResult<Option<(token::LParen, Span)>> {
        self.eat_token(|token| match token {
            Token::LParen(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_r_paren(&mut self) -> PResult<Option<(token::RParen, Span)>> {
        self.eat_token(|token| match token {
            Token::RParen(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_semicolon(&mut self) -> PResult<Option<(token::Semicolon, Span)>> {
        self.eat_token(|token| match token {
            Token::Semicolon(token) => Ok(token),
            token => Err(token),
        })
    }

    #[inline]
    fn eat_tilde(&mut self) -> PResult<Option<(token::Tilde, Span)>> {
        self.eat_token(|token| match token {
            Token::Tilde(token) => Ok(token),
            token => Err(token),
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
            cursor: ParserCursor::new(Tokenizer::new(allocator, source, syntax, None, false)),
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
    pub(crate) fn allocator(&self) -> &'a Allocator {
        self.allocator
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

    pub(super) fn parse_dollar_var_ident(&mut self) -> PResult<(Ident<'a>, Span)> {
        let (dollar_var, span) = expect!(self, DollarVar);
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
