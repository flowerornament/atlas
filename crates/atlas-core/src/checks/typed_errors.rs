//! Written String-error boundaries, using the retained Rust AST and test facts.
//! This fixed spelling rule does not resolve aliases, expand macros or prove HIR types.
use crate::{
    analysis::Analysis,
    population::{FileSelection, TestSelection},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use syn::{spanned::Spanned, visit::Visit};

/// One package's source reading, independent of project authority and policy verdicts.
#[derive(Default, Serialize)]
pub struct Reading {
    pub sources: BTreeMap<String, String>,
    /// Distinct production source lines containing a written String-error type.
    pub boundaries: BTreeMap<String, BTreeSet<usize>>,
    pub unavailable: Vec<Unavailable>,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Unavailable {
    EmptyPopulation,
    Syntax { file: String, reason: String },
    ExcludedSource { path: String, reason: String },
}
/// Read the native typed-error population, package by package. Unknown packages
/// are invalid selection; missing required source or syntax is unavailable data.
pub fn read(
    analysis: &Analysis<'_>,
    packages: &[String],
) -> crate::Result<BTreeMap<String, Reading>> {
    let snapshot = analysis.snapshot();
    let metadata = snapshot.metadata();
    let mut reports = BTreeMap::new();
    if packages.is_empty() {
        return Ok(reports);
    }
    let members: BTreeMap<_, _> = metadata
        .workspace_packages()
        .into_iter()
        .map(|package| (package.name.as_str(), package))
        .collect();
    let mut observations: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    for file in snapshot
        .files(FileSelection::SourceTree)
        .filter(|file| !file.test_only)
    {
        observations
            .entry(file.crate_name.as_str())
            .or_default()
            .push(file);
    }
    for name in packages {
        if reports.contains_key(name) {
            return Err(format!("duplicate typed-error package {name}").into());
        }
        let package = members
            .get(name.as_str())
            .ok_or_else(|| format!("typed-error package {name} is not a workspace member"))?;
        let src = package
            .manifest_path
            .as_std_path()
            .parent()
            .ok_or("manifest lacks parent")?
            .join("src");
        let src_rel = src
            .strip_prefix(metadata.workspace_root.as_std_path())?
            .to_string_lossy()
            .replace('\\', "/");
        let mut report = Reading::default();
        for file in observations.get(name.as_str()).into_iter().flatten() {
            report
                .sources
                .insert(file.path.clone(), file.content_fingerprint.clone());
            let source = snapshot
                .source_facts(&file.path)
                .ok_or("source population lacks captured facts")?;
            let ast = match analysis.syntax(&file.path) {
                Ok(ast) => ast,
                Err(error) => {
                    report.unavailable.push(Unavailable::Syntax {
                        file: file.path.clone(),
                        reason: error.to_string(),
                    });
                    continue;
                }
            };
            let mut visitor = Boundaries {
                source,
                lines: BTreeSet::new(),
            };
            visitor.visit_file(ast);
            if !visitor.lines.is_empty() {
                report.boundaries.insert(file.path.clone(), visitor.lines);
            }
        }
        let source_prefix = format!("{src_rel}/");
        for diagnostic in &snapshot.graph().diagnostics {
            if diagnostic
                .package
                .as_ref()
                .is_none_or(|package| package == name)
                && diagnostic.file.starts_with(&source_prefix)
                && !report.sources.contains_key(&diagnostic.file)
                && snapshot.source_facts(&diagnostic.file).is_none()
            {
                report.unavailable.push(Unavailable::ExcludedSource {
                    path: diagnostic.file.clone(),
                    reason: diagnostic.message.clone(),
                });
            }
        }
        if report.sources.is_empty() {
            report.unavailable.push(Unavailable::EmptyPopulation);
        }
        reports.insert(name.clone(), report);
    }
    Ok(reports)
}
struct Boundaries<'a> {
    source: &'a crate::source::Source,
    lines: BTreeSet<usize>,
}
impl<'ast> Visit<'ast> for Boundaries<'_> {
    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        let line = ty.span().start().line;
        if TestSelection::Production.admits_line(self.source, line) && string_error(ty) {
            self.lines.insert(line);
        }
        syn::visit::visit_type_path(self, ty);
    }
}
fn string_error(ty: &syn::TypePath) -> bool {
    if ty.qself.is_some() {
        return false;
    }
    let path = &ty.path;
    let Some(last) = path.segments.last() else {
        return false;
    };
    if last.ident != "Result" {
        return false;
    }
    let constructor = spelling(path, "Result", &["std", "core"], "result");
    if !constructor
        || path
            .segments
            .iter()
            .take(path.segments.len() - 1)
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        return false;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return false;
    };
    if arguments.args.len() != 2 {
        return false;
    }
    let Some(syn::GenericArgument::Type(syn::Type::Path(error))) = arguments.args.last() else {
        return false;
    };
    if error.qself.is_some()
        || error
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        return false;
    }
    spelling(&error.path, "String", &["std", "alloc"], "string")
}
fn spelling(path: &syn::Path, name: &str, scopes: &[&str], middle: &str) -> bool {
    let mut segments = path.segments.iter();
    match (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) {
        (Some(one), None, None, None) => path.leading_colon.is_none() && one.ident == name,
        (Some(scope), Some(module), Some(last), None) => {
            scopes.iter().any(|expected| scope.ident == *expected)
                && module.ident == middle
                && last.ident == name
        }
        _ => false,
    }
}
