//! The contract, from outside: what an effect compiles to, and where each
//! problem is reported. Positions are those `docs/design/headless-transpile-poc.md`
//! §2 measured, on the same cases.

use candeo_compiler::{compile, Api, Compiled, Diagnostic};

/// The names `api.js` exports that these cases use.
fn api() -> Api {
    Api::from_module(
        "export const BLACK = {}\n\
         export function defineEffect(e) { return e }\n\
         export function hsv() {}\n\
         export function rgb() {}\n",
    )
}

fn lines(lines: &[&str]) -> String {
    lines.join("\n") + "\n"
}

fn compiled(source: &str) -> Compiled {
    compile(source, &api()).unwrap_or_else(|e| panic!("{e:?}"))
}

fn refused(source: &str) -> Vec<Diagnostic> {
    match compile(source, &api()) {
        Ok(out) => panic!("compiled:\n{}", out.js),
        Err(errors) => errors,
    }
}

/// Where the first problem is, and its message.
fn first(source: &str) -> (u32, u32, String) {
    let e = &refused(source)[0];
    (e.line, e.column, e.message.clone())
}

#[test]
fn types_are_stripped_and_the_api_import_stays_an_import() {
    let js = compiled(&lines(&[
        "import { defineEffect, rgb, type Rgb } from '@candeo/effects-api'",
        "const level: number = 128",
        "const red: Rgb = rgb(level, 0, 0)",
        "export default defineEffect({",
        "  render({ layout, frame }) {",
        "    for (const key of layout.keys) frame.set(key, red)",
        "  },",
        "})",
    ]))
    .js;
    // The engine's loader resolves the import to its built-in module: a
    // `require` would not load.
    assert!(js.contains("from \"@candeo/effects-api\""), "{js}");
    assert!(!js.contains("require("), "{js}");
    assert!(!js.contains("Rgb"), "the type import is gone: {js}");
    assert!(!js.contains(": number"), "{js}");
    assert!(js.contains("export default defineEffect("), "{js}");
}

/// #226: the window's TypeScript repaired this loop without a word, and the
/// effect ran as its guess.
#[test]
fn a_syntax_error_is_reported_where_typescript_reported_it() {
    let source = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "export default defineEffect({",
        "  render({ layout, frame }) {",
        "    for (const key of layout.keys frame.set(key, { r: 0, g: 0, b: 0 })",
        "  },",
        "})",
    ]);
    let (line, column, message) = first(&source);
    assert_eq!((line, column), (4, 35));
    assert!(message.contains("Expected `)`"), "{message}");
    // The same with Windows line endings.
    let (line, column, _) = first(&source.replace('\n', "\r\n"));
    assert_eq!((line, column), (4, 35));
}

#[test]
fn a_column_after_accents_and_an_emoji_counts_as_monaco_counts() {
    let source = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "",
        "// The error sits after non-ASCII characters on its line.",
        "",
        "const étiquette = { déjà: 'vu 😀' ) }",
        "export default defineEffect({ render({ frame }) { frame.fill(étiquette) } })",
    ]);
    let (line, column, _) = first(&source);
    assert_eq!((line, column), (5, 35));
}

#[test]
fn early_errors_an_engine_refuses_are_located() {
    let redeclared = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "",
        "// Valid syntax, refused before the module runs.",
        "let level = 64",
        "let level = 128",
        "export default defineEffect({ render({ frame }) { frame.fill({ r: level, g: 0, b: 0 }) } })",
    ]);
    let (line, column, _) = first(&redeclared);
    assert_eq!((line, column), (5, 5));
    let with = lines(&[
        "const colour = { r: 1, g: 2, b: 3 }",
        "export default {",
        "  render() {",
        "    with (colour) {}",
        "  },",
        "}",
    ]);
    assert_eq!((first(&with).0, first(&with).1), (4, 5));
    let groups = "export default /(?<digit>\\d)(?<digit>\\d)/\n";
    assert_eq!(first(groups).0, 1);
}

#[test]
fn class_fields_follow_typescripts_es2020_semantics() {
    // A field initialised from a parameter property: the parameter property is
    // assigned first, then the field, as TypeScript compiled it for ES2020. A
    // native field would run first and read `this.base` undefined.
    let js = compiled(&lines(&[
        "class Pulse {",
        "  level = this.base * 2",
        "  unset: number",
        "  constructor(readonly base: number) {}",
        "}",
        "export default new Pulse(50)",
    ]))
    .js;
    let base = js
        .find("this.base = base")
        .expect("parameter property assigned");
    let level = js
        .find("this.level = this.base * 2")
        .expect("field assigned");
    assert!(base < level, "{js}");
    assert!(
        !js.contains("unset"),
        "a field without initialiser is dropped: {js}"
    );
}

/// TypeScript gives a numeric member a reverse mapping, `Mode[1] === 'Slow'`,
/// and a string member none: `Mode.fast` stays undefined, for `Quick`, whose
/// value is another member's, too. Without the semantic pass's enum values,
/// Oxc mapped `Quick` back, which a release build does not say.
#[test]
fn a_string_enum_member_has_no_reverse_mapping() {
    let js =
        compiled("enum Mode { Fast = 'fast', Slow = 1, Quick = Fast }\nexport default Mode\n").js;
    assert!(js.contains(r#"Mode["Fast"] = "fast""#), "{js}");
    assert!(js.contains(r#"Mode[Mode["Slow"] = 1] = "Slow""#), "{js}");
    assert!(js.contains(r#"Mode["Quick"] = "fast""#), "{js}");
    assert!(!js.contains(r#"Mode[Mode["Quick"]"#), "{js}");
}

#[test]
fn nothing_is_lowered() {
    // QuickJS-ng runs these as they are; lowering them for ES2020 would need
    // helpers the engine cannot import.
    let js = compiled(&lines(&[
        "class Counter { #n = 0; next() { return ++this.#n } }",
        "let a: number | undefined",
        "a ??= 1",
        "export default new Counter()",
    ]))
    .js;
    assert!(js.contains("#n"), "{js}");
    assert!(js.contains("??="), "{js}");
    assert!(!js.contains("babelHelpers"), "{js}");
}

#[test]
fn what_the_engine_cannot_run_is_refused_where_it_is_written() {
    let decorated = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "function sealed(cls: unknown, _context: ClassDecoratorContext) {}",
        "@sealed",
        "class Level {}",
        "export default defineEffect({ render({ frame }) { frame.fill({ r: 0, g: 0, b: 0 }) } })",
    ]);
    let (line, column, message) = first(&decorated);
    assert_eq!((line, column), (3, 1));
    assert!(
        message.contains("decorators are not supported"),
        "{message}"
    );

    let parameter = lines(&[
        "function inject(_target: object, _key: string | undefined, _index: number) {}",
        "class Level {",
        "  constructor(@inject readonly value: number) {}",
        "}",
        "export default new Level(1)",
    ]);
    assert_eq!((first(&parameter).0, first(&parameter).1), (3, 15));

    let accessor = lines(&[
        "class Level {",
        "  accessor value = 60",
        "}",
        "export default new Level()",
    ]);
    let (line, column, message) = first(&accessor);
    assert_eq!((line, column), (2, 3));
    assert!(message.contains("write a getter and a setter"), "{message}");
}

/// TypeScript dropped both without a word in an ES module: the effect loaded
/// with `fs` undefined, or without its export.
#[test]
fn commonjs_forms_are_refused_with_the_form_to_write() {
    let import = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "// TypeScript's CommonJS import form.",
        "import fs = require('fs')",
        "export default defineEffect({ description: typeof fs, render() {} })",
    ]);
    let (line, column, message) = first(&import);
    assert_eq!((line, column), (3, 13));
    assert!(
        message.contains("import { … } from '@candeo/effects-api'"),
        "{message}"
    );

    let export = lines(&[
        "// TypeScript's CommonJS export form.",
        "export = { render() {} }",
    ]);
    let (line, column, message) = first(&export);
    assert_eq!((line, column), (2, 1));
    assert!(message.contains("export default defineEffect"), "{message}");
}

#[test]
fn an_import_the_engine_cannot_resolve_is_located() {
    let other = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "import _ from 'lodash'",
        "export default defineEffect({ render() { _.noop() } })",
    ]);
    let (line, column, message) = first(&other);
    assert_eq!((line, column), (2, 15));
    assert!(message.contains("“lodash” cannot be imported"), "{message}");

    let unknown = lines(&[
        "import { defineEffect, sparkle } from '@candeo/effects-api'",
        "export default defineEffect({ render() { sparkle() } })",
    ]);
    let (line, column, message) = first(&unknown);
    assert_eq!((line, column), (1, 24));
    assert!(message.contains("exports no “sparkle”"), "{message}");

    let default = lines(&[
        "import api from '@candeo/effects-api'",
        "export default api.defineEffect({ render() {} })",
    ]);
    let (line, column, message) = first(&default);
    assert_eq!((line, column), (1, 8));
    assert!(message.contains("no default export"), "{message}");

    let type_as_value = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "export { Rgb } from '@candeo/effects-api'",
        "export default defineEffect({ render() {} })",
    ]);
    let (line, column, message) = first(&type_as_value);
    assert_eq!((line, column), (2, 10));
    assert!(message.contains("export type"), "{message}");

    let dynamic = lines(&[
        "import { defineEffect } from '@candeo/effects-api'",
        "const fs = await import('node:fs')",
        "export default defineEffect({ description: typeof fs, render() {} })",
    ]);
    let (line, column, message) = first(&dynamic);
    assert_eq!((line, column), (2, 18));
    assert!(message.contains("import() is not supported"), "{message}");
}

#[test]
fn what_is_imported_only_as_a_type_is_not_checked_against_the_api() {
    // `Rgb` is a type: gone from the JavaScript, so nothing links to it.
    compiled(&lines(&[
        "import { defineEffect, Rgb } from '@candeo/effects-api'",
        "import * as api from '@candeo/effects-api'",
        "const c: Rgb = api.BLACK",
        "export default defineEffect({ render({ frame }) { frame.fill(c) } })",
    ]));
}

#[test]
fn type_errors_are_not_reported() {
    // The editor's language service reports them; neither the window's
    // TypeScript nor Oxc ever did when compiling.
    compiled(&lines(&[
        "import { defineEffect, hsv } from '@candeo/effects-api'",
        "const n: number = 'x'",
        "export default defineEffect({ render({ frame }) { frame.fill(hsv('red', 1, 1)) } })",
    ]));
}

/// QuickJS reports positions in the JavaScript; the map brings them back to
/// the line the author wrote.
#[test]
fn the_source_map_leads_back_to_the_source() {
    let source = lines(&[
        "interface Cell { readonly row: number }",
        "",
        "type Grid = Cell[][]",
        "",
        "const grid: Grid = []",
        "",
        "export default function f(row: number) {",
        "  return grid[row]![0]!.row",
        "}",
    ]);
    let out = compiled(&source);
    let js_line = out
        .js
        .lines()
        .position(|l| l.contains("grid[row]"))
        .expect("the line is in the output") as u32;
    assert!(js_line < 7, "types took lines in the output:\n{}", out.js);
    let map = oxc_sourcemap::SourceMap::from_json_string(&out.source_map).unwrap();
    let table = map.generate_lookup_table();
    let token = map
        .lookup_token(&table, js_line, 2)
        .expect("a mapping on that line");
    assert_eq!(token.get_src_line() + 1, 8, "{}", out.source_map);
    assert_eq!(map.get_sources().collect::<Vec<_>>(), ["effect.ts"]);
}

#[test]
fn a_byte_order_mark_is_not_an_error() {
    compiled("\u{feff}export default { render() {} }\n");
}

#[test]
fn every_problem_is_returned_in_source_order() {
    let errors = refused(&lines(&[
        "import _ from 'lodash'",
        "import { sparkle } from '@candeo/effects-api'",
        "export default { render() { _(sparkle) } }",
    ]));
    let at: Vec<_> = errors.iter().map(|e| (e.line, e.column)).collect();
    assert_eq!(at, [(1, 15), (2, 10)], "{errors:?}");
    assert_eq!(
        errors[0].to_string(),
        format!("line 1, column 15: {}", errors[0].message)
    );
}
