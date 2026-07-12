use super::Parser;
use crate::{
    Parse,
    ast::*,
    error::{Error, ErrorKind, PResult},
};

// https://www.w3.org/TR/css-color-5/#at-profile
//
// @color-profile [ <dashed-ident> | device-cmyk ] { <declaration-list> }
impl<'a> Parse<'a> for ColorProfilePrelude<'a> {
    fn parse(input: &mut Parser<'a>) -> PResult<Self> {
        // Not `parse_dashed_ident`: the predefined `device-cmyk` name is
        // valid without leading dashes and must not report its error.
        match input.parse()? {
            InterpolableIdent::Literal(ident) if ident.name.eq_ignore_ascii_case("device-cmyk") => {
                Ok(ColorProfilePrelude::DeviceCmyk(ident))
            }
            ident => {
                if let InterpolableIdent::Literal(ident) = &ident
                    && !ident.name.starts_with("--")
                {
                    input
                        .recoverable_errors
                        .push(Error { kind: ErrorKind::ExpectDashedIdent, span: ident.span });
                }
                Ok(ColorProfilePrelude::DashedIdent(ident))
            }
        }
    }
}
