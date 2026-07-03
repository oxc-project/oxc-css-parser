use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

fn parse(code: &'static str, syntax: Syntax) -> Stylesheet<'static> {
    let allocator = Box::leak(Box::new(Allocator::default()));
    let mut parser = ParserBuilder::new(allocator, code).syntax(syntax).build();
    let ss = parser.parse::<Stylesheet>().unwrap();
    assert!(
        parser.recoverable_errors().is_empty(),
        "recoverable errors: {:?}",
        parser.recoverable_errors()
    );
    ss
}

fn at_rule_prelude<'a>(ss: &'a Stylesheet<'static>) -> &'a AtRulePrelude<'static> {
    let Statement::AtRule(at_rule) = &ss.statements[0] else {
        panic!("expected at-rule");
    };
    at_rule.prelude.as_ref().expect("expected prelude")
}

// A pure `'a', 'b', ...` import is a Sass multi-path import: each path stays
// typed (`SassImportPrelude`), never flattened into raw modifier tokens.
#[test]
fn scss_multi_path_import_is_sass_import_prelude() {
    let ss = parse("@import 'mixins', 'variables', 'reset';", Syntax::Scss);
    let AtRulePrelude::SassImport(import) = at_rule_prelude(&ss) else {
        panic!("expected SassImportPrelude");
    };
    assert_eq!(import.paths.len(), 3);
    assert_eq!(import.paths[0].raw, "'mixins'");
    assert_eq!(import.comma_spans.len(), 2);
}

// Comments between the paths must not break the typed-path shape
// (Prettier keeps them: `@import // Comment\n  "mixins", ...`).
#[test]
fn scss_multi_path_import_with_comments() {
    let ss = parse("@import // Comment\n  'mixins',\n  // Comment\n  'reset';", Syntax::Scss);
    let AtRulePrelude::SassImport(import) = at_rule_prelude(&ss) else {
        panic!("expected SassImportPrelude");
    };
    assert_eq!(import.paths.len(), 2);
}

// A tail that is NOT just strings keeps the lenient raw-modifiers shape.
#[test]
fn scss_non_string_tail_stays_import_modifiers() {
    let ss = parse("@import 'a' b c(d), 'e' supports(f: g);", Syntax::Scss);
    let AtRulePrelude::Import(import) = at_rule_prelude(&ss) else {
        panic!("expected ImportPrelude");
    };
    assert!(import.modifiers.is_some());
}

// A url() href cannot be a Sass path list; the comma tail stays raw.
#[test]
fn scss_url_href_with_comma_tail_stays_import_modifiers() {
    let ss = parse("@import url(a), 'b';", Syntax::Scss);
    let AtRulePrelude::Import(import) = at_rule_prelude(&ss) else {
        panic!("expected ImportPrelude");
    };
    assert!(import.modifiers.is_some());
}

// A path list followed by anything else is not a full-prelude match;
// the whole tail keeps the lenient raw-modifiers shape.
#[test]
fn scss_path_list_with_trailing_media_stays_import_modifiers() {
    let ss = parse("@import 'a', 'b' screen;", Syntax::Scss);
    let AtRulePrelude::Import(import) = at_rule_prelude(&ss) else {
        panic!("expected ImportPrelude");
    };
    assert!(import.modifiers.is_some());
}

// An interpolated href can't be a Sass path (`SassImportPrelude` takes only
// plain strings); the comma tail stays raw instead of hard-failing.
#[test]
fn scss_interpolated_href_with_comma_tail_stays_import_modifiers() {
    let ss = parse("@import \"a#{$x}\", 'b';", Syntax::Scss);
    let AtRulePrelude::Import(import) = at_rule_prelude(&ss) else {
        panic!("expected ImportPrelude");
    };
    assert!(import.modifiers.is_some());
}

// Plain CSS has no Sass import; the comma-chained tail stays raw modifiers.
#[test]
fn css_multi_path_import_stays_import_modifiers() {
    let ss = parse("@import 'one.css', 'two.css';", Syntax::Css);
    let AtRulePrelude::Import(import) = at_rule_prelude(&ss) else {
        panic!("expected ImportPrelude");
    };
    let modifiers = import.modifiers.as_ref().expect("expected modifiers");
    assert_eq!(modifiers.values.len(), 2);
}
