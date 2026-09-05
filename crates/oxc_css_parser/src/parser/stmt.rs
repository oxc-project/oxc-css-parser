use super::{
    Parser,
    state::{ParserState, QualifiedRuleContext},
};
use crate::{
    Parse, Syntax,
    ast::*,
    error::{Error, ErrorKind, PResult},
    pos::Span,
    tokenizer::{Token, TokenWithSpan},
};

// https://drafts.csswg.org/css-syntax-3/#consume-declaration
//
// <declaration> = <ident-token> : <declaration-value>? [ '!' important ]?
impl<'a> Parse<'a> for Declaration<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        // Css: a postcss property name (README "Acceptance").
        // Statement position only: a feature query keeps the `<ident-token>`
        // grammar, and one the typed grammar rejects (`@supports (*zoom: 1)`)
        // is kept anyway by the `<general-enclosed>` raw fallback.
        let postcss_name = if input.syntax == Syntax::Css
            && input.state.in_statement
            && !input.at_plain_ident_property_name()?
        {
            input.try_parse(Parser::parse_postcss_property_name).ok()
        } else {
            None
        };
        // Scss / Less: the IE `*color` hack (dart-sass and less.js accept it),
        // kept as a property-name prefix, but only when glued: `* color`
        // (whitespace or a comment after the sigil) is not the hack, so leave
        // the token for the normal (failing) parse.
        let name_prefix_start = if input.syntax != Syntax::Css
            && input.state.in_statement
            && let TokenWithSpan { token: Token::Asterisk(..), span } = input.cursor.peek()?
            && input
                .source
                .as_bytes()
                .get(span.end)
                .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'/')
        {
            let start = span.start;
            input.cursor.bump()?;
            Some(start)
        } else {
            None
        };
        // A css-in-js `${}` placeholder may stand in for the property name
        // (`${foo}: ${bar}`); it is not a real ident, so accept it directly.
        let name = if let Some(name) = postcss_name {
            name
        } else if let Token::Placeholder(..) = input.cursor.peek()?.token {
            let (placeholder, span) = input.cursor.expect_placeholder()?;
            InterpolableIdent::Placeholder((placeholder, span).into())
        } else if input.state.in_statement
            && input.syntax == Syntax::Less
            && let token = input.cursor.peek()?
            && let Some(raw) = token.number_raw(input.source)
            && raw.bytes().all(|b| b.is_ascii_digit())
        {
            let span = token.span;
            input.cursor.bump()?;
            InterpolableIdent::Literal(Ident { name: raw, raw, span })
        } else {
            input
                .with_state(ParserState {
                    qualified_rule_ctx: Some(QualifiedRuleContext::DeclarationName),
                    ..input.state
                })
                .parse::<InterpolableIdent>()?
        };

        // https://tailwindcss.com/docs/theme#overriding-the-default-theme
        let name_suffix = if let TokenWithSpan { token: Token::Asterisk(..), span } =
            input.cursor.peek()?
            && name.span().end == span.start
        {
            input.cursor.bump()?;
            Some('*')
        } else {
            None
        };

        // Less property merge (`prop+: v`, `prop+_: v`). In Css the `+` is
        // part of the postcss property name above.
        let less_property_merge = if input.syntax == Syntax::Less { input.parse()? } else { None };

        let (_, colon_span) = input.cursor.expect_colon()?;
        let is_custom_property = name.is_custom_property();
        let (mut value, mut important, value_is_raw) = {
            let mut parser = input.with_state(ParserState {
                qualified_rule_ctx: Some(QualifiedRuleContext::DeclarationValue),
                ..input.state
            });
            // For IE-compatibility, regardless of the property name (`filter`,
            // `-ms-filter`, vendor variants...): `filter: progid:...`.
            // Not peeked for a custom property: its text rescan below must
            // start from the colon with nothing cached.
            let source = parser.source;
            let starts_with_progid = !is_custom_property
                && matches!(&name, InterpolableIdent::Literal(..))
                && parser.cursor.peek()?.is_ident_name_eq_ignore_ascii_case(source, "progid");
            if is_custom_property && matches!(parser.syntax, Syntax::Scss | Syntax::Sass) {
                // dart-sass reads a custom property as text,
                // so `//` outside `#{...}` is part of the value
                // even when the typed grammar can otherwise reach the terminator.
                parser.parse_sass_custom_property_value()?
            } else if is_custom_property || starts_with_progid {
                // The value is everything up to the top-level `;` (§5.5.6).
                // The typed parse may stop earlier (Scss ends a value at a nesting block),
                // which would turn `--x: { a: b } --y: c;` into two declarations.
                // So take the typed value only when it reaches the terminator.
                if let Ok((values, important)) =
                    parser.try_parse(Parser::parse_whole_declaration_value)
                {
                    (values, important, false)
                } else {
                    (parser.parse_declaration_value_tokens(false)?, None, true)
                }
            } else if parser.syntax == Syntax::Css
                || (parser.state.in_css_function_body
                    && matches!(&name, InterpolableIdent::Literal(..)))
            {
                // Scss/Sass/Less keep the strict grammar:
                // their dialects assign meaning to these tokens
                // and are expected to reject exactly what their reference compilers reject.
                parser.parse_css_any_value()?
            } else {
                (parser.parse_declaration_value()?, None, false)
            }
        };

        // CSS Syntax removes a trailing top-level `!important` from every declaration value,
        // including one preserved as raw tokens because the typed grammar could not read it.
        // Do this before looking at the cursor:
        // the raw scan has already advanced to the declaration terminator.
        if important.is_none() && value_is_raw {
            important = take_raw_important(input, &mut value);
        }
        if important.is_none()
            && let Token::Exclamation(..) = &input.cursor.peek()?.token
        {
            important = Some(input.parse::<ImportantAnnotation>()?);
        }
        // dart-sass allows `!important` mid-value (`fludge: foo bar
        // !important hux;`): when more value follows, the annotation is just
        // another component, and only a trailing one is structural.
        while matches!(input.syntax, Syntax::Scss | Syntax::Sass)
            && important.is_some()
            && !at_declaration_value_end(&input.cursor.peek()?.token)
        {
            if let Some(annotation) = important.take() {
                value.push(ComponentValue::ImportantAnnotation(annotation));
            }
            let more = input
                .with_state(ParserState {
                    qualified_rule_ctx: Some(QualifiedRuleContext::DeclarationValue),
                    ..input.state
                })
                .parse_declaration_value()?;
            for component in more {
                value.push(component);
            }
            if let Token::Exclamation(..) = &input.cursor.peek()?.token {
                important = Some(input.parse::<ImportantAnnotation>()?);
            }
        }

        let span = Span {
            start: name_prefix_start.unwrap_or(name.span().start),
            end: if let Some(important) = &important {
                important.span.end
            } else if let Some(last) = value.last() {
                last.span().end
            } else {
                colon_span.end
            },
        };
        Ok(Declaration {
            name,
            name_prefix: name_prefix_start.map(|_| '*'),
            name_suffix,
            colon_span,
            value,
            value_is_raw,
            important,
            less_property_merge,
            span,
        })
    }
}

/// Remove a trailing top-level `!important` from a preserved raw value.
///
/// Raw values flatten paired blocks into delimiter tokens, so first replay the
/// pair stack up to the `!`: at EOF an unclosed `(... !important` must keep the
/// annotation inside the value rather than promote it to declaration priority.
fn take_raw_important<'a>(
    input: &Parser<'a>,
    value: &mut oxc_allocator::Vec<'a, ComponentValue<'a>>,
) -> Option<ImportantAnnotation<'a>> {
    let len = value.len();
    if len < 2 {
        return None;
    }
    let (bang_start, important) = match (&value[len - 2], &value[len - 1]) {
        (
            ComponentValue::TokenWithSpan(TokenWithSpan { token: Token::Exclamation(..), span }),
            ComponentValue::TokenWithSpan(important),
        ) if important.is_ident_name_eq_ignore_ascii_case(input.source, "important") => {
            (span.start, *important)
        }
        _ => return None,
    };

    let mut pairs = Vec::with_capacity(1);
    for component in &value[..len - 2] {
        if let ComponentValue::TokenWithSpan(token) = component
            && !crate::util::track_paired_token(&token.token, &mut pairs)
        {
            return None;
        }
    }
    if !pairs.is_empty() {
        return None;
    }

    let ident = input.ident(important.ident(input.source)?, important.span);
    value.truncate(len - 2);
    Some(ImportantAnnotation { span: Span { start: bang_start, end: important.span.end }, ident })
}

impl<'a> Parser<'a> {
    /// `)` ends a feature-query declaration (`@supports (a: b)`), never a
    /// statement one: there it is a stray closer the caller must deal with.
    fn at_statement_value_end(in_statement: bool, token: &Token) -> bool {
        at_declaration_value_end(token) && !(in_statement && matches!(token, Token::RParen(..)))
    }

    /// A Scss / Sass custom property value as dart-sass reads it: text where
    /// `//` starts no comment outside `#{...}` (whose contents are SassScript).
    /// The typed grammar wins for formatter layout only when it reaches the
    /// terminator without meeting a line comment; otherwise the value is the
    /// raw text run.
    fn parse_sass_custom_property_value(
        &mut self,
    ) -> PResult<(oxc_allocator::Vec<'a, ComponentValue<'a>>, Option<ImportantAnnotation<'a>>, bool)>
    {
        debug_assert!(self.cursor.cached_token.is_none());
        let line_comments_seen = self.cursor.tokenizer.state.line_comments_seen;
        let typed = self.try_parse(|parser| {
            let value = parser.parse_whole_declaration_value()?;
            // A `//` the typed grammar read as a comment is value text (or, inside
            // `#{...}`, a comment the typed stream cannot place): rescan as text.
            if parser.cursor.tokenizer.state.line_comments_seen != line_comments_seen {
                let span = parser.cursor.peek()?.span;
                return Err(Error { kind: ErrorKind::TryParseError, span });
            }
            Ok(value)
        });
        if let Ok((values, important)) = typed {
            return Ok((values, important, false));
        }

        // The failed `try_parse` restored the cursor to the colon; rescan the
        // text with line comments disabled outside interpolation.
        let line_comments = self.cursor.tokenizer.state.line_comments;
        self.cursor.tokenizer.state.line_comments = false;
        let values = self.parse_declaration_value_tokens(false);
        self.cursor.tokenizer.state.line_comments = line_comments;
        Ok((values?, None, true))
    }

    /// The Css `<any-value>` declaration value (CSS Syntax §5): serialized
    /// selectors (`b: .c > d`), map-like blocks (`b: (3: 4)`) or stray delimiters
    /// are all valid preserved tokens even though the typed grammar has no node
    /// for them. The typed grammar wins when it accounts for everything up to
    /// the terminator; otherwise the whole value is the raw token run.
    pub(super) fn parse_css_any_value(
        &mut self,
    ) -> PResult<(oxc_allocator::Vec<'a, ComponentValue<'a>>, Option<ImportantAnnotation<'a>>, bool)>
    {
        if let Ok((values, important)) = self.try_parse(Parser::parse_whole_declaration_value) {
            return Ok((values, important, false));
        }
        // A CSS custom function body holds declarations only, so a top-level
        // `{}` there is part of the value; elsewhere it means this construct is
        // really a qualified rule (CSS Nesting disambiguation) and the
        // declaration is rejected.
        let in_fn_body = self.state.in_css_function_body;
        let values = self.parse_declaration_value_tokens(!in_fn_body)?;
        let next = self.cursor.peek()?;
        if !in_fn_body && let Token::LBrace(..) = next.token {
            return Err(Error { kind: ErrorKind::BlockInDeclarationValue, span: next.span });
        }
        Ok((values, None, true))
    }

    /// The common `color: red`: an `<ident-token>` followed by whitespace or a
    /// terminator can only be the single-ident run `parse_postcss_property_name`
    /// rejects, so skip its snapshot and rescan.
    fn at_plain_ident_property_name(&mut self) -> PResult<bool> {
        let TokenWithSpan { token, span } = self.cursor.peek()?;
        Ok(matches!(token, Token::Ident(..))
            && self
                .source
                .as_bytes()
                .get(span.end)
                .is_none_or(|b| b.is_ascii_whitespace() || matches!(b, b':' | b';' | b'{' | b'}')))
    }

    /// postcss's property name (Css only): the glued token run up to the first
    /// top-level `:`, whitespace or comment. A leading `:` is part of the run
    /// (`:x: y`, an IE hack). Errors when the run is a single `<ident-token>`
    /// (the typed grammar owns it), empty, or unbalanced, so `try_parse`
    /// restores the cursor.
    fn parse_postcss_property_name(&mut self) -> PResult<InterpolableIdent<'a>> {
        let TokenWithSpan { token, span } = self.cursor.peek()?;
        let start = span.start;
        let ident_only_end = matches!(token, Token::Ident(..)).then_some(span.end);
        let mut end = start;
        let mut pairs: Vec<crate::util::PairedToken> = Vec::new();
        loop {
            let TokenWithSpan { token, span } = self.cursor.peek()?;
            if end != start && span.start != end {
                break;
            }
            match token {
                Token::Colon(..) if pairs.is_empty() && end != start => break,
                Token::Semicolon(..) | Token::LBrace(..) | Token::RBrace(..)
                    if pairs.is_empty() =>
                {
                    break;
                }
                // Never name bytes; `#{}` pieces never appear in a Css property name.
                Token::Eof(..)
                | Token::Dedent(..)
                | Token::Linebreak(..)
                | Token::StrTemplate(..)
                | Token::Placeholder(..) => break,
                token => {
                    if !crate::util::track_paired_token(token, &mut pairs) {
                        break;
                    }
                }
            }
            end = span.end;
            self.cursor.bump()?;
        }
        if end == start || Some(end) == ident_only_end || !pairs.is_empty() {
            return Err(Error { kind: ErrorKind::ExpectRule, span: Span { start, end } });
        }
        let raw = &self.source[start..end];
        Ok(InterpolableIdent::Literal(Ident { name: raw, raw, span: Span { start, end } }))
    }

    /// The typed `<declaration-value>` and its `!important`.
    /// Fails unless they reach the declaration terminator,
    /// so the caller can fall back to raw tokens for whatever the typed grammar missed.
    pub(super) fn parse_whole_declaration_value(
        &mut self,
    ) -> PResult<(oxc_allocator::Vec<'a, ComponentValue<'a>>, Option<ImportantAnnotation<'a>>)>
    {
        let values = self.parse_declaration_value()?;
        let important = match &self.cursor.peek()?.token {
            Token::Exclamation(..) => Some(self.parse::<ImportantAnnotation>()?),
            _ => None,
        };
        let in_statement = self.state.in_statement;
        let next = self.cursor.peek()?;
        if Self::at_statement_value_end(in_statement, &next.token) {
            Ok((values, important))
        } else {
            Err(Error { kind: ErrorKind::ExpectComponentValue, span: next.span })
        }
    }
}

/// End of a declaration's value: the declaration terminator tokens.
fn at_declaration_value_end(token: &Token) -> bool {
    matches!(
        token,
        Token::Semicolon(..)
            | Token::RBrace(..)
            | Token::RParen(..)
            | Token::Dedent(..)
            | Token::Linebreak(..)
            | Token::Eof(..)
    )
}

// <important> = '!' important
impl<'a> Parse<'a> for ImportantAnnotation<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let (_, span) = input.cursor.expect_exclamation()?;
        input.eat_sass_line_continuation()?;
        let ident: Ident = input.parse::<Ident>()?;
        let span = Span { start: span.start, end: ident.span.end };
        if ident.name.eq_ignore_ascii_case("important") {
            Ok(ImportantAnnotation { ident, span })
        } else {
            Err(Error { kind: ErrorKind::ExpectImportantAnnotation, span })
        }
    }
}

// https://drafts.csswg.org/css-syntax-3/#consume-qualified-rule
//
// <qualified-rule> = <prelude> <{}-block>
// In a style context the prelude is a selector list:
//   <style-rule> = <selector-list> { <style-block> }
impl<'a> Parse<'a> for QualifiedRule<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let selector_list = input
            .with_state(ParserState {
                qualified_rule_ctx: Some(QualifiedRuleContext::Selector),
                ..input.state
            })
            .parse::<SelectorList>()?;
        let block = input.parse::<SimpleBlock>()?;
        let span = Span { start: selector_list.span.start, end: block.span.end };
        Ok(QualifiedRule { selector: selector_list, block, span })
    }
}

// https://drafts.csswg.org/css-syntax-3/#consume-block-contents
//
// <unknown-qualified-rule> = <ident-token> ':' <any-value> <{}-block>
//
// Two shapes end up here in CSS (section numbers: 2026-07 ED):
// - a declaration §5.5.6 rejects because a `{}` block is mixed with other values
//   (`BlockInDeclarationValue`); §5.5.5 then re-consumes it as a qualified rule
// - a statement led by a token that can start neither a declaration nor an at-rule
//   (`50% { }`); §5.5.5 consumes it as a qualified rule directly
// Either way the prelude is no selector, so it is kept as raw tokens.
// postcss keeps such rules too (postcss-nested-style dialects use the shape for nested config blocks),
// and Prettier prints the prelude verbatim.
impl<'a> Parse<'a> for UnknownQualifiedRule<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        debug_assert!(input.syntax == Syntax::Css);
        let prelude = input.parse_raw_prelude_tokens()?;
        // The scan also stops at `;` / statement boundaries;
        // `SimpleBlock`'s `expect_l_brace` rejects those, so only a block opener makes this shape.
        let block = input.parse::<SimpleBlock>()?;
        let span = Span { start: prelude.span.start, end: block.span.end };
        Ok(UnknownQualifiedRule { prelude, block, span })
    }
}

// https://drafts.csswg.org/css-syntax-3/#consume-simple-block
//
// <simple-block> = '{' <block-contents> '}'
// (Sass indented syntax substitutes Indent/Dedent for the braces.)
impl<'a> Parse<'a> for SimpleBlock<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let is_sass = input.syntax == Syntax::Sass;
        let start = if is_sass {
            // A continuation line deeper than this block's own level leaves a
            // pending indent whose `Dedent` arrives before the block opens
            // (`a,\n    b\n  c: d`); cancel those out first.
            let drained = input.drain_sass_pending_dedents()?;
            if let Some((_, span)) = input.cursor.eat_indent()? {
                span.end
            } else if drained
                && input.sass_pending_indents == 0
                && input.cursor.tokenizer.reopen_indent_level()
            {
                // The block's level sat between two known indents, so its
                // `Indent` was never emitted; re-open it directly.
                input.cursor.peek()?.span.start
            } else if input.sass_pending_indents > 0 {
                // The statement's clause consumed this block's `Indent` as a
                // line continuation (`@each $a in\n  b, c\n  .x\n    ...`);
                // enter the block "virtually" at that depth.
                input.sass_pending_indents -= 1;
                input.cursor.peek()?.span.start
            } else {
                let offset = input.cursor.peek()?.span.start;
                return Ok(SimpleBlock {
                    statements: input.vec(),
                    span: Span { start: offset, end: offset },
                });
            }
        } else {
            input.cursor.expect_l_brace()?.1.start
        };

        let statements = input.parse_statements(/* is_top_level */ false)?;

        // CSS Syntax: EOF closes all open constructs (a parse error, but the
        // tree is valid — browsers accept unclosed blocks at EOF). The
        // dialects' reference compilers reject them. Recovery is unchanged; the
        // parse error is surfaced via `recoverable_errors` so downstream
        // consumers can tell it apart from a properly closed block.
        if input.syntax == Syntax::Css && matches!(input.cursor.peek()?.token, Token::Eof(..)) {
            let end = input.cursor.peek()?.span.start;
            input
                .recoverable_errors
                .push(Error { kind: ErrorKind::EofInBlock, span: Span { start, end: start + 1 } });
            return Ok(SimpleBlock { statements, span: Span { start, end } });
        }

        if is_sass {
            match input.cursor.bump()? {
                TokenWithSpan { token: Token::Dedent(..) | Token::Eof(..), span } => {
                    let end = statements.last().map_or(span.start, |last| last.span().end);
                    Ok(SimpleBlock { statements, span: Span { start, end } })
                }
                TokenWithSpan { span, .. } => {
                    Err(Error { kind: ErrorKind::ExpectDedentOrEof, span })
                }
            }
        } else {
            let end = input.cursor.expect_r_brace()?.1.end;
            Ok(SimpleBlock { statements, span: Span { start, end } })
        }
    }
}

// https://drafts.csswg.org/css-syntax-3/#parse-a-stylesheet
//
// <stylesheet> = <rule-list>
impl<'a> Parse<'a> for Stylesheet<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let statements = input.parse_statements(/* is_top_level */ true)?;
        input.cursor.expect_eof()?;
        Ok(Stylesheet { statements, span: Span { start: 0, end: input.source.len() } })
    }
}

impl<'a> Parser<'a> {
    /// `<declaration-value>` consumed as raw tokens (CSS Syntax "preserved
    /// tokens"), balancing `()`/`[]`/`{}` pairs, until a top-level `;`, an
    /// unbalanced closer, or a statement boundary. Used for custom-property
    /// values and as the fallback for CSS values the typed grammar rejects.
    ///
    /// <https://drafts.csswg.org/css-syntax-3/#typedef-declaration-value>
    ///
    /// `stop_at_top_level_brace` implements the CSS Nesting disambiguation: a
    /// `{` at the top level of a normal declaration's value means the whole
    /// construct is really a qualified rule, so the value must end there.
    /// Custom properties are exempt (`--foo: {a:b}` is a valid value).
    pub(super) fn parse_declaration_value_tokens(
        &mut self,
        stop_at_top_level_brace: bool,
    ) -> PResult<oxc_allocator::Vec<'a, ComponentValue<'a>>> {
        let mut values = self.vec_with_capacity(3);
        let mut pairs = Vec::with_capacity(1);
        // Span of the outermost currently-open pair (kept in sync with `pairs`
        // going empty↔non-empty), so the EOF parse error can point at the
        // opener like `SimpleBlock` does, not at the end of file.
        let mut outermost_pair_span: Option<Span> = None;
        loop {
            match &self.cursor.peek()?.token {
                Token::Dedent(..) | Token::Linebreak(..) => break,
                // CSS Syntax: EOF closes any still-open `(`/`[`/`{` group; recovery
                // is unchanged, but record the parse error so downstream consumers
                // can tell it apart from a balanced value. Report the outermost
                // unclosed opener; a raw-value `{` (e.g. `--x: {` — legal in custom
                // properties) is a block, not a paren.
                Token::Eof(..) => {
                    if let (Some(pair), Some(span)) = (pairs.first(), outermost_pair_span) {
                        let kind = match pair {
                            crate::util::PairedToken::Brace => ErrorKind::EofInBlock,
                            _ => ErrorKind::UnclosedParen,
                        };
                        self.recoverable_errors.push(Error { kind, span });
                    }
                    break;
                }
                Token::Semicolon(..) if pairs.is_empty() => {
                    break;
                }
                Token::LBrace(..) if stop_at_top_level_brace && pairs.is_empty() => {
                    break;
                }
                // An unterminated string survives as a preserved token (a parse
                // error kept verbatim; CSS Syntax §4.3.5). The tokenizer emits a
                // `BadStr` for both recoverable forms; recover the spec's split
                // from where the string stopped — at EOF it is a `<string-token>`
                // (canonically closable by appending the quote), at a newline a
                // `<bad-string-token>`.
                Token::BadStr(..) => {
                    let span = self.cursor.peek()?.span;
                    let kind = if span.end == self.source.len() {
                        ErrorKind::UnterminatedString
                    } else {
                        ErrorKind::BadString
                    };
                    self.recoverable_errors.push(Error { kind, span });
                }
                // An interpolated string (e.g. `'#{$expr}'` inside
                // `filter: progid:...`) must be parsed structurally:
                // the tokenizer needs `scan_string_template` to resume
                // the string after each `#{...}`, so consuming its
                // tokens as a plain stream would mis-lex the rest.
                Token::StrTemplate(..) => {
                    values.push(ComponentValue::InterpolableStr(self.parse()?));
                    continue;
                }
                token => {
                    let was_empty = pairs.is_empty();
                    if !crate::util::track_paired_token(token, &mut pairs) {
                        break;
                    }
                    if was_empty && !pairs.is_empty() {
                        outermost_pair_span = Some(self.cursor.peek()?.span);
                    }
                }
            }
            values.push(ComponentValue::TokenWithSpan(self.cursor.bump()?));
        }
        Ok(values)
    }

    // The typed form of `<declaration-value>`: a list of `<component-value>` up to
    // the declaration terminator (`;`, `!`, `}`, or a statement boundary).
    pub(super) fn parse_declaration_value(
        &mut self,
    ) -> PResult<oxc_allocator::Vec<'a, ComponentValue<'a>>> {
        let mut values = self.vec_with_capacity(3);
        loop {
            match &self.cursor.peek()?.token {
                Token::RBrace(..)
                | Token::RParen(..)
                | Token::Semicolon(..)
                | Token::Dedent(..)
                | Token::Linebreak(..)
                | Token::Exclamation(..)
                | Token::Eof(..) => break,
                _ => {
                    let value = self.parse::<ComponentValue>()?;
                    match &value {
                        ComponentValue::SassNestingDeclaration(..)
                            if matches!(self.syntax, Syntax::Scss | Syntax::Sass) =>
                        {
                            values.push(value);
                            break;
                        }
                        _ => values.push(value),
                    }
                }
            }
        }
        Ok(values)
    }

    /// In a `@keyframes` body, an ident may start a keyframe block (`from {`)
    /// or — in real-world code — a plain declaration (`blah: blee;`); dart-sass
    /// accepts both. Returns the statement and whether it opened a block.
    fn parse_keyframe_block_or_declaration(&mut self) -> PResult<(Statement<'a>, bool)> {
        if let Ok(block) = self.try_parse(KeyframeBlock::parse) {
            Ok((Statement::KeyframeBlock(block), true))
        } else {
            match self.parse_statement_declaration() {
                Ok(decl) => Ok((Statement::Declaration(decl), false)),
                Err(error_decl) => {
                    // postcss accepts the declaration-shaped rule inside `@keyframes` too
                    if let Some(rule) = self.try_declaration_shaped_rule(&error_decl) {
                        return Ok((Statement::UnknownQualifiedRule(rule), true));
                    }
                    Err(error_decl)
                }
            }
        }
    }

    /// A declaration in statement position.
    /// CSS snapshots the statement start so `try_declaration_shaped_rule` can re-consume it;
    /// dialects have no fallback, and an unrecovered Err aborts the parse, so no snapshot.
    fn parse_statement_declaration(&mut self) -> PResult<Declaration<'a>> {
        if self.syntax == Syntax::Css {
            self.try_parse(Parser::parse_style_rule_declaration)
        } else {
            self.parse_style_rule_declaration()
        }
    }

    /// The §5.5.5 re-consume (see `UnknownQualifiedRule`), CSS only: dialects
    /// keep their reference compilers' strictness (Scss types the shape as
    /// nested properties; less.js rejects it).
    fn try_declaration_shaped_rule(
        &mut self,
        error_decl: &Error,
    ) -> Option<UnknownQualifiedRule<'a>> {
        if !matches!(error_decl.kind, ErrorKind::BlockInDeclarationValue)
            || self.syntax != Syntax::Css
        {
            return None;
        }
        self.try_parse(UnknownQualifiedRule::parse).ok()
    }

    /// The CSS Nesting `<style-block>` ambiguity: parse a qualified rule, falling
    /// back to a declaration when the `foo: bar` vs `foo { }` prelude is
    /// ambiguous. Returns the statement and whether it opened a block (for the
    /// caller's `is_block_element`).
    /// `prefer_rule_error` picks which attempt's error surfaces when both fail
    /// (inside a block the declaration's: `color red;` reports the missing `:`).
    ///
    /// <https://drafts.csswg.org/css-nesting-1/#syntax>
    fn parse_rule_or_declaration(
        &mut self,
        is_top_level: bool,
        prefer_rule_error: bool,
    ) -> PResult<(Statement<'a>, bool)> {
        match self.try_parse(QualifiedRule::parse) {
            Ok(rule) => Ok((Statement::QualifiedRule(rule), true)),
            Err(error_rule) => match self.parse_statement_declaration() {
                Ok(decl) => {
                    // Only Scss/Sass produce `SassNestingDeclaration`; in CSS this is
                    // always `false`, matching the previous per-syntax behavior.
                    let is_block_element = matches!(
                        decl.value.last(),
                        Some(ComponentValue::SassNestingDeclaration(..))
                    );
                    // A root declaration is a statement only in the css-in-js
                    // parse mode (README "Acceptance").
                    if is_top_level && self.options.template_placeholder.is_none() {
                        self.recoverable_errors
                            .push(Error { kind: ErrorKind::TopLevelDeclaration, span: decl.span });
                    }
                    Ok((Statement::Declaration(decl), is_block_element))
                }
                Err(error_decl) => {
                    if let Some(rule) = self.try_declaration_shaped_rule(&error_decl) {
                        return Ok((Statement::UnknownQualifiedRule(rule), true));
                    }
                    Err(if prefer_rule_error { error_rule } else { error_decl })
                }
            },
        }
    }

    /// A Css statement led by anything but an ident or an at-keyword:
    /// a qualified rule, a declaration with a postcss property name (`+color: red`),
    /// or, numeric-led, a §5.5.5 qualified rule with no selector prelude
    /// (`50% { }` inside a raw-prelude rule, oxc-project/oxc#26291). `"foo" {}` stays rejected.
    fn parse_css_statement(&mut self, is_top_level: bool) -> PResult<(Statement<'a>, bool)> {
        let TokenWithSpan { token, span } = self.cursor.peek()?;
        let span = *span;
        let numeric =
            matches!(token, Token::Percentage(..) | Token::Number(..) | Token::Dimension(..));
        // A non-ident lead is first of all a selector, so its rule error is
        // the useful one (`[attr {` reports the attribute matcher, not a colon).
        match self.parse_rule_or_declaration(is_top_level, true) {
            Err(_) if numeric => {
                let rule = self
                    .parse::<UnknownQualifiedRule>()
                    .map_err(|_| Error { kind: ErrorKind::ExpectRule, span })?;
                Ok((Statement::UnknownQualifiedRule(rule), true))
            }
            result => result,
        }
    }

    /// Parse a declaration in statement position (`ParserState::in_statement`);
    /// feature-query declarations call `Declaration::parse` directly.
    fn parse_style_rule_declaration(&mut self) -> PResult<Declaration<'a>> {
        self.with_state(ParserState { in_statement: true, ..self.state.clone() }).parse()
    }

    // Block contents: a mix of declarations, nested style rules and at-rules
    // (CSS Syntax `<block-contents>`; `is_top_level` selects the `<stylesheet>`
    // rule-list, where a declaration is a `TopLevelDeclaration` error except
    // in the css-in-js parse mode and in Less).
    // https://drafts.csswg.org/css-syntax-3/#consume-block-contents
    fn parse_statements(
        &mut self,
        is_top_level: bool,
    ) -> PResult<oxc_allocator::Vec<'a, Statement<'a>>> {
        let mut statements = self.vec_with_capacity(1);
        loop {
            // Set true for braced blocks AND `${}` placeholder statements: both
            // make the trailing terminator optional. A placeholder substitutes a
            // whole statement/declaration and, like postcss, needs no `;`, so the
            // next statement may follow directly (`${mixin}\n@media {...}`,
            // `${a} ${b}`, `${foo}: ${bar}`).
            let mut is_block_element = false;
            let TokenWithSpan { token, span } = self.cursor.peek()?;
            match token {
                Token::Ident(..) | Token::HashLBrace(..) | Token::AtLBraceVar(..) => {
                    match self.syntax {
                        Syntax::Css => {
                            if self.state.in_keyframes_at_rule {
                                let (stmt, is_block) =
                                    self.parse_keyframe_block_or_declaration()?;
                                is_block_element = is_block;
                                statements.push(stmt);
                            } else {
                                let (stmt, is_block) =
                                    self.parse_rule_or_declaration(is_top_level, is_top_level)?;
                                is_block_element = is_block;
                                statements.push(stmt);
                            }
                        }
                        Syntax::Scss | Syntax::Sass => {
                            if let Ok(sass_var_decl) =
                                self.try_parse(SassVariableDeclaration::parse)
                            {
                                statements.push(Statement::SassVariableDeclaration(
                                    self.alloc(sass_var_decl),
                                ));
                            } else if self.state.in_keyframes_at_rule {
                                let (stmt, is_block) =
                                    self.parse_keyframe_block_or_declaration()?;
                                is_block_element = is_block;
                                statements.push(stmt);
                            } else {
                                let (stmt, is_block) =
                                    self.parse_rule_or_declaration(is_top_level, is_top_level)?;
                                is_block_element = is_block;
                                statements.push(stmt);
                            }
                        }
                        Syntax::Less => {
                            if let Ok(stmt) = self.try_parse(Parser::parse_less_qualified_rule) {
                                statements.push(stmt);
                                is_block_element = true;
                            } else if let Ok(decl) =
                                // less.js parses root-level declarations and
                                // only rejects them at eval time.
                                self.try_parse(Declaration::parse)
                            {
                                statements.push(Statement::Declaration(decl));
                            } else if self.state.in_keyframes_at_rule {
                                statements.push(Statement::KeyframeBlock(self.parse()?));
                                is_block_element = true;
                            } else {
                                let fn_call = self.parse::<Function>()?;
                                is_block_element = matches!(
                                    fn_call.args.last(),
                                    Some(ComponentValue::LessDetachedRuleset(..))
                                );
                                statements.push(Statement::LessFunctionCall(fn_call));
                            }
                        }
                    }
                }
                // `5:-` — less.js's ruleProperty regex (`[_a-zA-Z0-9-]+`)
                // allows digit-only declaration names
                Token::Number(..)
                    if self.syntax == Syntax::Less
                        && !is_top_level
                        && self.source.as_bytes().get(span.end) == Some(&b':') =>
                {
                    let decl = self.parse_style_rule_declaration()?;
                    statements.push(Statement::Declaration(decl));
                }
                // `.3D(...)` — less.js allows digit-led mixin names, which
                // arrive as one <dimension-token>; they behave exactly like
                // `.foo` (`.3D ()`, `.3D;`), so only the leading `.` matters
                Token::Dot(..) | Token::Hash(..) | Token::Dimension(..)
                    if self.syntax == Syntax::Less
                        && (!matches!(token, Token::Dimension(..))
                            || self.source.as_bytes().get(span.start) == Some(&b'.')) =>
                {
                    let stmt = if let Ok(stmt) = self.try_parse(Parser::parse_less_qualified_rule) {
                        is_block_element = true;
                        stmt
                    } else if let Ok(mixin_def) = self.try_parse(LessMixinDefinition::parse) {
                        is_block_element = true;
                        Statement::LessMixinDefinition(self.alloc(mixin_def))
                    } else {
                        self.parse().map(Statement::LessMixinCall)?
                    };
                    statements.push(stmt);
                }
                // Css takes every remaining lead token through `parse_css_statement` below.
                Token::Dot(..) | Token::Hash(..)
                    if self.syntax != Syntax::Css && !self.state.in_keyframes_at_rule =>
                {
                    statements.push(Statement::QualifiedRule(self.parse()?));
                    is_block_element = true;
                }
                Token::Ampersand(..)
                | Token::LBracket(..)
                | Token::Colon(..)
                | Token::ColonColon(..)
                | Token::Asterisk(..)
                | Token::Bar(..)
                | Token::NumberSign(..)
                    if self.syntax != Syntax::Css && !self.state.in_keyframes_at_rule =>
                {
                    if matches!(self.cursor.peek()?.token, Token::Asterisk(..)) {
                        // `*color: red` / `*zoom: 1` (an IE<=7 hack) looks like a `*`
                        // universal selector but is a declaration; try the rule, then
                        // fall back to a declaration. (A `*` never starts a
                        // `LessExtendRule`, so this can precede the Less split.)
                        if self.syntax == Syntax::Less {
                            match self.try_parse(Parser::parse_less_qualified_rule) {
                                Ok(stmt) => {
                                    statements.push(stmt);
                                    is_block_element = true;
                                }
                                // less.js parses a root declaration (ident-led path above)
                                // but not a root `*` hack; keep root-level `*zoom: 1` an error.
                                Err(rule_err) if is_top_level => return Err(rule_err),
                                Err(_) => {
                                    let decl = self.parse_style_rule_declaration()?;
                                    statements.push(Statement::Declaration(decl));
                                }
                            }
                        } else {
                            let (stmt, is_block) =
                                self.parse_rule_or_declaration(is_top_level, is_top_level)?;
                            is_block_element = is_block;
                            statements.push(stmt);
                        }
                    } else if self.syntax == Syntax::Less {
                        if let Ok(extend_rule) = self.try_parse(LessExtendRule::parse) {
                            statements.push(Statement::LessExtendRule(extend_rule));
                        } else {
                            statements.push(self.parse_less_qualified_rule()?);
                            is_block_element = true;
                        }
                    } else {
                        statements.push(Statement::QualifiedRule(self.parse()?));
                        is_block_element = true;
                    }
                }
                Token::AtKeyword(..) => match self.syntax {
                    Syntax::Css => {
                        let at_rule = self.parse::<AtRule>()?;
                        is_block_element = at_rule.block.is_some();
                        statements.push(Statement::AtRule(at_rule));
                    }
                    Syntax::Scss | Syntax::Sass => {
                        let at_keyword_name =
                            self.cursor.peek()?.at_keyword(self.source).unwrap().ident.name();
                        match &*at_keyword_name {
                            "if" => {
                                let sass_if_at_rule = self.parse()?;
                                statements
                                    .push(Statement::SassIfAtRule(self.alloc(sass_if_at_rule)));
                                is_block_element = true;
                            }
                            "else" => {
                                return Err(Error {
                                    kind: ErrorKind::UnexpectedSassElseAtRule,
                                    span: self.cursor.bump()?.span,
                                });
                            }
                            _ => {
                                let at_rule = self.parse::<AtRule>()?;
                                is_block_element = at_rule.block.is_some();
                                statements.push(Statement::AtRule(at_rule));
                            }
                        }
                    }
                    Syntax::Less => {
                        if let Ok(less_variable_declaration) =
                            self.try_parse(LessVariableDeclaration::parse)
                        {
                            is_block_element = matches!(
                                less_variable_declaration.value,
                                ComponentValue::LessDetachedRuleset(..)
                            );
                            statements.push(Statement::LessVariableDeclaration(
                                self.alloc(less_variable_declaration),
                            ));
                        } else if let Ok(variable_call) = self.try_parse(LessVariableCall::parse) {
                            statements.push(Statement::LessVariableCall(variable_call));
                        } else {
                            let at_rule = self.parse::<AtRule>()?;
                            is_block_element = at_rule.block.is_some();
                            statements.push(Statement::AtRule(at_rule));
                        }
                    }
                },
                Token::Placeholder(..) => {
                    // A placeholder may start a qualified rule (a substituted
                    // selector, e.g. CSS-in-JS `${Component} { ... }`) or stand
                    // alone as a statement (e.g. `` `PLACEHOLDER-0`; ``).
                    //
                    // A placeholder-led selector must not absorb across a newline:
                    // prettier keeps `${mixin}` on its own line and the following
                    // selector as a separate rule (`${mixin}\n& > .x {}` is two
                    // statements, not one). So only attempt the rule when the block
                    // `{` is reachable without an intervening newline-then-selector.
                    //
                    // A placeholder may also be a declaration property name
                    // (`${foo}: ${bar}`), so try a declaration before falling back
                    // to a bare placeholder statement. Same-line only: the `:` of
                    // a rule on the next line (`${mixin}\n:hover { ... }`) must
                    // not be absorbed as a declaration colon — like the qualified
                    // rule check above, a newline ends what the placeholder can own.
                    let ph_end = self.cursor.peek()?.span.end;
                    if self.placeholder_starts_qualified_rule(ph_end)
                        && let Ok(rule) = self.try_parse(QualifiedRule::parse)
                    {
                        statements.push(Statement::QualifiedRule(rule));
                        is_block_element = true;
                    } else if self.placeholder_starts_declaration(ph_end)
                        && let Ok(declaration) = self.try_parse(Declaration::parse)
                    {
                        // Reached only via the placeholder token above, so this
                        // is the `${foo}: ${bar}` form (placeholder property name).
                        statements.push(Statement::Declaration(declaration));
                        is_block_element = true;
                    } else {
                        let (placeholder, span) = self.cursor.expect_placeholder()?;
                        statements.push(Statement::Placeholder((placeholder, span).into()));
                        is_block_element = true;
                    }
                }
                // Css too: postcss-extend-rule's `%thick-border {}`
                // (see the placeholder arm in `SimpleSelector`'s parser).
                Token::Percent(..)
                    if matches!(self.syntax, Syntax::Scss | Syntax::Sass | Syntax::Css) =>
                {
                    statements.push(Statement::QualifiedRule(self.parse()?));
                    is_block_element = true;
                }
                Token::DollarVar(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                    let declaration = self.parse()?;
                    statements.push(Statement::SassVariableDeclaration(self.alloc(declaration)));
                }
                Token::DollarVar(..) if self.syntax == Syntax::Css => {
                    // Prefer the typed postcss-simple-vars node for an exact
                    // `$name: value` declaration. If the name continues
                    // (`$name+`, `$name.foo`) or a top-level block makes the
                    // statement a raw-prelude rule, use the general Css
                    // rule/declaration disambiguation instead.
                    if let Ok(declaration) = self.try_parse(PostcssSimpleVarDeclaration::parse) {
                        statements
                            .push(Statement::PostcssSimpleVarDeclaration(self.alloc(declaration)));
                    } else {
                        let (stmt, is_block) = self.parse_css_statement(is_top_level)?;
                        is_block_element = is_block;
                        statements.push(stmt);
                    }
                }
                // Indented-syntax shorthands: `=name` defines a mixin
                // (`@mixin name`) and `+name` includes one (`@include name`).
                // A spaced `+ b` stays a sibling-combinator selector: `+` is
                // an include only when glued to an identifier.
                Token::Equal(..) if self.syntax == Syntax::Sass => {
                    let eq_span = self.cursor.bump()?.span;
                    self.eat_sass_line_continuation()?;
                    let prelude = self.parse::<SassMixin>()?;
                    let block = self
                        .with_state(ParserState {
                            sass_ctx: self.state.sass_ctx
                                | super::state::SASS_CTX_ALLOW_KEYFRAME_BLOCK,
                            ..self.state.clone()
                        })
                        .parse::<SimpleBlock>()?;
                    let span = Span { start: eq_span.start, end: block.span.end };
                    statements.push(Statement::AtRule(AtRule {
                        name: Ident { name: "mixin", raw: "=", span: eq_span },
                        prelude: Some(AtRulePrelude::SassMixin(self.alloc(prelude))),
                        block: Some(block),
                        span,
                    }));
                    is_block_element = true;
                }
                Token::Plus(..)
                    if self.syntax == Syntax::Sass
                        && crate::tokenizer::ident_starts_at(self.source, span.end) =>
                {
                    let plus_span = self.cursor.bump()?.span;
                    let prelude = self.parse::<SassInclude>()?;
                    let block = if matches!(
                        self.cursor.peek()?.token,
                        Token::LBrace(..) | Token::Indent(..)
                    ) {
                        Some(
                            self.with_state(ParserState {
                                sass_ctx: self.state.sass_ctx
                                    | super::state::SASS_CTX_ALLOW_KEYFRAME_BLOCK,
                                ..self.state.clone()
                            })
                            .parse::<SimpleBlock>()?,
                        )
                    } else {
                        None
                    };
                    let end = block.as_ref().map_or(prelude.span.end, |block| block.span.end);
                    let span = Span { start: plus_span.start, end };
                    is_block_element = block.is_some();
                    statements.push(Statement::AtRule(AtRule {
                        name: Ident { name: "include", raw: "+", span: plus_span },
                        prelude: Some(AtRulePrelude::SassInclude(self.alloc(prelude))),
                        block,
                        span,
                    }));
                }
                Token::GreaterThan(..) | Token::Plus(..) | Token::Tilde(..) | Token::BarBar(..)
                    if self.syntax != Syntax::Css =>
                {
                    if self.syntax == Syntax::Less {
                        statements.push(self.parse_less_qualified_rule()?);
                    } else {
                        statements.push(Statement::QualifiedRule(self.parse()?));
                    }
                    is_block_element = true;
                }
                Token::DollarLBraceVar(..) if self.syntax == Syntax::Less => {
                    statements.push(self.parse().map(Statement::Declaration)?);
                }
                Token::Cdo(..) | Token::Cdc(..) => {
                    self.cursor.bump()?;
                    continue;
                }
                Token::At(..) if matches!(self.syntax, Syntax::Scss | Syntax::Sass) => {
                    let unknown_sass_at_rule = self.parse::<UnknownSassAtRule>()?;
                    is_block_element = unknown_sass_at_rule.block.is_some();
                    statements.push(Statement::UnknownSassAtRule(self.alloc(unknown_sass_at_rule)));
                }
                Token::Percentage(..)
                    if self.state.in_keyframes_at_rule
                        || self.state.sass_ctx & super::state::SASS_CTX_ALLOW_KEYFRAME_BLOCK
                            != 0
                        || self.state.less_ctx & super::state::LESS_CTX_ALLOW_KEYFRAME_BLOCK
                            != 0 =>
                {
                    statements.push(Statement::KeyframeBlock(self.parse()?));
                    is_block_element = true;
                }
                Token::RBrace(..) | Token::Eof(..) | Token::Dedent(..) => break,
                Token::Semicolon(..) | Token::Linebreak(..) => {
                    self.cursor.bump()?;
                    continue;
                }
                Token::LBrace(..) if self.syntax == Syntax::Css => {
                    // An empty selector (`{}`): postcss parses it as a qualified rule
                    // with no selector, so build one with an empty selector list.
                    let start = span.start;
                    let block = self.parse::<SimpleBlock>()?;
                    let selector = SelectorList {
                        selectors: self.vec(),
                        comma_spans: self.vec(),
                        span: Span { start, end: start },
                    };
                    let span = Span { start, end: block.span.end };
                    statements.push(Statement::QualifiedRule(QualifiedRule {
                        selector,
                        block,
                        span,
                    }));
                    is_block_element = true;
                }
                // `@3: red` is an at-word to postcss, not a property name.
                Token::At(..) if self.syntax == Syntax::Css => {
                    return Err(Error { kind: ErrorKind::ExpectRule, span: *span });
                }
                _ if self.syntax == Syntax::Css && !self.state.in_keyframes_at_rule => {
                    let (stmt, is_block) = self.parse_css_statement(is_top_level)?;
                    is_block_element = is_block;
                    statements.push(stmt);
                }
                _ => {
                    return Err(Error {
                        kind: if self.state.in_keyframes_at_rule {
                            ErrorKind::ExpectKeyframeBlock
                        } else {
                            ErrorKind::ExpectRule
                        },
                        span: *span,
                    });
                }
            };
            // Drain continuation indents that never became a block (e.g.
            // `$a\n  : b` — the deeper line belonged to the statement's own
            // clause, so its matching `Dedent` has no block to close). A
            // drained `Dedent` is itself a line boundary, so the statement
            // separator is already satisfied.
            if self.drain_sass_pending_dedents()? {
                continue;
            }
            match &self.cursor.peek()?.token {
                Token::RBrace(..) | Token::Eof(..) | Token::Dedent(..) => break,
                _ => {
                    if self.syntax == Syntax::Sass {
                        // The indented syntax also accepts `;` as a statement
                        // terminator/separator (`a; b`), like a newline.
                        if is_block_element {
                            if self.cursor.eat_semicolon()?.is_none() {
                                self.cursor.eat_linebreak()?;
                            }
                        } else if self.cursor.eat_semicolon()?.is_none() {
                            self.cursor.expect_linebreak()?;
                        }
                    } else if is_block_element {
                        self.cursor.eat_semicolon()?;
                    } else {
                        self.cursor.expect_semicolon()?;
                    }
                }
            }
        }
        Ok(statements)
    }

    /// Whether a statement-position `${}` placeholder (ending at byte `from`)
    /// should be offered to `QualifiedRule::parse`. The css-in-js rule the parser
    /// can't see on its own, matching prettier:
    /// - a bare `{` after the placeholder IS absorbed — the placeholder is the
    ///   selector for that block (`${mixin}\n{ color: red }` is one rule; a bare
    ///   `{...}` is meaningless without a selector, so this is the only valid read)
    /// - a placeholder separated by whitespace from what follows, then a newline,
    ///   then selector content = a separate rule (`${mixin}\n& > .x {}` and
    ///   `${a} ${b}\nhtml {}` are two statements, not one — spaced placeholders
    ///   are typically mixin invocations, not selector pieces)
    /// - but a placeholder IMMEDIATELY glued to non-whitespace (e.g. `${p}:hover`
    ///   or `${p},`) is a compound-selector piece, so a multi-line selector list
    ///   (`${p}:hover &,\n${q}:focus &, { ... }`) is one rule — keep scanning for `{` across newlines.
    ///
    /// The real grammar (strings, comments, `#{...}` interpolations, validity) is
    /// left to `QualifiedRule::parse`, which runs next and rolls back if this guess was wrong.
    /// Deliberately NOT a tokenizer: it never early-exits on `;`/`}`
    /// (those may sit inside an attribute string or comment),
    /// so it can't misclassify a same-line selector containing them.
    fn placeholder_starts_qualified_rule(&self, from: usize) -> bool {
        let bytes = &self.source.as_bytes()[from..];
        // Immediately-adjacent non-whitespace (`${p}:hover`, `${p},`) means the
        // placeholder is a compound-selector piece: only `{` matters from here, regardless of newlines.
        if bytes.first().is_some_and(|b| !b.is_ascii_whitespace()) {
            return bytes.contains(&b'{');
        }
        // Otherwise the placeholder is separated by whitespace from what follows.
        // A `{` on the same line (whitespace-only prefix) still makes the
        // placeholder its selector; any non-whitespace after a newline starts a separate rule.
        let mut newline_seen = false;
        for &b in bytes {
            match b {
                b'{' => return true,
                // `\r`, `\r\n`, and `\n` all count as a newline (the tokenizer
                // treats a bare `\r` as a line break too).
                b'\n' | b'\r' => newline_seen = true,
                _ if b.is_ascii_whitespace() => {}
                _ if newline_seen => return false,
                _ => {}
            }
        }
        // No block at all -> a declaration or a bare placeholder, not a rule.
        false
    }

    /// Whether a statement-position `${}` placeholder (ending at byte `from`)
    /// should be offered to `Declaration::parse` as a property name
    /// (`${foo}: ${bar}`). Same-line only: a newline before the next
    /// non-whitespace means the placeholder stands alone and what follows is a
    /// separate statement (`${mixin}\n\n:disabled { ... }` must not become a
    /// declaration `${mixin}: disabled { ... }`). Whether a same-line follower
    /// actually forms a declaration is left to `Declaration::parse`, which
    /// rolls back if this guess was wrong.
    fn placeholder_starts_declaration(&self, from: usize) -> bool {
        for &b in &self.source.as_bytes()[from..] {
            match b {
                // A bare `\r` counts as a newline too, same as
                // `placeholder_starts_qualified_rule` above.
                b'\n' | b'\r' => return false,
                _ if b.is_ascii_whitespace() => {}
                _ => return true,
            }
        }
        false
    }
}
