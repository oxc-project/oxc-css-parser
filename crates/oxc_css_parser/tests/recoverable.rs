use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFile,
    term,
};
use insta::{Settings, assert_snapshot, glob};
use oxc_css_parser::{
    Allocator, Parser, ParserBuilder, ParserOptions, Syntax,
    ast::{Statement, Stylesheet},
};
use std::fs;

#[test]
fn recoverable_errors_snapshot() {
    glob!("recoverable/**/*.{css,scss,sass,less}", |path| {
        let file_name = path.file_name().unwrap().to_str().unwrap();

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

        let file = SimpleFile::new(file_name, &code);
        let config = term::Config::default();

        // Assert the parse recovers (produces an AST with recoverable errors) rather
        // than snapshotting the AST; only the recoverable errors are snapshotted.
        let errors = match parser.parse::<Stylesheet>() {
            Ok(_) => {
                let recoverable_errors = parser.recoverable_errors();
                assert!(
                    !recoverable_errors.is_empty(),
                    "'{}' should contain recoverable errors",
                    path.display()
                );

                let mut errors = String::new();
                recoverable_errors
                    .iter()
                    .map(|error| {
                        Diagnostic::error()
                            .with_message(error.kind.to_string())
                            .with_labels(vec![Label::primary((), error.span.start..error.span.end)])
                    })
                    .for_each(|diagnostic| {
                        term::emit_to_string(&mut errors, &config, &file, &diagnostic).unwrap();
                    });
                errors
            }
            Err(error) => {
                let diagnostic = Diagnostic::error()
                    .with_message(error.kind.to_string())
                    .with_labels(vec![Label::primary((), error.span.start..error.span.end)]);
                let error = term::emit_into_string(&config, &file, &diagnostic).unwrap();
                panic!("\n{error}");
            }
        };

        let mut settings = Settings::clone_current();
        settings.set_snapshot_path(path.parent().unwrap());
        settings.remove_snapshot_suffix();
        settings.set_prepend_module_to_snapshot(false);
        settings.remove_input_file();
        settings.set_omit_expression(true);
        settings.remove_input_file();
        settings.remove_info();
        settings.bind(|| {
            assert_snapshot!(format!("{file_name}.error"), errors);
        });
    });
}

// `recoverable/declaration/top-level.css` pins the strict shape;
// a block's contents takes the same root declarations as statements (and Less its `*` hack).
#[test]
fn block_contents_takes_root_declarations() {
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
        let allocator = Allocator::default();
        let source = if syntax == Syntax::Less {
            "*zoom: 1;\na { b: c }\nd: e"
        } else {
            "color: red;\na { b: c }\nd: e"
        };
        let mut parser = ParserBuilder::new(&allocator, source)
            .syntax(syntax)
            .options(ParserOptions { block_contents: true, ..Default::default() })
            .build();
        let stylesheet = parser.parse::<Stylesheet>().unwrap();
        assert!(parser.recoverable_errors().is_empty(), "{syntax:?}");
        assert!(
            matches!(
                stylesheet.statements.as_slice(),
                [Statement::Declaration(_), Statement::QualifiedRule(_), Statement::Declaration(_)]
            ),
            "{syntax:?}"
        );
    }
}
