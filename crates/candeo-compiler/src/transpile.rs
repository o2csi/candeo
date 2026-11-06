//! Oxc's pipeline, with the settings #364 found give the same frames as the
//! window's TypeScript on every shipped effect
//! (`docs/design/headless-transpile-poc.md` §3 and §6.1). They hold together:
//! change one, and rerun that comparison.

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
use oxc::transformer::{HelperLoaderMode, TransformOptions, Transformer};

use crate::position::{line_column, sorted};
use crate::{Compiled, Diagnostic};

/// The file name given to Oxc: it decides the language, and names the source
/// in the source map.
const FILE: &str = "effect.ts";

/// The JavaScript and its source map, or every error found, in source order.
///
/// Errors are the parser's, the ECMAScript early errors a module must not have
/// (a redeclared `let`, `with`, a legacy octal literal), and the constructs
/// the engine cannot run. Type errors are not among them.
///
/// Recurses as deep as the source nests: called on [`crate::compile`]'s
/// thread only.
pub(crate) fn transpile(source: &str) -> Result<Compiled, Vec<Diagnostic>> {
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

    // TypeScript's CommonJS forms. TypeScript drops them without a word in an
    // ES module (`import fs = require('fs')` left `fs` undefined); Oxc turns
    // them into `require` and `module.exports`, which the engine does not
    // have. Refused where they are written.
    errors.extend(Refused::in_source(&program).located(source));

    let semantic = SemanticBuilder::new()
        .with_check_syntax_error(true)
        // The enum transform reads the members' values from here: without
        // them, `Quick = Fast`, a string, gets a reverse mapping TypeScript
        // does not give it (oxc#21667). Only a debug assertion says so, and
        // the PoC was built in release.
        .with_enum_eval(true)
        .with_excess_capacity(2.0)
        .build(&program);
    errors.extend(errors_of(source, &semantic.diagnostics));
    if !errors.is_empty() {
        return Err(sorted(errors));
    }
    let scoping = semantic.semantic.into_scoping();

    // `esnext`, so nothing is lowered: QuickJS-ng 0.16 runs ES2023 natively,
    // `using` included. For ES2020, what the window asked TypeScript for, Oxc
    // lowers class fields and `#private` members with helpers the engine
    // cannot import (§3.2).
    let mut transform = TransformOptions::from_target("esnext").expect("a target Oxc knows");
    // TypeScript's `useDefineForClassFields: false`, its default for the
    // ES2020 target the window used: a field is assigned in the constructor,
    // after the parameter properties, and one without an initialiser is
    // dropped. Without this, `level = this.base * 2` next to
    // `constructor(readonly base: number)` is NaN with Oxc and 100 with
    // TypeScript (§3.1).
    transform.assumptions.set_public_class_fields = true;
    transform.typescript.remove_class_fields_without_initializer = true;
    // Nothing needs a helper at `esnext`. Should a transform ever want one,
    // External names it on a global, which says what is missing when the
    // module loads; the default imports a package the engine cannot resolve.
    transform.helper_loader.mode = HelperLoaderMode::External;
    let transformed = Transformer::new(&allocator, Path::new(FILE), &transform)
        .build_with_scoping(scoping, &mut program);
    errors.extend(errors_of(source, &transformed.diagnostics));
    // What Oxc leaves untransformed and QuickJS-ng cannot parse: decorators
    // and `accessor`, which TypeScript lowered (§3.3). Their spans are still
    // the source's.
    errors.extend(Refused::in_output(&program).located(source));
    if !errors.is_empty() {
        return Err(sorted(errors));
    }

    let codegen = Codegen::new()
        .with_options(CodegenOptions {
            source_map_path: Some(Path::new(FILE).to_path_buf()),
            ..CodegenOptions::default()
        })
        .build(&program);
    Ok(Compiled {
        js: codegen.code,
        source_map: codegen
            .map
            .map(|map| map.to_json_string())
            .unwrap_or_default(),
    })
}

/// Constructs found by walking a program, each with the reason it is refused.
#[derive(Default)]
struct Refused {
    /// Walking the transformed program rather than the parsed one.
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
                    message: message.to_owned(),
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

/// Oxc's errors, each at its primary label, where its own rendering
/// underlines it. Without one, at the label furthest in the file: a
/// redeclaration labels the first declaration, then the second, and the
/// second is the one to change.
pub(crate) fn errors_of(source: &str, diagnostics: &[OxcDiagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| {
            let offset = d
                .labels
                .iter()
                .find(|l| l.primary())
                .or_else(|| d.labels.iter().max_by_key(|l| l.offset()))
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
