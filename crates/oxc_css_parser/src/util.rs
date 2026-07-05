use crate::{
    Span,
    error::{Error, ErrorKind, PResult},
};
use oxc_allocator::Allocator;
use std::borrow::Cow;

pub fn is_css_wide_keyword(s: &str) -> bool {
    s.eq_ignore_ascii_case("initial")
        || s.eq_ignore_ascii_case("inherit")
        || s.eq_ignore_ascii_case("unset")
        || s.eq_ignore_ascii_case("revert")
        || s.eq_ignore_ascii_case("revert-layer")
}

/// `PairedToken` is used for tracking when parsing with raw tokens.
pub(crate) enum PairedToken {
    Paren,
    Bracket,
    Brace,
}

/// Track `()`/`[]`/`{}` nesting for raw-token scans (`#{` counts as a brace
/// opener). Returns `false` for an unmatched closer — the caller's scan
/// should stop before consuming it. Non-pairing tokens return `true`.
pub(crate) fn track_paired_token(
    token: &crate::tokenizer::Token,
    pairs: &mut Vec<PairedToken>,
) -> bool {
    use crate::tokenizer::Token;
    match token {
        Token::LParen(..) => pairs.push(PairedToken::Paren),
        Token::RParen(..) => {
            if !matches!(pairs.pop(), Some(PairedToken::Paren)) {
                return false;
            }
        }
        Token::LBracket(..) => pairs.push(PairedToken::Bracket),
        Token::RBracket(..) => {
            if !matches!(pairs.pop(), Some(PairedToken::Bracket)) {
                return false;
            }
        }
        Token::LBrace(..) | Token::HashLBrace(..) => pairs.push(PairedToken::Brace),
        Token::RBrace(..) if !matches!(pairs.pop(), Some(PairedToken::Brace)) => {
            return false;
        }
        _ => {}
    }
    true
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ListSeparatorKind {
    Unknown,
    Comma,
    Space,
}

pub fn handle_escape(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    let mut escaped = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            // Copy a run of literal (unescaped) bytes up to the next backslash.
            // A `\\` (0x5C) is never a UTF-8 continuation byte, so the run always
            // ends on a code point boundary.
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'\\' {
                i += 1;
            }
            escaped.push_str(&s[start..i]);
            continue;
        }
        i += 1; // consume `\`
        match bytes.get(i) {
            Some(&c) if c.is_ascii_hexdigit() => {
                let start = i;
                let mut count: usize = 1;
                i += 1;
                while count < 6 && bytes.get(i).is_some_and(u8::is_ascii_hexdigit) {
                    count += 1;
                    i += 1;
                }
                // according to https://www.w3.org/TR/css-syntax-3/#hex-digit,
                // consume a whitespace
                if bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                    i += 1;
                }
                let unicode = u32::from_str_radix(&s[start..start + count], 16)
                    .expect("expect unicode value"); // this line should be unreachable
                escaped.push(char::from_u32(unicode).unwrap_or(char::REPLACEMENT_CHARACTER));
            }
            // `\` before any other code point escapes it literally. Copy the whole
            // (possibly multi-byte) code point: its leading byte plus any UTF-8
            // continuation bytes (`0x80..=0xBF`).
            Some(_) => {
                let start = i;
                i += 1;
                while bytes.get(i).is_some_and(|&b| b & 0xC0 == 0x80) {
                    i += 1;
                }
                escaped.push_str(&s[start..i]);
            }
            None => unreachable!(),
        }
    }
    Cow::from(escaped)
}

pub fn handle_escape_in<'a>(s: &'a str, allocator: &'a Allocator) -> &'a str {
    let escaped = handle_escape(s);
    match escaped {
        Cow::Borrowed(value) => value,
        Cow::Owned(value) => allocator.alloc_str(&value),
    }
}

pub(crate) fn assert_no_ws_or_comment(left: &Span, right: &Span) -> PResult<()> {
    debug_assert!(left.end <= right.start);
    if left.end == right.start {
        Ok(())
    } else {
        Err(Error {
            kind: ErrorKind::UnexpectedWhitespaceOrComments,
            span: Span { start: left.end, end: right.start },
        })
    }
}

pub(crate) fn assert_no_ws(source: &str, start: &Span, end: &Span) -> PResult<()> {
    if has_ws(source, start.end, end.start) {
        Err(Error {
            kind: ErrorKind::UnexpectedWhitespace,
            span: Span { start: start.end, end: end.start },
        })
    } else {
        Ok(())
    }
}

pub(crate) fn has_ws(source: &str, start: usize, end: usize) -> bool {
    debug_assert!(start <= end);
    if end == start {
        false
    } else {
        match (source.as_bytes().get(start), source.as_bytes().get(end - 1)) {
            (Some(first), Some(last)) => first.is_ascii_whitespace() || last.is_ascii_whitespace(),
            (Some(first), _) => first.is_ascii_whitespace(),
            (_, Some(last)) => last.is_ascii_whitespace(),
            _ => false,
        }
    }
}
