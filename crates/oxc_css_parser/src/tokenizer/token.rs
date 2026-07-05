//! All supported tokens, and with comments.

use crate::pos::Span;
#[cfg(feature = "serialize")]
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "type", rename_all = "camelCase"))]
pub struct Comment<'s> {
    pub content: &'s str,
    pub kind: CommentKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub enum CommentKind {
    Block,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub enum TokenData {
    Eof(Eof),
    Ampersand(Ampersand),
    BadStr(BadStrMeta),
    Asterisk(Asterisk),
    AsteriskEqual(AsteriskEqual),
    At(At),
    AtKeyword(IdentMeta),
    AtLBraceVar(IdentMeta),
    BacktickCode(BacktickCodeMeta),
    Bar(Bar),
    BarBar(BarBar),
    BarEqual(BarEqual),
    CaretEqual(CaretEqual),
    Cdc(Cdc),
    Cdo(Cdo),
    Colon(Colon),
    ColonColon(ColonColon),
    Comma(Comma),
    Dedent(Dedent),
    Dimension(DimensionMeta),
    DollarEqual(DollarEqual),
    DollarLBraceVar(IdentMeta),
    DollarVar(IdentMeta),
    Dot(Dot),
    DotDotDot(DotDotDot),
    Equal(Equal),
    EqualEqual(EqualEqual),
    Exclamation(Exclamation),
    ExclamationEqual(ExclamationEqual),
    GreaterThan(GreaterThan),
    GreaterThanEqual(GreaterThanEqual),
    Hash(HashMeta),
    HashLBrace(HashLBrace),
    Ident(IdentMeta),
    Indent(Indent),
    LBrace(LBrace),
    LBracket(LBracket),
    LessThan(LessThan),
    LessThanEqual(LessThanEqual),
    Linebreak(Linebreak),
    LParen(LParen),
    Minus(Minus),
    Number(NumberMeta),
    NumberSign(NumberSign),
    Percent(Percent),
    Percentage(PercentageMeta),
    Placeholder(PlaceholderMeta),
    Plus(Plus),
    PlusUnderscore(PlusUnderscore),
    Question(Question),
    RBrace(RBrace),
    RBracket(RBracket),
    RParen(RParen),
    Semicolon(Semicolon),
    Solidus(Solidus),
    Str(StrMeta),
    StrTemplate(StrTemplateMeta),
    Tilde(Tilde),
    TildeEqual(TildeEqual),
    Unknown(Unknown),
    UrlRaw(UrlRawMeta),
    UrlTemplate(UrlTemplateMeta),
}

pub type Token<'s> = TokenData;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "type", rename_all = "camelCase"))]
pub struct TokenWithSpanData {
    pub token: TokenData,
    pub span: Span,
}

pub type TokenWithSpan<'s> = TokenWithSpanData;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct BadStrMeta {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct BacktickCodeMeta {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct IdentMeta {
    pub escaped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct HashMeta {
    pub escaped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct NumberMeta {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct DimensionMeta {
    pub number_end: u32,
    pub unit_escaped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct PercentageMeta {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct PlaceholderMeta {
    pub index: u32,
    pub suffix_start: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct StrMeta {
    pub escaped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct StrTemplateMeta {
    pub escaped: bool,
    pub head: bool,
    pub tail: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct UrlMeta {
    pub escaped: bool,
}

pub type UrlRawMeta = UrlMeta;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct UrlTemplateMeta {
    pub escaped: bool,
    pub tail: bool,
}

impl TokenWithSpanData {
    #[inline]
    pub fn raw<'s>(&self, source: &'s str) -> &'s str {
        &source[self.span.start..self.span.end]
    }

    #[inline]
    pub fn bad_str<'s>(&self, source: &'s str) -> Option<BadStr<'s>> {
        matches!(self.token, TokenData::BadStr(..)).then(|| BadStr { raw: self.raw(source) })
    }

    #[inline]
    pub fn at_keyword<'s>(&self, source: &'s str) -> Option<AtKeyword<'s>> {
        match self.token {
            TokenData::AtKeyword(meta) => Some(AtKeyword {
                ident: Ident {
                    raw: &source[self.span.start + 1..self.span.end],
                    escaped: meta.escaped,
                },
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn at_l_brace_var<'s>(&self, source: &'s str) -> Option<AtLBraceVar<'s>> {
        match self.token {
            TokenData::AtLBraceVar(meta) => Some(AtLBraceVar {
                ident: Ident {
                    raw: &source[self.span.start + 2..self.span.end - 1],
                    escaped: meta.escaped,
                },
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn backtick_code<'s>(&self, source: &'s str) -> Option<BacktickCode<'s>> {
        matches!(self.token, TokenData::BacktickCode(..))
            .then(|| BacktickCode { raw: self.raw(source) })
    }

    #[inline]
    pub fn dimension<'s>(&self, source: &'s str) -> Option<Dimension<'s>> {
        match self.token {
            TokenData::Dimension(meta) => {
                let number_end = meta.number_end as usize;
                Some(Dimension {
                    value: Number { raw: &source[self.span.start..number_end] },
                    unit: Ident {
                        raw: &source[number_end..self.span.end],
                        escaped: meta.unit_escaped,
                    },
                })
            }
            _ => None,
        }
    }

    #[inline]
    pub fn dimension_value_raw<'s>(&self, source: &'s str) -> Option<&'s str> {
        match self.token {
            TokenData::Dimension(meta) => Some(&source[self.span.start..meta.number_end as usize]),
            _ => None,
        }
    }

    #[inline]
    pub fn dollar_l_brace_var<'s>(&self, source: &'s str) -> Option<DollarLBraceVar<'s>> {
        match self.token {
            TokenData::DollarLBraceVar(meta) => Some(DollarLBraceVar {
                ident: Ident {
                    raw: &source[self.span.start + 2..self.span.end - 1],
                    escaped: meta.escaped,
                },
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn dollar_var<'s>(&self, source: &'s str) -> Option<DollarVar<'s>> {
        match self.token {
            TokenData::DollarVar(meta) => Some(DollarVar {
                ident: Ident {
                    raw: &source[self.span.start + 1..self.span.end],
                    escaped: meta.escaped,
                },
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn hash<'s>(&self, source: &'s str) -> Option<Hash<'s>> {
        match self.token {
            TokenData::Hash(meta) => Some(Hash {
                raw: &source[self.span.start + 1..self.span.end],
                escaped: meta.escaped,
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn hash_raw<'s>(&self, source: &'s str) -> Option<&'s str> {
        matches!(self.token, TokenData::Hash(..))
            .then(|| &source[self.span.start + 1..self.span.end])
    }

    #[inline]
    pub fn ident<'s>(&self, source: &'s str) -> Option<Ident<'s>> {
        match self.token {
            TokenData::Ident(meta) => Some(Ident { raw: self.raw(source), escaped: meta.escaped }),
            _ => None,
        }
    }

    #[inline]
    pub fn ident_raw<'s>(&self, source: &'s str) -> Option<&'s str> {
        matches!(self.token, TokenData::Ident(..)).then(|| self.raw(source))
    }

    #[inline]
    pub fn is_ident_raw(&self, source: &str, expected: &str) -> bool {
        self.ident_raw(source).is_some_and(|raw| raw == expected)
    }

    #[inline]
    pub fn is_ident_raw_starts_with(&self, source: &str, prefix: &str) -> bool {
        self.ident_raw(source).is_some_and(|raw| raw.starts_with(prefix))
    }

    #[inline]
    pub fn is_ident_name_eq_ignore_ascii_case(&self, source: &str, expected: &str) -> bool {
        self.ident(source).is_some_and(|ident| ident.name().eq_ignore_ascii_case(expected))
    }

    #[inline]
    pub fn number<'s>(&self, source: &'s str) -> Option<Number<'s>> {
        matches!(self.token, TokenData::Number(..)).then(|| Number { raw: self.raw(source) })
    }

    #[inline]
    pub fn number_raw<'s>(&self, source: &'s str) -> Option<&'s str> {
        matches!(self.token, TokenData::Number(..)).then(|| self.raw(source))
    }

    #[inline]
    pub fn percentage<'s>(&self, source: &'s str) -> Option<Percentage<'s>> {
        matches!(self.token, TokenData::Percentage(..)).then(|| Percentage {
            value: Number { raw: &source[self.span.start..self.span.end - 1] },
        })
    }

    #[inline]
    pub fn placeholder<'s>(&self, source: &'s str) -> Option<Placeholder<'s>> {
        match self.token {
            TokenData::Placeholder(meta) => Some(Placeholder {
                index: meta.index,
                suffix: &source[meta.suffix_start as usize..self.span.end],
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn str<'s>(&self, source: &'s str) -> Option<Str<'s>> {
        match self.token {
            TokenData::Str(meta) => Some(Str { raw: self.raw(source), escaped: meta.escaped }),
            _ => None,
        }
    }

    #[inline]
    pub fn str_template<'s>(&self, source: &'s str) -> Option<StrTemplate<'s>> {
        match self.token {
            TokenData::StrTemplate(meta) => Some(StrTemplate {
                raw: self.raw(source),
                escaped: meta.escaped,
                head: meta.head,
                tail: meta.tail,
            }),
            _ => None,
        }
    }

    #[inline]
    pub fn url_raw<'s>(&self, source: &'s str) -> Option<UrlRaw<'s>> {
        match self.token {
            TokenData::UrlRaw(meta) => {
                Some(UrlRaw { raw: self.raw(source), escaped: meta.escaped })
            }
            _ => None,
        }
    }

    #[inline]
    pub fn url_template<'s>(&self, source: &'s str) -> Option<UrlTemplate<'s>> {
        match self.token {
            TokenData::UrlTemplate(meta) => {
                Some(UrlTemplate { raw: self.raw(source), escaped: meta.escaped, tail: meta.tail })
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Ampersand {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Asterisk {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct AsteriskEqual {}

/// CSS Syntax `<bad-string-token>`: a string terminated by a newline or EOF
/// instead of its quote. Not a tokenizer error in CSS — it is a preserved
/// token that raw component-value contexts keep verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct BadStr<'s> {
    pub raw: &'s str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct At {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct AtKeyword<'s> {
    pub ident: Ident<'s>,
}

/// An atomic backtick-delimited template placeholder token (see
/// [`ParserOptions::template_placeholder`](crate::config::ParserOptions)),
/// carrying the parsed decimal index and any glued literal suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Placeholder<'s> {
    pub index: u32,
    /// An ident-continuation run glued directly after the placeholder
    /// (`` `PLACEHOLDER-0`px `` -> index 0, suffix `"px"`), empty when none.
    /// Mirrors `#{$x}px` being a single identifier rather than two tokens.
    pub suffix: &'s str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct AtLBraceVar<'s> {
    pub ident: Ident<'s>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct BacktickCode<'s> {
    pub raw: &'s str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Bar {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct BarBar {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct BarEqual {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct CaretEqual {}

/// `-->`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Cdc {}

/// `<!--`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Cdo {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Colon {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct ColonColon {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Comma {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Dedent {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Dimension<'s> {
    pub value: Number<'s>,
    pub unit: Ident<'s>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct DollarEqual {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct DollarLBraceVar<'s> {
    pub ident: Ident<'s>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct DollarVar<'s> {
    pub ident: Ident<'s>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Dot {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct DotDotDot {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Eof {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Equal {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct EqualEqual {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Exclamation {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct ExclamationEqual {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct GreaterThan {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct GreaterThanEqual {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Hash<'s> {
    /// raw string without beginning `#` char
    pub raw: &'s str,
    pub escaped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct HashLBrace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Ident<'s> {
    pub escaped: bool,
    pub raw: &'s str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Indent {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct LBrace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct LBracket {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct LessThan {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct LessThanEqual {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Linebreak {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct LParen {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Minus {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Number<'s> {
    pub raw: &'s str,
}

/// U+0023 `#`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct NumberSign {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Percent {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Percentage<'s> {
    pub value: Number<'s>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Plus {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct PlusUnderscore {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Question {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct RBrace {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct RBracket {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct RParen {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Semicolon {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Solidus {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Str<'s> {
    pub raw: &'s str,
    pub escaped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct StrTemplate<'s> {
    pub raw: &'s str,
    pub escaped: bool,
    pub head: bool,
    pub tail: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Tilde {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct TildeEqual {}

/// Any single code point that no other token matches (a stray `^`, a control
/// character, ...). CSS Syntax calls this a `<delim-token>`: it is not an
/// error at the tokenizer level, and raw component-value contexts (custom
/// property values, unparsable declaration values) preserve it verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct Unknown {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct UrlRaw<'s> {
    pub raw: &'s str,
    pub escaped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "serialize", serde(tag = "kind", rename_all = "camelCase"))]
pub struct UrlTemplate<'s> {
    pub raw: &'s str,
    pub escaped: bool,
    pub tail: bool,
}

#[cfg(test)]
mod test {
    use super::{TokenData, TokenWithSpanData};

    const _: () = assert!(size_of::<TokenData>() <= 12);
    const _: () = assert!(size_of::<TokenWithSpanData>() <= 32);
}
