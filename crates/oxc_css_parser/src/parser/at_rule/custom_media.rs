use super::Parser;
use crate::{Parse, ast::*, error::PResult, pos::Span};

// https://www.w3.org/TR/mediaqueries-5/#custom-mq
//
// @custom-media <extension-name> [ <media-query-list> | true | false ] ;
// <extension-name> = <dashed-ident>
impl<'a> Parse<'a> for CustomMedia<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let name = input.parse_dashed_ident()?;
        let value = input.parse::<CustomMediaValue>()?;
        let span = Span { start: name.span().start, end: value.span().end };
        Ok(CustomMedia { name, value, span })
    }
}

// <custom-media-value> = <media-query-list> | true | false
impl<'a> Parse<'a> for CustomMediaValue<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        let peek = input.cursor.peek()?;
        if peek.ident(input.source).is_some() {
            if peek.is_ident_name_eq_ignore_ascii_case(input.source, "true") {
                input.parse().map(CustomMediaValue::True)
            } else if peek.is_ident_name_eq_ignore_ascii_case(input.source, "false") {
                input.parse().map(CustomMediaValue::False)
            } else {
                input.parse().map(CustomMediaValue::MediaQueryList)
            }
        } else {
            input.parse().map(CustomMediaValue::MediaQueryList)
        }
    }
}
