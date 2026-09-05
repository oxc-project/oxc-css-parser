use oxc_css_parser::{Allocator, ParserBuilder, Syntax, ast::*};

fn parse_css(code: &'static str) -> Stylesheet<'static> {
    let allocator = Box::leak(Box::new(Allocator::default()));
    ParserBuilder::new(allocator, code).syntax(Syntax::Css).build().parse::<Stylesheet>().unwrap()
}

#[test]
fn accepts_dollar_variable_declaration() {
    let ss = parse_css("$primary: red;");
    assert!(matches!(ss.statements[0], Statement::PostcssSimpleVarDeclaration(_)));
}

#[test]
fn accepts_dollar_variable_reference_in_value() {
    let ss = parse_css(".a { color: $primary; }");
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    let Statement::Declaration(decl) = &rule.block.statements[0] else {
        panic!("expected declaration");
    };
    assert!(matches!(decl.value[0], ComponentValue::PostcssSimpleVar(_)));
}

// The formatter lays the feature out (`(max-width:$bp)` gets its space), so the
// `$var` must be the typed feature value, not a `<general-enclosed>` raw fallback.
#[test]
fn dollar_variable_in_media_query_is_a_typed_feature_value() {
    let ss = parse_css("@media (max-width: $bp) { .a { color: red; } }");
    let Statement::AtRule(at_rule) = &ss.statements[0] else {
        panic!("expected at-rule");
    };
    let Some(AtRulePrelude::Media(list)) = &at_rule.prelude else {
        panic!("expected media prelude");
    };
    let MediaQuery::ConditionOnly(condition) = &list.queries[0] else {
        panic!("expected condition-only query");
    };
    let MediaConditionKind::MediaInParens(in_parens) = &condition.conditions[0] else {
        panic!("expected media-in-parens");
    };
    let MediaInParensKind::MediaFeature(feature) = &in_parens.kind else {
        panic!("expected media feature, got {:?}", in_parens.kind);
    };
    let MediaFeature::Plain(plain) = &**feature else {
        panic!("expected plain feature");
    };
    assert!(matches!(plain.value, ComponentValue::PostcssSimpleVar(_)));
}

#[test]
fn preserves_important_annotation_in_value() {
    let ss = parse_css("$primary: red !important;");
    let Statement::PostcssSimpleVarDeclaration(decl) = &ss.statements[0] else {
        panic!("expected dollar variable declaration");
    };
    assert!(!decl.value_is_raw);
    assert!(matches!(decl.value.last(), Some(ComponentValue::ImportantAnnotation(_))));
}

// The dedicated node exists for formatter layout; it must retain the same
// `<any-value>` fallback as an ordinary Css declaration.
#[test]
fn variable_declaration_falls_back_to_raw_value() {
    let ss = parse_css("$a: */; $b: .foo > bar; $c: 1px !foo;");
    assert_eq!(ss.statements.len(), 3);
    for statement in &ss.statements {
        let Statement::PostcssSimpleVarDeclaration(decl) = statement else {
            panic!("expected postcss-simple-vars declaration, got {statement:?}");
        };
        assert!(decl.value_is_raw);
        assert!(decl.value.iter().all(|value| matches!(value, ComponentValue::TokenWithSpan(..))));
    }
}

// A `$var` token may only select the dedicated node when the variable name
// ends there. Glued suffixes belong to the general postcss property name.
#[test]
fn dollar_led_postcss_property_names_use_declarations() {
    let ss = parse_css(".a { $foo+: red; $foo.bar: blue; }");
    let Statement::QualifiedRule(rule) = &ss.statements[0] else {
        panic!("expected qualified rule");
    };
    let names: Vec<_> = rule
        .block
        .statements
        .iter()
        .map(|statement| {
            let Statement::Declaration(decl) = statement else {
                panic!("expected declaration, got {statement:?}");
            };
            let InterpolableIdent::Literal(name) = &decl.name else {
                panic!("expected literal property name");
            };
            name.name
        })
        .collect();
    assert_eq!(names, ["$foo+", "$foo.bar"]);
}

#[test]
fn dollar_led_raw_prelude_rule_is_not_a_variable_declaration() {
    let ss = parse_css("$x: { color: red; }");
    assert!(matches!(ss.statements[0], Statement::UnknownQualifiedRule(..)));
}
