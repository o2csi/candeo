//! An effect's TypeScript to the JavaScript Candeo's QuickJS runs, with Oxc and
//! without a WebView: the proof of concept of #364.
//!
//! It does what `apps/desktop/src/editor/effect.ts` does with
//! `ts.transpileModule`: types stripped, the module kept an ES module so that
//! `import … from '@candeo/effects-api'` reaches the engine's built-in module,
//! and a syntax error turned into a module that throws it, located as the editor
//! locates it. On top of that, [`check_imports`] does statically what the
//! engine only finds when it links the module.
//!
//! What it does **not** do is check types: Oxc's TypeScript support is a parser
//! and a transform, not TypeScript's checker. See
//! `docs/design/headless-transpile-poc.md` §5.

use std::fmt;
use std::path::Path;

use oxc::allocator::Allocator;
use oxc::ast::ast::{
    AccessorProperty, Decorator, Program, TSExportAssignment, TSExternalModuleReference,
};
use oxc::ast_visit::{walk, Visit};
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::diagnostics::{OxcDiagnostic, Severity};
use oxc::parser::{ParseOptions, Parser};
use oxc::semantic::SemanticBuilder;
use oxc::span::{SourceType, Span};
use oxc::syntax::module_record::{ExportImportName, ImportImportName, ModuleRecord};
use oxc::transformer::{HelperLoaderMode, TransformOptions, Transformer};

/// The one module an effect may import: the engine resolves it to `api.js`.
pub const API_MODULE: &str = "@candeo/effects-api";

/// The file name given to Oxc, as `effect.ts` gives TypeScript one: it only
/// decides the language.
const FILE: &str = "effect.ts";

/// What an effect is compiled for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// What `effect.ts` asks TypeScript for. Oxc lowers ES2021+ syntax for it,
    /// and class fields then need helpers it imports from
    /// `@oxc-project/runtime`, which the engine cannot resolve.
    Es2020,
    /// No lowering at all: QuickJS-ng 0.16, in rquickjs 0.14, runs ES2023
    /// natively. What the PoC recommends.
    EsNext,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub target: Target,
    /// Also produce a source map, to bring a runtime error back to the source.
    pub source_map: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            target: Target::EsNext,
            source_map: false,
        }
    }
}

/// A problem where its author finds it: 1-based line and column, the column
/// counted in UTF-16 code units as TypeScript counts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    /// The shape `located()` in `effect.ts` gives TypeScript's diagnostics.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

/// The JavaScript, and what producing it cost.
#[derive(Debug)]
pub struct Output {
    pub js: String,
    /// The source map's segments, `[js line, js column, source line, source
    /// column]`, 0-based, when [`Options::source_map`] asked for it.
    pub mappings: Vec<[u32; 4]>,
    /// Bytes the arena reserved: the transpilation's memory, freed at return.
    pub arena_bytes: usize,
}

impl Output {
    /// Where a 1-based position of the JavaScript comes from in the source:
    /// what turns QuickJS's `at render (effect:6:14)` into the line the author
    /// wrote.
    pub fn original(&self, line: u32, column: u32) -> Option<(u32, u32)> {
        let (line, column) = (line.checked_sub(1)?, column.saturating_sub(1));
        self.mappings
            .iter()
            .rfind(|m| m[0] == line && m[1] <= column)
            .or_else(|| self.mappings.iter().find(|m| m[0] == line))
            .map(|m| (m[2] + 1, m[3] + 1))
    }
}

/// The JavaScript for this source, or every error found, in source order.
///
/// Errors are the parser's, the ECMAScript early errors a module must not
/// have (a redeclared `let`, `with`, a legacy octal literal), and the
/// transform's (TypeScript it cannot strip). Type errors are not among them.
pub fn transpile(source: &str, options: &Options) -> Result<Output, Vec<Diagnostic>> {
    let allocator = Allocator::default();
    // A module, always: the engine loads every effect as one, so the strict
    // mode rules apply even to a file without `import` or `export`.
    let source_type = SourceType::ts().with_module(true);
    let parsed = Parser::new(&allocator, source, source_type)
        // Off by default: without it `/(?<a>.)(?<a>.)/` passes, and QuickJS
        // refuses the module with no position.
        .with_options(ParseOptions {
            parse_regular_expression: true,
            ..ParseOptions::default()
        })
        .parse();
    let mut errors = errors_of(source, &parsed.diagnostics);
    if parsed.fatal_error || !errors.is_empty() {
        return Err(sorted(errors));
    }
    let mut program = parsed.program;

    // TypeScript's CommonJS forms: TypeScript drops them without a word for an
    // ES module (`import fs = require('fs')` leaves `fs` undefined), Oxc turns
    // them into `require` and `module.exports`, which the engine does not
    // have. Refused here, where they are written.
    errors.extend(Refused::in_source(&program).located(source));

    let semantic = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .with_excess_capacity(2.0)
        .build(&program);
    errors.extend(errors_of(source, &semantic.diagnostics));
    if !errors.is_empty() {
        return Err(sorted(errors));
    }
    let scoping = semantic.semantic.into_scoping();

    let mut transform = match options.target {
        Target::Es2020 => TransformOptions::from_target("es2020"),
        Target::EsNext => TransformOptions::from_target("esnext"),
    }
    .expect("a target Oxc knows");
    // TypeScript's `useDefineForClassFields: false`, its default for the
    // ES2020 target `effect.ts` asks for: a field is assigned in the
    // constructor, after the parameter properties, and one without an
    // initialiser is dropped. Without this, `level = this.base * 2` next to
    // `constructor(readonly base: number)` is NaN with Oxc and 100 with
    // TypeScript (`cases/adversarial/Class fields - …`).
    transform.assumptions.set_public_class_fields = true;
    transform.typescript.remove_class_fields_without_initializer = true;
    // Runtime (the default) imports helpers from `@oxc-project/runtime`;
    // Inline is not implemented in 0.157. External at least names them on one
    // global, which says what is missing instead of failing to link.
    transform.helper_loader.mode = HelperLoaderMode::External;
    let transformed = Transformer::new(&allocator, Path::new(FILE), &transform)
        .build_with_scoping(scoping, &mut program);
    errors.extend(errors_of(source, &transformed.diagnostics));
    // What Oxc leaves untransformed and QuickJS-ng cannot parse: decorators
    // and `accessor`, which TypeScript lowers. Their spans are still the
    // source's.
    errors.extend(Refused::in_output(&program).located(source));
    if !errors.is_empty() {
        return Err(sorted(errors));
    }

    let codegen = Codegen::new()
        .with_options(CodegenOptions {
            source_map_path: options.source_map.then(|| Path::new(FILE).to_path_buf()),
            ..CodegenOptions::default()
        })
        .build(&program);
    let mappings = codegen.map.map_or_else(Vec::new, |map| {
        map.get_tokens()
            .map(|t| {
                [
                    t.get_dst_line(),
                    t.get_dst_col(),
                    t.get_src_line(),
                    t.get_src_col(),
                ]
            })
            .collect()
    });
    Ok(Output {
        js: codegen.code,
        mappings,
        arena_bytes: allocator.capacity(),
    })
}

/// Constructs found by walking a program, each with the reason it is refused.
#[derive(Default)]
struct Refused {
    output: bool,
    found: Vec<(Span, &'static str)>,
}

impl Refused {
    fn in_source(program: &Program) -> Self {
        let mut v = Self::default();
        v.visit_program(program);
        v
    }

    fn in_output(program: &Program) -> Self {
        let mut v = Self {
            output: true,
            ..Self::default()
        };
        v.visit_program(program);
        v
    }

    fn located(self, source: &str) -> Vec<Diagnostic> {
        self.found
            .into_iter()
            .map(|(span, message)| {
                let (line, column) = line_column(source, span.start as usize);
                Diagnostic {
                    line,
                    column,
                    message: message.to_string(),
                }
            })
            .collect()
    }
}

impl<'a> Visit<'a> for Refused {
    fn visit_ts_external_module_reference(&mut self, it: &TSExternalModuleReference<'a>) {
        if !self.output {
            self.found.push((
                it.span,
                "`import … = require(…)` is CommonJS: write `import { … } from '@candeo/effects-api'`",
            ));
        }
    }

    fn visit_ts_export_assignment(&mut self, it: &TSExportAssignment<'a>) {
        if !self.output {
            self.found.push((
                it.span,
                "`export =` is CommonJS: write `export default defineEffect({ … })`",
            ));
        }
        walk::walk_ts_export_assignment(self, it);
    }

    fn visit_decorator(&mut self, it: &Decorator<'a>) {
        if self.output {
            self.found.push((
                it.span,
                "decorators are not supported by the effects engine",
            ));
        }
    }

    fn visit_accessor_property(&mut self, it: &AccessorProperty<'a>) {
        if self.output {
            self.found.push((
                it.span,
                "`accessor` fields are not supported by the effects engine: write a getter and a setter",
            ));
        }
        walk::walk_accessor_property(self, it);
    }
}

/// What `effect.ts` hands the engine: the JavaScript, or a module that throws
/// the first error, so that the library records an effect that does not load
/// and says why (#226).
pub fn as_module(source: &str, options: &Options) -> String {
    match transpile(source, options) {
        Ok(output) => output.js,
        Err(errors) => throw_module(&errors[0]),
    }
}

/// The module `effect.ts` writes for a syntax error.
pub fn throw_module(error: &Diagnostic) -> String {
    format!(
        "throw new SyntaxError({})\n",
        json_string(&error.to_string())
    )
}

/// The names a module exports, read from its syntax: for `api.js`, what an
/// effect may import, without a list kept by hand next to it.
pub fn exports_of(js: &str) -> Vec<String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, js, SourceType::mjs()).parse();
    let mut names: Vec<String> = parsed
        .module_record
        .local_export_entries
        .iter()
        .filter_map(|e| match &e.export_name {
            oxc::syntax::module_record::ExportExportName::Name(n) => Some(n.name.to_string()),
            oxc::syntax::module_record::ExportExportName::Default(_) => Some("default".into()),
            oxc::syntax::module_record::ExportExportName::Null => None,
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// What the engine would refuse when it links the module, found before it
/// runs: an import of anything but [`API_MODULE`], a name the API does not
/// export, a dynamic `import()`.
///
/// Read from the **JavaScript**, since only what survives type stripping is
/// imported at run time — `import { Rgb }` used as a type alone is gone — and
/// located in the **source**, where the author reads it.
pub fn check_imports(source: &str, js: &str, api_exports: &[String]) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let in_source = Parser::new(&allocator, source, SourceType::ts().with_module(true)).parse();
    let in_js = Parser::new(&allocator, js, SourceType::mjs()).parse();
    let source_record = &in_source.module_record;
    let record = &in_js.module_record;
    let at = |offset: u32, message: String| {
        let (line, column) = line_column(source, offset as usize);
        Diagnostic {
            line,
            column,
            message,
        }
    };
    let mut problems = Vec::new();

    for (module, _) in record.requested_modules.iter() {
        if module.as_str() != API_MODULE {
            let offset = first_request(source_record, module.as_str()).unwrap_or(0);
            problems.push(at(
                offset,
                format!(
                    "“{module}” cannot be imported: an effect imports from '{API_MODULE}' only"
                ),
            ));
        }
    }
    for entry in record.import_entries.iter() {
        if entry.module_request.name.as_str() != API_MODULE {
            continue;
        }
        let wanted = match &entry.import_name {
            ImportImportName::Name(name) => name.name.to_string(),
            ImportImportName::Default(_) => "default".to_string(),
            ImportImportName::NamespaceObject => continue,
        };
        if !api_exports.contains(&wanted) {
            let offset = source_record
                .import_entries
                .iter()
                .find(|e| e.local_name.name.as_str() == entry.local_name.name.as_str())
                .map_or(0, |e| e.local_name.span.start);
            problems.push(at(
                offset,
                if wanted == "default" {
                    format!(
                        "'{API_MODULE}' has no default export: import {{ … }} or import * as api"
                    )
                } else {
                    format!("'{API_MODULE}' exports no “{wanted}”")
                },
            ));
        }
    }
    // `export { Rgb } from '@candeo/effects-api'`: a file compiled alone
    // cannot tell a type from a value, so the re-export stays, and links to
    // nothing.
    for entry in record.indirect_export_entries.iter() {
        let (Some(module), ExportImportName::Name(name)) =
            (&entry.module_request, &entry.import_name)
        else {
            continue;
        };
        if module.name.as_str() == API_MODULE
            && !api_exports.iter().any(|e| e == name.name.as_str())
        {
            let offset = source_record
                .indirect_export_entries
                .iter()
                .find_map(|e| match &e.import_name {
                    ExportImportName::Name(n) if n.name.as_str() == name.name.as_str() => {
                        Some(n.span.start)
                    }
                    _ => None,
                })
                .unwrap_or(0);
            problems.push(at(
                offset,
                format!(
                    "'{API_MODULE}' exports no “{}”: a type is re-exported with `export type {{ … }}`",
                    name.name
                ),
            ));
        }
    }
    for (i, _) in record.dynamic_imports.iter().enumerate() {
        let offset = source_record
            .dynamic_imports
            .get(i)
            .map_or(0, |d| d.span.start);
        problems.push(at(
            offset,
            format!("import() is not supported: import from '{API_MODULE}' at the top of the file"),
        ));
    }
    sorted(problems)
}

fn first_request(record: &ModuleRecord, module: &str) -> Option<u32> {
    record
        .requested_modules
        .iter()
        .find(|(name, _)| name.as_str() == module)
        .and_then(|(_, requests)| requests.first())
        .map(|r| r.span.start)
}

fn errors_of(source: &str, diagnostics: &[OxcDiagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| {
            // The primary label when there is one: where Oxc points, as the
            // terminal rendering underlines it.
            let offset = d
                .labels
                .iter()
                .find(|l| l.primary())
                .or_else(|| d.labels.first())
                .map_or(0, |l| l.offset() as usize);
            let (line, column) = line_column(source, offset);
            let mut message = d.message.to_string();
            if let Some(help) = &d.help {
                message.push_str(&format!(" ({help})"));
            }
            Diagnostic {
                line,
                column,
                message,
            }
        })
        .collect()
}

fn sorted(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by_key(|d| (d.line, d.column));
    diagnostics
}

/// 1-based line and column of a byte offset, as TypeScript's
/// `getLineAndCharacterOfPosition` counts them: lines broken by `\n`, `\r\n`,
/// `\r`, U+2028 and U+2029, columns in UTF-16 code units. Oxc's spans are
/// UTF-8 byte offsets: an accent earlier on the line would otherwise shift the
/// column.
pub fn line_column(source: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(source.len());
    let before = &source[..floor_char_boundary(source, offset)];
    let mut line = 1u32;
    let mut column = 0u32;
    let mut chars = before.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                column = 0;
            }
            '\n' | '\u{2028}' | '\u{2029}' => {
                line += 1;
                column = 0;
            }
            c => column += c.len_utf16() as u32,
        }
    }
    (line, column + 1)
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const API_EXPORTS: &[&str] = &["defineEffect", "hsv", "rgb"];

    fn exports() -> Vec<String> {
        API_EXPORTS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn strips_types_and_keeps_the_api_import_an_import() {
        let js = transpile(
            "import { defineEffect, rgb, type Rgb } from '@candeo/effects-api'\n\
             const level: number = 128\n\
             const red: Rgb = rgb(level, 0, 0)\n\
             export default defineEffect({ render({ layout, frame }) { for (const key of layout.keys) frame.set(key, red) } })\n",
            &Options::default(),
        )
        .unwrap()
        .js;
        assert!(js.contains("from \"@candeo/effects-api\""), "{js}");
        assert!(!js.contains("Rgb"), "the type import is gone: {js}");
        assert!(!js.contains(": number"), "{js}");
        assert!(js.contains("export default defineEffect("), "{js}");
    }

    /// The case `effect.test.ts` holds for TypeScript: a parenthesis missing.
    #[test]
    fn a_syntax_error_is_located_and_makes_a_throwing_module() {
        let source = [
            "import { defineEffect } from '@candeo/effects-api'",
            "export default defineEffect({",
            "  render({ layout, frame }) {",
            "    for (const key of layout.keys frame.set(key, { r: 0, g: 0, b: 0 })",
            "  },",
            "})",
        ]
        .join("\n");
        let errors = transpile(&source, &Options::default()).unwrap_err();
        assert_eq!((errors[0].line, errors[0].column), (4, 35), "{errors:?}");
        assert!(as_module(&source, &Options::default()).starts_with("throw new SyntaxError("));
    }

    #[test]
    fn columns_count_utf16_units_like_typescript() {
        // 'é' is two bytes in UTF-8, one unit in UTF-16; '😀' four and two.
        assert_eq!(line_column("é = 1", 2), (1, 2));
        assert_eq!(line_column("😀x", 4), (1, 3));
        assert_eq!(line_column("a\r\nb", 3), (2, 1));
        assert_eq!(line_column("a\rb", 2), (2, 1));
    }

    #[test]
    fn imports_other_than_the_api_are_found_where_written() {
        let source = "import { defineEffect } from '@candeo/effects-api'\nimport _ from 'lodash'\nexport default defineEffect({ render() { _.noop() } })\n";
        let js = transpile(source, &Options::default()).unwrap().js;
        let problems = check_imports(source, &js, &exports());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!((problems[0].line, problems[0].column), (2, 15));
    }

    #[test]
    fn a_name_the_api_does_not_export_is_found() {
        let source = "import { defineEffect, sparkle } from '@candeo/effects-api'\nexport default defineEffect({ render() { sparkle() } })\n";
        let js = transpile(source, &Options::default()).unwrap().js;
        let problems = check_imports(source, &js, &exports());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].message.contains("sparkle"));
        assert_eq!((problems[0].line, problems[0].column), (1, 24));
    }

    #[test]
    fn a_value_import_used_only_as_a_type_is_not_imported_at_run_time() {
        let source = "import { defineEffect, Rgb } from '@candeo/effects-api'\nconst c: Rgb = { r: 0, g: 0, b: 0 }\nexport default defineEffect({ render({ frame }) { frame.fill(c) } })\n";
        let js = transpile(source, &Options::default()).unwrap().js;
        assert!(check_imports(source, &js, &exports()).is_empty(), "{js}");
    }

    /// TypeScript's ES2020 output assigns the parameter property, then the
    /// field: the order a native class field would reverse.
    #[test]
    fn class_fields_follow_typescripts_es2020_semantics() {
        let js = transpile(
            "class Pulse {\n  level = this.base * 2\n  unset: number\n  constructor(readonly base: number) {}\n}\nexport default new Pulse(50)\n",
            &Options::default(),
        )
        .unwrap()
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

    #[test]
    fn what_the_engine_cannot_parse_is_refused_where_written() {
        let errors = transpile(
            "function f(c: unknown, _: unknown) { return c }\n@f class A {}\nexport default A\n",
            &Options::default(),
        )
        .unwrap_err();
        assert_eq!((errors[0].line, errors[0].column), (2, 1), "{errors:?}");
        let errors = transpile(
            "import fs = require('fs')\nexport default fs\n",
            &Options::default(),
        )
        .unwrap_err();
        assert_eq!((errors[0].line, errors[0].column), (1, 13), "{errors:?}");
    }

    #[test]
    fn regular_expressions_are_checked() {
        let errors =
            transpile("export default /(?<a>.)(?<a>.)/\n", &Options::default()).unwrap_err();
        assert_eq!(errors[0].line, 1, "{errors:?}");
    }

    #[test]
    fn a_runtime_position_maps_back_to_the_source() {
        let source = "interface A { x: number }\n\ntype B = A\n\nexport default function f(a: A) {\n  return a.x.y.z\n}\n";
        let out = transpile(
            source,
            &Options {
                source_map: true,
                ..Options::default()
            },
        )
        .unwrap();
        let line = out.js.lines().position(|l| l.contains("a.x.y.z")).unwrap() as u32 + 1;
        assert_eq!(out.original(line, 2).map(|p| p.0), Some(6), "{}", out.js);
    }

    #[test]
    fn the_exports_of_a_module_are_read_from_its_syntax() {
        let names = exports_of("export const A = 1\nexport function b() {}\nexport default {}\n");
        assert_eq!(names, ["A", "b", "default"]);
    }
}
