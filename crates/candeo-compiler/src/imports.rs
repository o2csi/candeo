//! What the engine would refuse when it links the module, found before it
//! runs, and located (`docs/design/headless-transpile-poc.md` §3.6).
//!
//! The engine resolves one module, [`API_MODULE`]. Without this check an
//! import of anything else, or of a name the API does not export, fails when
//! the module loads, with no position: `Error resolving module 'lodash' from
//! 'effect'`.

use oxc::allocator::Allocator;
use oxc::parser::Parser;
use oxc::span::SourceType;
use oxc::syntax::module_record::{
    ExportExportName, ExportImportName, ImportImportName, ModuleRecord,
};

use crate::position::{line_column, sorted};
use crate::{Diagnostic, API_MODULE};

/// The names a module exports, read from its syntax.
pub(crate) fn exports_of(js: &str) -> Vec<String> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, js, SourceType::mjs()).parse();
    let mut names: Vec<String> = parsed
        .module_record
        .local_export_entries
        .iter()
        .filter_map(|e| match &e.export_name {
            ExportExportName::Name(n) => Some(n.name.to_string()),
            ExportExportName::Default(_) => Some("default".into()),
            ExportExportName::Null => None,
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// An import of anything but [`API_MODULE`], a name it does not export, a
/// default import, a type re-exported as a value, a dynamic `import()`.
///
/// Read from the **JavaScript**, since only what survives type stripping is
/// imported at run time (`import { Rgb }` used as a type alone is gone), and
/// located in the **source**, where the author reads it. Parses the source
/// again: called on [`crate::compile`]'s thread only.
pub(crate) fn check(source: &str, js: &str, api_exports: &[String]) -> Vec<Diagnostic> {
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
