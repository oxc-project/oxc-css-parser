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
