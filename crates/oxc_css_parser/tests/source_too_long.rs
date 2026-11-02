#![cfg(target_pointer_width = "64")]

use oxc_css_parser::{
    Allocator, Parser, Syntax,
    ast::*,
    error::{ErrorKind, PResult},
};

#[test]
fn source_too_long_is_an_error() {
    // Zeroed and never written, so the 4 GiB stays virtual.
    let bytes = vec![0u8; u32::MAX as usize + 1];
    // SAFETY: NUL bytes are valid UTF-8; validating would read all 4 GiB.
    let source = unsafe { std::str::from_utf8_unchecked(&bytes) };
    let allocator = Allocator::default();
    let assert_too_long = |syntax, parse: fn(&mut Parser) -> PResult<()>| {
        let error = parse(&mut Parser::new(&allocator, source, syntax)).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::SourceTooLong), "{syntax:?}: {error:?}");
    };
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Sass, Syntax::Less] {
        assert_too_long(syntax, |p| p.parse::<Stylesheet>().map(drop));
        assert_too_long(syntax, |p| p.parse::<Declaration>().map(drop));
        // Rescans in the tokenizer before reading any token.
        assert_too_long(syntax, |p| p.parse::<UrlRaw>().map(drop));
    }
    for syntax in [Syntax::Scss, Syntax::Sass] {
        assert_too_long(syntax, |p| p.parse::<SassInterpolatedUrl>().map(drop));
    }
}
