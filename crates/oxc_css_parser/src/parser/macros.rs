#[doc(hidden)]
#[macro_export]
macro_rules! expect {
    ($parser:expr, $variant:ident) => {{
        use $crate::{
            error::{Error, ErrorKind},
            tokenizer::{Token, TokenSymbol, TokenWithSpan},
        };
        match $parser.cursor.bump()? {
            TokenWithSpan { token: Token::$variant(token), span } => (token, span),
            TokenWithSpan { token, span } => {
                return Err(Error {
                    kind: ErrorKind::Unexpected(
                        $crate::tokenizer::token::$variant::symbol(),
                        token.symbol(),
                    ),
                    span,
                });
            }
        }
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! expect_without_ws_or_comments {
    ($parser:expr, Ident) => {
        $crate::expect_without_ws_or_comments!($parser, Ident, /* allow_leading_digit */ false)
    };
    // Like `Ident`, but the name may start with a digit — Less interpolation
    // names such as `@{3}` (matching the standalone `@{name}` tokenizer path).
    ($parser:expr, Ident, $allow_leading_digit:expr) => {{
        use $crate::{
            error::{Error, ErrorKind},
            tokenizer::TokenSymbol,
        };
        debug_assert!($parser.cursor.cached_token.is_none());
        let allow_leading_digit = $allow_leading_digit;
        let tokenizer = &mut $parser.cursor.tokenizer;
        if tokenizer.is_start_of_ident() || (allow_leading_digit && tokenizer.is_start_of_digit()) {
            tokenizer.scan_ident_sequence(allow_leading_digit)?
        } else {
            let token_with_span = tokenizer.bump_without_ws_or_comments()?;
            return Err(Error {
                kind: ErrorKind::Unexpected(
                    $crate::tokenizer::token::Ident::symbol(),
                    token_with_span.token.symbol(),
                ),
                span: token_with_span.span,
            });
        }
    }};
    ($parser:expr, $variant:ident) => {{
        use $crate::{
            error::{Error, ErrorKind},
            tokenizer::{Token, TokenSymbol, TokenWithSpan},
        };
        debug_assert!($parser.cursor.cached_token.is_none());
        let tokenizer = &mut $parser.cursor.tokenizer;
        let token_with_span = tokenizer.bump_without_ws_or_comments()?;
        match token_with_span {
            TokenWithSpan { token: Token::$variant(token), span } => (token, span),
            TokenWithSpan { token, span } => {
                return Err(Error {
                    kind: ErrorKind::Unexpected(
                        $crate::tokenizer::token::$variant::symbol(),
                        token.symbol(),
                    ),
                    span,
                });
            }
        }
    }};
}
