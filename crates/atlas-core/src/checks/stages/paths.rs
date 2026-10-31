//! Read captured use statements and textual paths in their module namespace.
//! This native path grammar is distinct from qualified call resolution.
use super::{FileMap, ItemKind, RootTable, Unjudged};
use crate::scan::{ImportBinding, use_import};
use std::collections::{BTreeMap, BTreeSet};
/// Which namespaces a path's last segment names: a `use` binding imports
/// every one, an inline path names one, and a braced `self`
/// (`use crate::parse::{self};`) names only the module.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PathForm {
    Use,
    Inline,
    Module,
}

/// One logical line of a production file: a whole `use` statement at its
/// first line, or a code line outside every `use`.
pub(super) enum SourceLine<'a> {
    Use {
        line: usize,
        top_level: bool,
        bindings: &'a [ImportBinding],
    },
    UnreadableUse {
        line: usize,
        text: &'a str,
    },
    Code {
        line: usize,
        code: &'a str,
    },
}

/// A file's production lines, `use` statements joined across lines.
pub(super) fn source_lines(file: &FileMap) -> Vec<SourceLine<'_>> {
    logical_lines(file, file.production_code_lines())
}
pub(super) fn logical_lines<'a>(
    file: &'a FileMap,
    code_lines: impl Iterator<Item = (usize, &'a str)>,
) -> Vec<SourceLine<'a>> {
    let mut lines = Vec::new();
    let mut covered_until = 0;
    for (line, code) in code_lines {
        if line <= covered_until {
            continue;
        }
        if let Some(import) = file.source.import_at(line) {
            covered_until = import.end_line;
            lines.push(match &import.bindings {
                Ok(bindings) => SourceLine::Use {
                    line,
                    top_level: import.top_level,
                    bindings,
                },
                Err(_) => SourceLine::UnreadableUse {
                    line,
                    text: &import.statement,
                },
            });
        } else if use_import(code).is_some() {
            lines.push(SourceLine::UnreadableUse { line, text: code });
        } else {
            lines.push(SourceLine::Code { line, code });
        }
    }
    lines
}

/// A file's test lines, `use` statements joined across lines: the whole of a
/// test file, its test regions and its items gated to test builds, each line
/// keyed by the module it sits in (`module` plus its inline `mod` chain).
pub(super) fn test_source_lines<'a>(
    file: &'a FileMap,
    module: &[&str],
) -> BTreeMap<Vec<String>, Vec<SourceLine<'a>>> {
    let blocks: Vec<&crate::scan::Item> = file
        .source
        .outline()
        .iter()
        .filter(|item| {
            item.kind == ItemKind::Mod && !item.declaration_block.trim_end().ends_with(';')
        })
        .collect();
    let mut by_module: BTreeMap<Vec<String>, Vec<(usize, &str)>> = BTreeMap::new();
    for (index, code) in file.source.projection().code_lines().enumerate() {
        let line = index + 1;
        if !file.test_file
            && crate::population::TestSelection::Production.admits_line(&file.source, line)
        {
            continue;
        }
        let mut path: Vec<String> = module.iter().map(|segment| (*segment).to_owned()).collect();
        path.extend(
            blocks
                .iter()
                .filter(|item| item.line < line && item.span.contains(&line))
                .map(|item| item.name.clone()),
        );
        by_module.entry(path).or_default().push((line, code));
    }
    by_module
        .into_iter()
        .map(|(path, lines)| (path, logical_lines(file, lines.into_iter())))
        .collect()
}

/// The leading `crate`/`super`/`self` (or root alias) segments of `path`
/// walked from inside `module`: the module reached, and the remaining
/// segments. `None` for a path rooted elsewhere (another crate, or a uniform
/// path through a child module).
pub(super) fn walk<'a>(
    module: &[&'a str],
    path: &'a str,
    aliases: &BTreeSet<String>,
) -> Option<(Vec<&'a str>, Vec<&'a str>)> {
    let mut segments = path.split("::").map(str::trim);
    let mut base = module.to_vec();
    match segments.next()? {
        "crate" => base.clear(),
        "super" => {
            base.pop();
        }
        "self" => {}
        alias if aliases.contains(alias) => base.clear(),
        _ => return None,
    }
    let mut rest: Vec<&str> = segments.collect();
    while let Some(&segment) = rest.first() {
        match segment {
            "super" => {
                base.pop();
            }
            "self" => {}
            _ => break,
        }
        rest.remove(0);
    }
    Some((base, rest))
}

/// The crate paths a crate-relative path names from inside `module`
/// (`["load", "plan"]`), each from its top-level module: empty for paths
/// rooted elsewhere or naming only lib.rs's own items. A path that reaches
/// the crate root is read through `root`.
pub(super) fn read_path(
    module: &[&str],
    path: &str,
    form: PathForm,
    root: &RootTable,
    aliases: &BTreeSet<String>,
) -> std::result::Result<Vec<Vec<String>>, Unjudged> {
    let Some((base, rest)) = walk(module, path, aliases) else {
        return Ok(Vec::new());
    };
    if base.is_empty() {
        root.read(&rest, form, 0)
    } else {
        Ok(vec![
            base.iter().chain(&rest).map(|s| (*s).to_owned()).collect(),
        ])
    }
}

/// The local names one file binds to the crate root (`use crate as c;`,
/// `use super::super as root;`), with the crate's `extern crate self` names.
pub(super) fn root_aliases(
    module: &[&str],
    lines: &[SourceLine<'_>],
    root: &RootTable,
) -> BTreeSet<String> {
    let mut aliases = root.self_names.clone();
    for line in lines {
        let SourceLine::Use { bindings, .. } = line else {
            continue;
        };
        for binding in bindings.iter().filter(|binding| binding.local_name != "*") {
            if let Some((base, rest)) = walk(module, &binding.source_path, &root.self_names)
                && base.is_empty()
                && root.names_root(&rest, 0)
            {
                aliases.insert(binding.local_name.clone());
            }
        }
    }
    aliases
}

/// Every identifier path on one code line (`engine::Engine`, `Engine`).
pub(super) fn identifier_paths(line: &str) -> Vec<&str> {
    line.split(|ch: char| !(ch == '_' || ch == ':' || ch.is_ascii_alphanumeric()))
        .map(|token| token.trim_matches(':'))
        .filter(|token| token.starts_with(|ch: char| ch == '_' || ch.is_ascii_alphabetic()))
        .collect()
}

/// Inline paths on one code line rooted at `crate::`, `super::`, `self::`
/// or one of `aliases`.
pub(super) fn inline_paths<'a>(line: &'a str, aliases: &BTreeSet<String>) -> Vec<&'a str> {
    let roots = ["crate", "super", "self"]
        .into_iter()
        .map(str::to_owned)
        .chain(aliases.iter().cloned())
        .map(|root| format!("{root}::"));
    let mut paths = Vec::new();
    for root in roots {
        for (start, _) in line.match_indices(&root) {
            let bounded = line[..start]
                .chars()
                .next_back()
                .is_none_or(|ch| !(ch == '_' || ch == ':' || ch.is_ascii_alphanumeric()));
            if !bounded {
                continue;
            }
            let end = line[start..]
                .find(|ch: char| !(ch == '_' || ch == ':' || ch.is_ascii_alphanumeric()))
                .map_or(line.len(), |end| start + end);
            paths.push(line[start..end].trim_end_matches(':'));
        }
    }
    paths
}

/// Every crate-internal path of one production file as `(line, path, form)`:
/// each `use` binding at its statement's first line, then inline paths.
pub(super) fn crate_paths(
    lines: &[SourceLine<'_>],
    aliases: &BTreeSet<String>,
) -> Vec<(usize, String, PathForm)> {
    let mut paths = Vec::new();
    for source_line in lines {
        match source_line {
            SourceLine::UnreadableUse { .. } => {}
            SourceLine::Use { line, bindings, .. } => {
                paths.extend(bindings.iter().map(|binding| {
                    let form = if binding.module_only {
                        PathForm::Module
                    } else {
                        PathForm::Use
                    };
                    (*line, binding.source_path.clone(), form)
                }));
            }
            SourceLine::Code { line, code } => {
                paths.extend(
                    inline_paths(code, aliases)
                        .into_iter()
                        .map(|path| (*line, path.to_owned(), PathForm::Inline)),
                );
            }
        }
    }
    paths
}
