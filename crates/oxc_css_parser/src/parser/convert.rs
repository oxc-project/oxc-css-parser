use crate::{
    Span,
    ast::{DimensionKind, Number, Placeholder},
    error::{Error, ErrorKind, PResult},
    tokenizer::token,
};

pub(super) fn dimension_kind(unit_name: &str) -> DimensionKind {
    if unit_name.eq_ignore_ascii_case("px")
        || unit_name.eq_ignore_ascii_case("em")
        || unit_name.eq_ignore_ascii_case("rem")
        || unit_name.eq_ignore_ascii_case("ex")
        || unit_name.eq_ignore_ascii_case("rex")
        || unit_name.eq_ignore_ascii_case("cap")
        || unit_name.eq_ignore_ascii_case("rcap")
        || unit_name.eq_ignore_ascii_case("ch")
        || unit_name.eq_ignore_ascii_case("rch")
        || unit_name.eq_ignore_ascii_case("ic")
        || unit_name.eq_ignore_ascii_case("ric")
        || unit_name.eq_ignore_ascii_case("lh")
        || unit_name.eq_ignore_ascii_case("rlh")
        || unit_name.eq_ignore_ascii_case("vw")
        || unit_name.eq_ignore_ascii_case("vh")
        || unit_name.eq_ignore_ascii_case("vi")
        || unit_name.eq_ignore_ascii_case("vb")
        || unit_name.eq_ignore_ascii_case("vmin")
        || unit_name.eq_ignore_ascii_case("vmax")
        || unit_name.eq_ignore_ascii_case("lvw")
        || unit_name.eq_ignore_ascii_case("lvh")
        || unit_name.eq_ignore_ascii_case("lvi")
        || unit_name.eq_ignore_ascii_case("lvb")
        || unit_name.eq_ignore_ascii_case("lvmin")
        || unit_name.eq_ignore_ascii_case("lvmax")
        || unit_name.eq_ignore_ascii_case("svw")
        || unit_name.eq_ignore_ascii_case("svh")
        || unit_name.eq_ignore_ascii_case("svi")
        || unit_name.eq_ignore_ascii_case("svb")
        || unit_name.eq_ignore_ascii_case("svmin")
        || unit_name.eq_ignore_ascii_case("svmax")
        || unit_name.eq_ignore_ascii_case("dvw")
        || unit_name.eq_ignore_ascii_case("dvh")
        || unit_name.eq_ignore_ascii_case("dvi")
        || unit_name.eq_ignore_ascii_case("dvb")
        || unit_name.eq_ignore_ascii_case("dvmin")
        || unit_name.eq_ignore_ascii_case("dvmax")
        || unit_name.eq_ignore_ascii_case("cm")
        || unit_name.eq_ignore_ascii_case("mm")
        || unit_name.eq_ignore_ascii_case("Q")
        || unit_name.eq_ignore_ascii_case("in")
        || unit_name.eq_ignore_ascii_case("pc")
        || unit_name.eq_ignore_ascii_case("pt")
    {
        DimensionKind::Length
    } else if unit_name.eq_ignore_ascii_case("deg")
        || unit_name.eq_ignore_ascii_case("grad")
        || unit_name.eq_ignore_ascii_case("rad")
        || unit_name.eq_ignore_ascii_case("turn")
    {
        DimensionKind::Angle
    } else if unit_name.eq_ignore_ascii_case("s") || unit_name.eq_ignore_ascii_case("ms") {
        DimensionKind::Duration
    } else if unit_name.eq_ignore_ascii_case("Hz") || unit_name.eq_ignore_ascii_case("kHz") {
        DimensionKind::Frequency
    } else if unit_name.eq_ignore_ascii_case("dpi")
        || unit_name.eq_ignore_ascii_case("dpcm")
        || unit_name.eq_ignore_ascii_case("dppx")
    {
        DimensionKind::Resolution
    } else if unit_name.eq_ignore_ascii_case("fr") {
        DimensionKind::Flex
    } else {
        DimensionKind::Unknown
    }
}

impl<'a> From<(token::Placeholder<'a>, Span)> for Placeholder<'a> {
    fn from((token, span): (token::Placeholder<'a>, Span)) -> Self {
        Placeholder { index: token.index, suffix: token.suffix, span }
    }
}

impl<'a> TryFrom<(token::Number<'a>, Span)> for Number<'a> {
    type Error = Error;

    fn try_from((token, span): (token::Number<'a>, Span)) -> PResult<Self> {
        token
            .raw
            .parse()
            .map_err(|_| Error { kind: ErrorKind::InvalidNumber, span })
            .map(|value| Self { value, raw: token.raw, span })
    }
}
