# AGENTS.md - AI Assistant Guide for oxc-css-parser

A fork of [g-plane/raffia](https://github.com/g-plane/raffia) parsing CSS, SCSS, Sass, and Less into an AST; preprocessor syntax is not compiled to CSS.
It is the parsing layer for oxc's CSS formatter (`oxc_formatter_css`).

## Acceptance

Why there is a line to draw: css-syntax-3 alone rejects the postcss-plugin CSS real projects are full of (`*zoom`, `$var`, `--color-*`, `x: { }`),
while postcss itself is a tokenizer plus a statement splitter, so following its acceptance wholesale means an AST of strings.
Leniency never reaches below a statement: selectors, values and preludes are typed or raw, never re-parsed strings.

Each line has one grammar owner.
postcss is never one, and neither is "Prettier prints it".

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
  `$var: value` is already a postcss property name.
  `$var: value;` at the root is a statement, that is where postcss-simple-vars defines variables
- Root declarations: the `TopLevelDeclaration` recoverable error, unless the root is a block's contents (below).
  postcss keeps them; not followed
- Everything else the spec's syntax layer discards stays rejected (`color red;`, `x: {a:b} more;`),
  and so do shapes the spec keeps but nobody asked for (`"foo" {}`, `( ) {}`, `, .a {}`): the spec is a ceiling, not a floor
- Errors: css-syntax-3 recovery (EOF closes blocks, bad strings) is kept even where postcss throws
- Oracles: `cssparser` (Servo) for the spec ceiling, postcss at Prettier's pinned version for the two shapes

### SCSS / Sass

Owner: dart-sass.
No postcss-scss leniency.

- No raw fallback for a normal property's value: the expression grammar owns it
- A custom property value is text, as dart-sass reads it (`//` is no comment inside)
- Root declarations: the `TopLevelDeclaration` recoverable error, as dart-sass rejects them, unless the root is a block's contents (below)
- The IE `*color` hack is kept as a name prefix, as dart-sass accepts it

### Less

Owner: less.js.
No postcss-less leniency.

- No raw fallback for a normal property's value
- Root declarations parse: less.js accepts them at parse time and fails only at eval
- The IE `*color` hack as a name prefix and digit-only names (`5: x`), as less.js accepts them
- Variable and property names are `[\w-]+` (`@-`, `@1`, `$-`), not CSS idents
- A string interpolates only on an exact `@{name}` / `${name}`; `"@{box"` and the outer `@{box-` of `"@{box-@{suffix}}"` are text
- An unquoted `url()` body runs to `)`, whitespace and newlines included
- A variable declaration must reach `;` / `}`: `@page :first { ... }` is an at-rule, and the permissive `@this: () => { ... };` read is tried only when no typed value parses

### Block contents

Owner: css-syntax-3's "parse a block's contents" (what a `style` attribute holds).
`block_contents` set, for a fragment rather than a stylesheet (a css-in-js template, a Markdown code block).

- The root follows the variant's rules inside a block
  - Root declarations are statements, without the error: a fragment is usually a declaration list (`` css`display: flex;` ``)
  - Less also takes the IE `*color` hack and digit-only names there

### css-in-js parse mode

SCSS with `template_placeholder` set, along with `block_contents`.
The only place a dialect line is relaxed.

- A backtick-delimited `` `<prefix><digits>` `` is one typed `Token::Placeholder`
- Nothing from the CSS line comes along; postcss property names extend here only on demand

## Architecture

- `src/parser/`: one recursive-descent parser for every syntax; dialect-specific grammar in `less.rs`, `sass.rs`, `postcss_simple_vars.rs`, at-rule preludes in `at_rule/`
- `src/ast_generated.rs`: span accessors, hand-maintained since the proc-macro derives were removed.
  A new node or variant needs its accessor here.
- `wasm/`: WebAssembly build for the playground (`wasm-pack build --target web --no-typescript wasm`)

## Testing

| Command | Covers |
| --- | --- |
| `just test` | `cargo test --all-features`: fixtures under `tests/` and per-feature tests (`tests/*.rs`) |
| `cargo insta review` | accept snapshot changes after an intentional change |
| `just conformance [suite]...` | css-parsing-tests, wpt, webref, postcss-parser-tests, sass-spec, less.js; fetches the suites itself, snapshot-pinned |
| `cargo bench -p oxc-css-parser --bench self` | parse speed over `benchmark/fixtures` (or a local `bench_data` directory) |

CI (`.github/workflows`) runs the tests, fails when the conformance snapshots are stale, and builds the wasm on push.

- `tests/ast/**`: each fixture must parse without any error; the AST is not snapshotted.
- `tests/error/**`: the fatal error is snapshotted; `tests/recoverable/**`: the recoverable errors are.
- Fixtures are read as the syntax their extension names (`.css`, `.scss`, `.sass`, `.less`).
- What the conformance columns mean, how suites are pinned and what a filtered run rewrites: the header of `tasks/conformance/src/main.rs`.
