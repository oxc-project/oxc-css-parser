# oxc-css-parser

This project is a fork of [g-plane/raffia](https://github.com/g-plane/raffia) for Oxfmt.

[![Crates.io](https://img.shields.io/crates/v/oxc-css-parser?style=flat-square)](https://crates.io/crates/oxc-css-parser)
[![docs.rs](https://img.shields.io/docsrs/oxc-css-parser?style=flat-square)](https://docs.rs/oxc-css-parser)

`oxc-css-parser` parses CSS, SCSS, Sass, and Less. It produces an AST and does not compile preprocessor syntax to CSS.

## Example

```rust
use oxc_css_parser::{Allocator, Parser, Syntax, ast::Stylesheet};

let allocator = Allocator::default();
let mut parser = Parser::new(&allocator, "a { color: green }", Syntax::Css);
let ast = parser.parse::<Stylesheet>().unwrap();
println!("{:#?}", ast);
```

More examples are available in [`examples`](https://github.com/oxc-project/oxc-css-parser/tree/main/examples).

For detailed API documentation, see [docs.rs](https://docs.rs/oxc-css-parser).

## Acceptance

Why there is a line to draw: css-syntax-3 alone rejects the postcss-plugin CSS real projects are full of (`*zoom`, `$var`, `--color-*`, `x: { }`),
while postcss itself is a tokenizer plus a statement splitter, so following its acceptance wholesale means an AST of strings.
Leniency never reaches below a statement: selectors, values and preludes are typed or raw, never re-parsed strings.

Each line has one grammar owner. postcss is never one, and neither is "Prettier prints it".

Changing acceptance is additive: it only turns errors into parses, with a comment citing the owner, a test pinning the strict shape, and the conformance snapshot flip in the PR.
Changing how already-accepted input is represented is a bug fix or a refactor, and the PR says which.

### CSS

Owner: css-syntax-3's syntax layer (tokenizer, rule / declaration / block structure).

- A declaration value is any component-value run: a value the typed grammar cannot read falls back to raw tokens
- Two postcss statement shapes on top, because the spec's syntax layer drops them and real projects run through postcss plugins:
  - postcss property names: the glued token run up to the first `:` / whitespace / comment, not necessarily an `<ident-token>`
    (`*zoom`, `+color`, `#x`, `2xl`, `background+`, `--color-*`, `$(var)-size`)
  - raw-prelude rules: `x: { ... }` is a rule, with or without a trailing `;`, and so is a numeric-led statement (`50% { }` outside `@keyframes`);
    the prelude is kept as raw tokens (`UnknownQualifiedRule`)
- `$var` gets a typed node (`PostcssSimpleVar`, postcss-simple-vars): an AST shape for the formatter's layout, not extra acceptance,
  `$var: value` is already a postcss property name. `$var: value;` at the root is a statement, that is where postcss-simple-vars defines variables
- Root declarations: the `TopLevelDeclaration` recoverable error. postcss keeps them; not followed
- Everything else the spec's syntax layer discards stays rejected (`color red;`, `x: {a:b} more;`),
  and so do shapes the spec keeps but nobody asked for (`"foo" {}`, `( ) {}`, `, .a {}`): the spec is a ceiling, not a floor
- Errors: css-syntax-3 recovery (EOF closes blocks, bad strings) is kept even where postcss throws
- Oracles: `cssparser` (Servo) for the spec ceiling, postcss at Prettier's pinned version for the two shapes

### SCSS / Sass

Owner: dart-sass. No postcss-scss leniency.

- No raw fallback for a normal property's value: the expression grammar owns it
- A custom property value is text, as dart-sass reads it (`//` is no comment inside)
- Root declarations: the `TopLevelDeclaration` recoverable error, as dart-sass rejects them
- The IE `*color` hack is kept as a name prefix, as dart-sass accepts it

### Less

Owner: less.js. No postcss-less leniency.

- No raw fallback for a normal property's value
- Root declarations parse: less.js accepts them at parse time and fails only at eval
- The IE `*color` hack as a name prefix and digit-only names (`5: x`), as less.js accepts them

### css-in-js parse mode

SCSS with `template_placeholder` set. It is the only option, and the only place a dialect line is relaxed.

- A backtick-delimited `` `<prefix><digits>` `` is one typed `Token::Placeholder`
- Root declarations are statements, without the error: a fragment is usually a declaration list (`` css`display: flex;` ``)
- Nothing from the CSS line comes along; postcss property names extend here only on demand

## Benchmark

The benchmark suite compares parser performance against other CSS parsers.

Install `cargo-criterion`, then run the checked-in fixture benchmark:

```sh
cargo install cargo-criterion
cargo criterion
```

To benchmark custom inputs, add CSS, SCSS, Sass, or Less files to a local `bench_data`
directory. When `bench_data` contains supported files, it is used instead of the
checked-in fixtures.

## Credits

Tests come from:

- [Web Platform Tests](https://github.com/web-platform-tests/wpt)
- [SWC CSS parser](https://github.com/swc-project/swc/tree/main/crates/swc_css_parser/tests)
- [ESBuild](https://github.com/evanw/esbuild/blob/master/internal/css_parser/css_parser_test.go)
- [Sass Spec Suite](https://github.com/sass/sass-spec)
- [Less Test Suite](https://github.com/less/less.js/tree/master/packages/test-data)

## License

MIT License

# [Sponsored By](https://oxc.rs/sponsor)

<p align="center">
  <a href="https://oxc.rs/sponsor">
    <img src="https://raw.githubusercontent.com/oxc-project/sponsors/main/sponsors.svg" alt="Our sponsors" />
  </a>
</p>
