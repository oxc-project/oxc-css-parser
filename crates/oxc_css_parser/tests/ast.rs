use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFile,
    term,
};
use insta::glob;
use oxc_css_parser::{Allocator, Parser, Syntax, ast::Stylesheet};
use std::fs;

#[test]
fn ast_parse() {
    glob!("ast/**/*.{css,scss,sass,less}", |path| {
        let code = fs::read_to_string(path).unwrap();
        let syntax = match path.extension().unwrap().to_str().unwrap() {
            "css" => Syntax::Css,
            "scss" => Syntax::Scss,
            "sass" => Syntax::Sass,
            "less" => Syntax::Less,
            _ => unreachable!("unknown file extension"),
        };
        let allocator = Allocator::default();
        let mut parser = Parser::new(&allocator, &code, syntax);
        // Assert each fixture parses cleanly (no recoverable errors); the AST is not
        // snapshotted.
        match parser.parse::<Stylesheet>() {
            Ok(_) => {
                let recoverable_errors = parser.recoverable_errors();
                assert!(
                    recoverable_errors.is_empty(),
                    "'{}' has recoverable errors: {recoverable_errors:?}",
                    path.file_name().unwrap().to_str().unwrap(),
                );
            }
            Err(error) => {
                let file = SimpleFile::new(path.file_name().unwrap().to_str().unwrap(), &code);
                let diagnostic = Diagnostic::error()
                    .with_message(error.kind.to_string())
                    .with_labels(vec![Label::primary((), error.span.start..error.span.end)]);
                let config = term::Config::default();
                let error = term::emit_into_string(&config, &file, &diagnostic).unwrap();
                panic!("\n{error}");
            }
        }
    });
}
