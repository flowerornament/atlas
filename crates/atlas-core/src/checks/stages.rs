//! Native textual stage reading over the shared snapshot population.
//! These observations rank declared source paths; they do not resolve execution.
use super::{CrateMap, FileMap, ItemKind, policy};
use crate::population::TestSelection;
use crate::{
    discovery::module_path_of,
    scan::{ImportBinding, use_import},
};
use policy::StageOrder;
use serde::Serialize;
pub mod seams;

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::{self, Write as _},
};

/// The longest chain of root aliases the reading follows before it reports
/// the path unjudged. Rust itself rejects a cyclic chain.
const ALIAS_DEPTH: usize = 16;

/// One crate-internal path from one module into another, at its site.
#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub file: String,
    pub line: usize,
    pub from: String,
    pub to: String,
    pub path: String,
    /// The path resolved from the crate root, `to` first
    /// (`["cover", "timing", "RegionWork"]`): what the item-level seam reads.
    pub target: Vec<String>,
}

/// Which namespaces a path's last segment names: a `use` binding imports
/// every one, an inline path names one, and a braced `self`
/// (`use crate::parse::{self};`) names only the module.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PathForm {
    Use,
    Inline,
    Module,
}

/// Why the textual reading cannot rank a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unjudged {
    /// A glob over the crate root (`use crate::*`).
    RootGlob,
    /// A root name lib.rs neither defines nor binds by name, while a lib.rs
    /// glob re-exports a crate module that may supply it.
    GlobbedRootName,
    /// A root name lib.rs neither defines nor binds, and no glob supplies.
    UnknownRootName,
    /// A chain of root aliases beyond the reader's fixed depth limit.
    AliasChain,
    /// Competing conditional root bindings cannot select one arbitrary target.
    AmbiguousRootName,
    /// The shared source grammar could not read this use statement.
    UnreadableImport,
}

impl Unjudged {
    /// The fix a refusal names.
    fn remedy(self) -> &'static str {
        match self {
            Self::AmbiguousRootName => {
                "name the defining stage directly; competing root bindings are unresolved"
            }
            Self::UnreadableImport => {
                "write a complete use statement in the supported Rust source form"
            }
            Self::RootGlob => "import each name by its true path, `crate::<stage>::…`",
            Self::GlobbedRootName => {
                "name the item by its true path, `crate::<stage>::…`, or replace the lib.rs glob with named re-exports"
            }
            Self::UnknownRootName | Self::AliasChain => {
                "name the item by its true path, `crate::<stage>::…`"
            }
        }
    }
}

impl fmt::Display for Unjudged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AmbiguousRootName => "competing crate-root bindings with the same name",
            Self::UnreadableImport => "a use statement whose syntax could not be read",
            Self::RootGlob => "a glob over the crate root names nothing the reading can rank",
            Self::GlobbedRootName => "a crate-root name a lib.rs glob re-export may supply",
            Self::UnknownRootName => "a crate-root name lib.rs neither defines nor re-exports",
            Self::AliasChain => "a chain of crate-root aliases too long to follow",
        })
    }
}

/// One path the reading could not rank, at its site.
#[derive(Debug, Serialize)]
pub struct UnjudgedPath {
    pub file: String,
    pub line: usize,
    pub path: String,
    pub why: Unjudged,
}

/// One logical line of a production file: a whole `use` statement at its
/// first line, or a code line outside every `use`.
enum SourceLine<'a> {
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
fn source_lines(file: &FileMap) -> Vec<SourceLine<'_>> {
    logical_lines(file, file.production_code_lines())
}
fn logical_lines<'a>(
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
fn test_source_lines<'a>(
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

/// The crate root as lib.rs presents it: the top-level modules, the names
/// lib.rs binds by `use` (local name to source path), the crate modules it
/// glob re-exports, the names it defines itself, the paths its fn, const,
/// static and type-alias items name, the `#[macro_export]` macros with their
/// defining modules, and its production inline modules, whose code the
/// reading does not see.
pub(in crate::checks) struct RootTable {
    modules: BTreeSet<String>,
    bindings: BTreeMap<String, BTreeSet<String>>,
    unreadable_imports: Vec<(usize, String)>,
    globs: Vec<String>,
    items: BTreeSet<String>,
    item_paths: BTreeMap<String, Vec<String>>,
    /// The lib.rs items being read, so mutually naming items end the walk
    /// instead of exhausting [`ALIAS_DEPTH`].
    reading: RefCell<BTreeSet<String>>,
    exported_macros: BTreeMap<String, String>,
    inline_modules: BTreeSet<String>,
    /// `extern crate self as NAME;`: NAME is the crate root in every file.
    self_names: BTreeSet<String>,
    /// Every file module's path from the root (`["cover", "timing"]`).
    module_paths: BTreeSet<Vec<String>>,
    /// Directly declared value-only names, not inferred import or type kinds.
    value_paths: BTreeSet<Vec<String>>,
}

impl RootTable {
    pub(in crate::checks) fn of(crate_map: &CrateMap, selection: TestSelection) -> Self {
        let mut table = Self {
            modules: BTreeSet::new(),
            bindings: BTreeMap::new(),
            unreadable_imports: Vec::new(),
            globs: Vec::new(),
            items: BTreeSet::new(),
            item_paths: BTreeMap::new(),
            reading: RefCell::new(BTreeSet::new()),
            exported_macros: BTreeMap::new(),
            inline_modules: BTreeSet::new(),
            self_names: BTreeSet::new(),
            module_paths: BTreeSet::new(),
            value_paths: BTreeSet::new(),
        };
        for file in &crate_map.files {
            if !selection.admits_file(file.test_file) {
                continue;
            }
            let Some(module) = module_path_of(&crate_map.src_rel, &file.rel) else {
                continue;
            };
            table
                .module_paths
                .insert(module.split("::").map(str::to_owned).collect());
            let head = module.split("::").next().unwrap_or_default().to_owned();
            table.modules.insert(head.clone());
            table.index_values(
                &module.split("::").map(str::to_owned).collect::<Vec<_>>(),
                file,
                selection,
            );
            let text: Vec<&str> = file.source.projection().text().lines().collect();
            for item in file.source.outline() {
                let exported = (*item.span.start()..item.line).any(|line| {
                    text.get(line - 1)
                        .is_some_and(|code| code.trim().starts_with("#[macro_export"))
                });
                if item.kind == ItemKind::MacroRules
                    && exported
                    && selection.admits_line(&file.source, item.line)
                {
                    table
                        .exported_macros
                        .insert(item.name.clone(), head.clone());
                }
            }
        }
        let lib_rel = format!("{}/lib.rs", crate_map.src_rel);
        if matches!(selection, TestSelection::All) {
            for mount in &crate_map.mounted {
                let path: Vec<String> = mount.module.split("::").map(str::to_owned).collect();
                if let Some(head) = path.first() {
                    table.modules.insert(head.clone());
                }
                table.module_paths.insert(path);
                table.index_values(
                    &mount
                        .module
                        .split("::")
                        .map(str::to_owned)
                        .collect::<Vec<_>>(),
                    &mount.file,
                    selection,
                );
            }
        }
        let Some(lib) = crate_map.files.iter().find(|file| file.rel == lib_rel) else {
            return table;
        };
        table.index_values(&[], lib, selection);
        let mut glob_sources = Vec::new();
        for source_line in logical_lines(lib, lib.code_lines(selection)) {
            match source_line {
                SourceLine::Use {
                    top_level: true,
                    bindings,
                    ..
                } => {
                    for binding in bindings {
                        if binding.local_name == "*" {
                            glob_sources.push(binding.source_path.clone());
                        } else {
                            table
                                .bindings
                                .entry(binding.local_name.clone())
                                .or_default()
                                .insert(binding.source_path.clone());
                        }
                    }
                }
                SourceLine::Code { code, .. } => {
                    if let Some(name) = code
                        .trim()
                        .strip_prefix("extern crate self as ")
                        .and_then(|rest| rest.strip_suffix(';'))
                    {
                        table.self_names.insert(name.trim().to_owned());
                    }
                }
                SourceLine::UnreadableUse { line, text } => {
                    table.unreadable_imports.push((line, text.to_owned()));
                }
                SourceLine::Use { .. } => {}
            }
        }
        for name in &table.self_names {
            table
                .bindings
                .entry(name.clone())
                .or_default()
                .insert("crate".to_owned());
        }
        for item in lib.items(selection).filter(|item| item.top_level) {
            match item.kind {
                ItemKind::Mod if !table.modules.contains(&item.name) => {
                    if !item.declaration_block.trim_end().ends_with(';') {
                        table.inline_modules.insert(item.name.clone());
                    }
                    table.items.insert(item.name.clone());
                }
                ItemKind::Struct
                | ItemKind::Enum
                | ItemKind::Union
                | ItemKind::Trait
                | ItemKind::TypeAlias
                | ItemKind::Fn
                | ItemKind::MacroRules
                | ItemKind::Const
                | ItemKind::Static => {
                    table.items.insert(item.name.clone());
                }
                _ => {}
            }
        }
        // Every lib.rs item's text, impl blocks under their type's name: what
        // it names is what a reader of the root name reaches.
        let code: BTreeMap<usize, &str> = lib.code_lines(selection).collect();
        for item in lib.items(selection).filter(|item| item.top_level) {
            if !matches!(
                item.kind,
                ItemKind::Fn
                    | ItemKind::Const
                    | ItemKind::Static
                    | ItemKind::TypeAlias
                    | ItemKind::Struct
                    | ItemKind::Enum
                    | ItemKind::Union
                    | ItemKind::Trait
                    | ItemKind::Impl
            ) {
                continue;
            }
            let paths: BTreeSet<String> = item
                .span
                .clone()
                .filter_map(|line| code.get(&line))
                .flat_map(|line| identifier_paths(line))
                .filter(|path| {
                    let head = path.split("::").next().unwrap_or_default();
                    // A bare name counts only when lib.rs binds or defines it.
                    let named = if path.contains("::") {
                        matches!(head, "crate" | "self") || table.is_internal(head)
                    } else {
                        table.bindings.contains_key(head) || table.items.contains(head)
                    };
                    head != item.name && named
                })
                .map(str::to_owned)
                .collect();
            table
                .item_paths
                .entry(item.name.clone())
                .or_default()
                .extend(paths);
        }
        // A glob over another crate supplies no crate-internal name.
        table.globs = glob_sources
            .into_iter()
            .filter(|source| {
                table
                    .rooted(source.trim_end_matches("::*"))
                    .is_some_and(|segments| !segments.is_empty())
            })
            .collect();
        table
    }

    fn is_internal(&self, name: &str) -> bool {
        self.modules.contains(name) || self.bindings.contains_key(name) || self.items.contains(name)
    }

    fn index_values(&mut self, module: &[String], file: &FileMap, selection: TestSelection) {
        self.value_paths.extend(
            file.items(selection)
                .filter(|item| {
                    item.top_level
                        && matches!(item.kind, ItemKind::Fn | ItemKind::Const | ItemKind::Static)
                })
                .map(|item| {
                    let mut path = module.to_vec();
                    path.push(item.name.clone());
                    path
                }),
        );
    }

    /// Only declarations prove the two namespaces. Unknown aliases cannot.
    fn namespace_pair<'a>(&self, sources: &'a BTreeSet<String>) -> Option<(&'a str, &'a str)> {
        if sources.len() != 2 {
            return None;
        }
        let (mut module, mut value) = (None, None);
        for source in sources {
            let path: Vec<_> = self
                .rooted(source)?
                .into_iter()
                .map(str::to_owned)
                .collect();
            match (
                self.module_paths.contains(&path),
                self.value_paths.contains(&path),
            ) {
                (true, false) if module.is_none() => module = Some(source.as_str()),
                (false, true) if value.is_none() => value = Some(source.as_str()),
                _ => return None,
            }
        }
        Some((module?, value?))
    }

    /// A lib.rs `use` source as segments from the crate root, or `None` when
    /// it names another crate.
    fn rooted<'a>(&self, source: &'a str) -> Option<Vec<&'a str>> {
        let mut segments: Vec<&str> = source.split("::").map(str::trim).collect();
        match segments.first().copied() {
            Some("crate" | "self") => {
                segments.remove(0);
                Some(segments)
            }
            Some(head) if self.is_internal(head) => Some(segments),
            _ => None,
        }
    }

    /// The crate paths a path from the crate root resolves to, each starting
    /// at its top-level module: none for the root itself, lib.rs's own items
    /// and other crates.
    fn read(
        &self,
        segments: &[&str],
        form: PathForm,
        depth: usize,
    ) -> std::result::Result<Vec<Vec<String>>, Unjudged> {
        let Some((&name, rest)) = segments.split_first() else {
            return Ok(Vec::new());
        };
        if name == "*" {
            return Err(Unjudged::RootGlob);
        }
        if depth > ALIAS_DEPTH {
            return Err(Unjudged::AliasChain);
        }
        let owned = || {
            segments
                .iter()
                .map(|segment| (*segment).to_owned())
                .collect()
        };
        let module = self.modules.contains(name);
        let candidates = self.bindings.get(name);
        if !module
            && let Some((module_source, value_source)) =
                candidates.and_then(|sources| self.namespace_pair(sources))
        {
            if !rest.is_empty() || form == PathForm::Module {
                return self.follow(module_source, rest, form, depth);
            }
            let mut paths = self.follow(value_source, rest, form, depth)?;
            if form == PathForm::Use {
                paths.extend(self.follow(module_source, rest, form, depth)?);
            }
            return Ok(paths);
        }
        if candidates.is_some_and(|candidates| candidates.len() > 1)
            && !(module && (!rest.is_empty() || form == PathForm::Module))
        {
            return Err(Unjudged::AmbiguousRootName);
        }
        match (module, candidates.and_then(|candidates| candidates.first())) {
            // `crate::parse::X` continues in the type namespace: the module.
            (true, Some(_)) if !rest.is_empty() || form == PathForm::Module => Ok(vec![owned()]),
            // A terminal name that is both: a call reads the re-export, and a
            // `use` imports both namespaces.
            (true, Some(source)) => {
                let mut paths = self.follow(source, rest, form, depth)?;
                if form == PathForm::Use {
                    paths.push(owned());
                }
                Ok(paths)
            }
            (true, None) => Ok(vec![owned()]),
            (false, Some(source)) => self.follow(source, rest, form, depth),
            (false, None) => self.read_unbound(name, depth),
        }
    }

    /// A root name lib.rs binds by no `use`: one of its own items (read
    /// through the paths its text names), an exported macro (its defining
    /// module), or a name the reading cannot place.
    fn read_unbound(
        &self,
        name: &str,
        depth: usize,
    ) -> std::result::Result<Vec<Vec<String>>, Unjudged> {
        if let Some(paths) = self.item_paths.get(name) {
            if !self.reading.borrow_mut().insert(name.to_owned()) {
                return Ok(Vec::new());
            }
            let mut heads = BTreeSet::new();
            let mut read = Ok(());
            for segments in paths.iter().filter_map(|path| self.rooted(path)) {
                match self.read(&segments, PathForm::Inline, depth + 1) {
                    Ok(found) => heads.extend(found),
                    Err(why) => read = Err(why),
                }
            }
            self.reading.borrow_mut().remove(name);
            read?;
            return Ok(heads.into_iter().collect());
        }
        if let Some(module) = self.exported_macros.get(name) {
            return Ok(vec![vec![module.clone(), name.to_owned()]]);
        }
        if self.items.contains(name) {
            Ok(Vec::new())
        } else if self.globs.is_empty() {
            Err(Unjudged::UnknownRootName)
        } else {
            Err(Unjudged::GlobbedRootName)
        }
    }

    /// Continue a root path through one lib.rs binding.
    fn follow(
        &self,
        source: &str,
        rest: &[&str],
        form: PathForm,
        depth: usize,
    ) -> std::result::Result<Vec<Vec<String>>, Unjudged> {
        let Some(mut segments) = self.rooted(source) else {
            return Ok(Vec::new());
        };
        segments.extend_from_slice(rest);
        self.read(&segments, form, depth + 1)
    }

    /// Whether a path from the crate root names the root itself, through
    /// any chain of lib.rs aliases (`pub(crate) use crate as c;`).
    fn names_root(&self, segments: &[&str], depth: usize) -> bool {
        let Some((&name, rest)) = segments.split_first() else {
            return true;
        };
        if depth > ALIAS_DEPTH || self.modules.contains(name) {
            return false;
        }
        self.bindings
            .get(name)
            .filter(|sources| sources.len() == 1)
            .and_then(|sources| sources.first())
            .and_then(|source| self.rooted(source))
            .is_some_and(|mut segments| {
                segments.extend_from_slice(rest);
                self.names_root(&segments, depth + 1)
            })
    }
}

impl RootTable {
    /// Every crate path one module's lines name, an edge per resolved
    /// target, and the paths the reading cannot rank. A local name bound to
    /// a crate module (`use crate::cover::timing;`) carries its inline paths
    /// (`timing::RegionWork`) with it, so a module import cannot hide what
    /// is read through it.
    fn edges(
        &self,
        file: &str,
        module: &[&str],
        lines: &[SourceLine<'_>],
    ) -> (Vec<Edge>, Vec<UnjudgedPath>) {
        let from = module[0].to_owned();
        let aliases = root_aliases(module, lines, self);
        let mut edges = Vec::new();
        let mut unjudged = lines
            .iter()
            .filter_map(|source| match source {
                SourceLine::UnreadableUse { line, text } => Some(UnjudgedPath {
                    file: file.to_owned(),
                    line: *line,
                    path: (*text).to_owned(),
                    why: Unjudged::UnreadableImport,
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut import_aliases = BTreeMap::new();
        for source in lines {
            if let SourceLine::Use { line, bindings, .. } = source {
                for binding in *bindings {
                    import_aliases
                        .entry((*line, binding.source_path.as_str()))
                        .or_insert(binding.local_name.as_str());
                }
            }
        }
        let mut module_aliases: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut push = |line: usize, path: &str, target: Vec<String>| {
            edges.push(Edge {
                file: file.to_owned(),
                line,
                from: from.clone(),
                to: target[0].clone(),
                path: path.to_owned(),
                target,
            });
        };
        for (line, path, form) in crate_paths(lines, &aliases) {
            match read_path(module, &path, form, self, &aliases) {
                Ok(targets) => {
                    if form != PathForm::Inline
                        && let [target] = targets.as_slice()
                        && self.module_paths.contains(target)
                        && let Some(local) = import_aliases.get(&(line, path.as_str()))
                        && *local != "*"
                    {
                        module_aliases.insert((*local).to_owned(), target.clone());
                    }
                    for target in targets {
                        push(line, &path, target);
                    }
                }
                Err(why) => unjudged.push(UnjudgedPath {
                    file: file.to_owned(),
                    line,
                    path,
                    why,
                }),
            }
        }
        let names: BTreeSet<String> = module_aliases.keys().cloned().collect();
        for source in lines {
            let SourceLine::Code { line, code } = source else {
                continue;
            };
            for path in inline_paths(code, &names) {
                let mut segments = path.split("::");
                let Some(target) = segments.next().and_then(|head| module_aliases.get(head)) else {
                    continue;
                };
                let mut target = target.clone();
                target.extend(segments.map(str::to_owned));
                push(*line, path, target);
            }
        }
        (edges, unjudged)
    }
}

/// The leading `crate`/`super`/`self` (or root alias) segments of `path`
/// walked from inside `module`: the module reached, and the remaining
/// segments. `None` for a path rooted elsewhere (another crate, or a uniform
/// path through a child module).
fn walk<'a>(
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
fn read_path(
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
fn root_aliases(module: &[&str], lines: &[SourceLine<'_>], root: &RootTable) -> BTreeSet<String> {
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
fn identifier_paths(line: &str) -> Vec<&str> {
    line.split(|ch: char| !(ch == '_' || ch == ':' || ch.is_ascii_alphanumeric()))
        .map(|token| token.trim_matches(':'))
        .filter(|token| token.starts_with(|ch: char| ch == '_' || ch.is_ascii_alphabetic()))
        .collect()
}

/// Inline paths on one code line rooted at `crate::`, `super::`, `self::`
/// or one of `aliases`.
fn inline_paths<'a>(line: &'a str, aliases: &BTreeSet<String>) -> Vec<&'a str> {
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
fn crate_paths(
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

/// A forward edge through unplaced modules: a placed stage reads an
/// unplaced module that, directly or through further unplaced modules,
/// reads a later stage.
#[derive(Debug, Serialize)]
pub struct TransitiveEdge {
    pub reader: Edge,
    pub via: Vec<String>,
    pub onward: Edge,
}

/// One crate's stage reading over its production files: forward edges, the
/// judged edge count, the paths and modules it could not judge, and the
/// modules and stages the order and the tree do not share.
#[derive(Debug, Serialize)]
pub struct StageReport {
    /// The declared stage interfaces, when the crate declares them.
    pub interfaces: Option<BTreeMap<String, policy::StageReads>>,
    /// The stages every stage reads whole, when the crate declares seams.
    pub floor: Vec<String>,
    pub forward: Vec<Edge>,
    /// Each path from a stage into an earlier one: what the stage reads.
    pub backward: Vec<Edge>,
    /// Every production path from one top-level module into another, each
    /// resolved target its own edge: the item-level seam reads these.
    pub crossing: Vec<Edge>,
    pub through_unplaced: Vec<TransitiveEdge>,
    pub unjudged: Vec<UnjudgedPath>,
    pub judged: usize,
    pub unplaced: BTreeSet<String>,
    pub inline_modules: BTreeSet<String>,
    pub absent: Vec<String>,
    /// Modules that share their name with a lib.rs re-export.
    pub shared_names: BTreeSet<String>,
}

impl StageReport {
    #[cfg(test)]
    pub(super) fn of(crate_map: &CrateMap, stages: &[String]) -> Self {
        let root = RootTable::of(crate_map, TestSelection::Production);
        Self::from_root(crate_map, stages, &root)
    }
    pub(in crate::checks) fn from_root(
        crate_map: &CrateMap,
        stages: &[String],
        root: &RootTable,
    ) -> Self {
        let rank = |module: &str| stages.iter().position(|stage| stage == module);
        let mut report = Self {
            interfaces: None,
            floor: Vec::new(),
            forward: Vec::new(),
            backward: Vec::new(),
            crossing: Vec::new(),
            through_unplaced: Vec::new(),
            unjudged: root
                .unreadable_imports
                .iter()
                .map(|(line, text)| UnjudgedPath {
                    file: format!("{}/lib.rs", crate_map.src_rel),
                    line: *line,
                    path: text.clone(),
                    why: Unjudged::UnreadableImport,
                })
                .collect(),
            judged: 0,
            unplaced: BTreeSet::new(),
            inline_modules: root.inline_modules.clone(),
            absent: Vec::new(),
            shared_names: root
                .modules
                .iter()
                .filter(|module| root.bindings.contains_key(*module))
                .cloned()
                .collect(),
        };
        let mut present = BTreeSet::new();
        // Every cross-module read, placed or not, for the transitive judgment.
        let mut reads: BTreeMap<String, Vec<Edge>> = BTreeMap::new();
        for (module, file) in crate_map.production_modules() {
            let Some(module) = module else {
                continue;
            };
            let module: Vec<&str> = module.split("::").collect();
            let from = module[0].to_owned();
            present.insert(from.clone());
            let from_rank = rank(&from);
            if from_rank.is_none() {
                report.unplaced.insert(from.clone());
            }
            let lines = source_lines(file);
            let (edges, unjudged) = root.edges(&file.rel, &module, &lines);
            report.unjudged.extend(unjudged);
            let mut seen = BTreeSet::new();
            for edge in edges {
                if edge.to == from {
                    continue;
                }
                reads.entry(from.clone()).or_default().push(edge.clone());
                report.crossing.push(edge.clone());
                let (Some(from_rank), Some(to_rank)) = (from_rank, rank(&edge.to)) else {
                    continue;
                };
                if !seen.insert((edge.line, edge.to.clone())) {
                    continue;
                }
                report.judged += 1;
                if to_rank > from_rank {
                    report.forward.push(edge);
                } else {
                    report.backward.push(edge);
                }
            }
        }
        for via in &report.unplaced {
            let Some((chain, onward)) = latest_reach(via, &reads, &report.unplaced, &rank) else {
                continue;
            };
            let onward_rank = rank(&onward.to).expect("reach ends at a ranked stage");
            for edges in reads.values() {
                for reader in edges.iter().filter(|edge| &edge.to == via) {
                    if rank(&reader.from).is_some_and(|reader_rank| reader_rank < onward_rank) {
                        report.through_unplaced.push(TransitiveEdge {
                            reader: reader.clone(),
                            via: chain.clone(),
                            onward: onward.clone(),
                        });
                    }
                }
            }
        }
        report.absent = stages
            .iter()
            .filter(|stage| !present.contains(*stage))
            .cloned()
            .collect();
        report
    }
}

/// The latest stage an unplaced module reaches through unplaced modules
/// only: the chain of unplaced modules walked (starting at `start`) and the
/// read that reaches the stage.
fn latest_reach(
    start: &str,
    reads: &BTreeMap<String, Vec<Edge>>,
    unplaced: &BTreeSet<String>,
    rank: &impl Fn(&str) -> Option<usize>,
) -> Option<(Vec<String>, Edge)> {
    let mut best: Option<(usize, Vec<String>, Edge)> = None;
    let mut visited = BTreeSet::from([start.to_owned()]);
    let mut queue = VecDeque::from([vec![start.to_owned()]]);
    while let Some(chain) = queue.pop_front() {
        let module = chain.last().expect("a chain names its module");
        for edge in reads.get(module).into_iter().flatten() {
            if let Some(to_rank) = rank(&edge.to) {
                if best.as_ref().is_none_or(|(rank, _, _)| to_rank > *rank) {
                    best = Some((to_rank, chain.clone(), edge.clone()));
                }
            } else if unplaced.contains(&edge.to) && visited.insert(edge.to.clone()) {
                let mut next = chain.clone();
                next.push(edge.to.clone());
                queue.push_back(next);
            }
        }
    }
    best.map(|(_, chain, edge)| (chain, edge))
}

/// The selected stage orders with their reports, in policy order.
impl StageReport {
    /// Paths per (reader, read) stage pair, upstream reads only.
    pub fn reads(&self) -> BTreeMap<String, BTreeMap<String, usize>> {
        let mut reads: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for edge in &self.backward {
            *reads
                .entry(edge.from.clone())
                .or_default()
                .entry(edge.to.clone())
                .or_default() += 1;
        }
        reads
    }

    /// The first site of each read the declared interfaces do not list. A
    /// crate without declared interfaces lists nothing and is not judged.
    pub fn undeclared_reads(&self) -> Vec<&Edge> {
        let Some(interfaces) = &self.interfaces else {
            return Vec::new();
        };
        let mut seen = BTreeSet::new();
        self.backward
            .iter()
            .filter(|edge| {
                !interfaces
                    .get(&edge.from)
                    .is_some_and(|declared| declared.admits(&edge.to, &self.floor))
            })
            .filter(|edge| seen.insert((edge.from.clone(), edge.to.clone())))
            .collect()
    }

    /// Declared reads with no path left: each can leave its row.
    pub fn unused_reads(&self) -> Vec<(String, String)> {
        let Some(interfaces) = &self.interfaces else {
            return Vec::new();
        };
        let reads = self.reads();
        interfaces
            .iter()
            .flat_map(|(reader, declared)| {
                declared
                    .reads
                    .iter()
                    .chain(&declared.input)
                    .map(String::as_str)
                    .chain(declared.debt.stages())
                    .map(move |read| (reader.clone(), read.to_owned()))
            })
            .filter(|(reader, read)| {
                !reads
                    .get(reader)
                    .is_some_and(|found| found.contains_key(read))
            })
            .collect()
    }

    /// One refusal per breach of the law, each naming its fix.
    pub fn refusals(&self, name: &str) -> Vec<String> {
        let report = self;
        let mut violations = Vec::new();
        violations.extend(report.forward.iter().map(|edge| {
            let mut refusal = format!(
                "stage order `{name}`: {}:{} {} → {} names `{}`, a later stage; move the type to the stage that produces it, or amend the stage_order row if the order is wrong; never alias it",
                edge.file, edge.line, edge.from, edge.to, edge.path
            );
            if let Some(shared) = edge
                .path
                .rsplit("::")
                .next()
                .filter(|last| report.shared_names.contains(*last) && edge.path.matches("::").count() == 1)
            {
                let _ = write!(
                    refusal,
                    "; `{shared}` is both a module and a lib.rs re-export: if only the module is meant, write `use crate::{shared}::{{self}};`"
                );
            }
            refusal
        }));
        violations.extend(report.through_unplaced.iter().map(|edge| {
            format!(
                "stage order `{name}`: {}:{} {} reaches the later stage {} through unplaced {} ({}:{}); name each unplaced module in its stage_order row, then move the type to its producer",
                edge.reader.file,
                edge.reader.line,
                edge.reader.from,
                edge.onward.to,
                edge.via.join(" → "),
                edge.onward.file,
                edge.onward.line
            )
        }));
        violations.extend(report.unjudged.iter().map(|path| {
            format!(
                "stage order `{name}`: {}:{} `{}` is unjudged ({}); {}",
                path.file,
                path.line,
                path.path,
                path.why,
                path.why.remedy()
            )
        }));
        // A crate with seams reads only the floor whole: the remedy is an
        // input or a debt row, never a whole-stage read.
        let declare = if report.floor.is_empty() {
            "add `{}` to the row's reads with the reason in a comment"
        } else {
            "name `{}` as the row's input (its boundary items) or give it a debt row with its owner"
        };
        violations.extend(report.undeclared_reads().into_iter().map(|edge| {
            format!(
                "stage interface `{name}`: {}:{} {} reads {} (`{}`), which its stage_reads row does not list; read it through the stage's input, or {}",
                edge.file,
                edge.line,
                edge.from,
                edge.to,
                edge.path,
                declare.replace("{}", &edge.to)
            )
        }));
        violations.extend(report.unplaced.iter().map(|module| {
            format!(
                "stage order `{name}`: top-level module `{module}` is not in its stage_order row; add it at its place in the row in .config/architecture.toml, or move its items into the stages that produce them"
            )
        }));
        violations.extend(report.inline_modules.iter().map(|module| {
            format!(
                "stage order `{name}`: lib.rs declares inline module `{module}`, which the census cannot read; move it to its own file and name it in the stage_order row"
            )
        }));
        violations
    }
}

pub(super) fn interface_policy_violations(
    orders: &[StageOrder],
    interfaces: &BTreeMap<String, BTreeMap<String, policy::StageReads>>,
    seams: &BTreeMap<String, policy::StageSeams>,
    key: Option<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (crate_name, declared) in seams
        .iter()
        .filter(|(name, _)| key.is_none_or(|key| name.as_str() == key))
    {
        let Some(order) = orders.iter().find(|order| &order.crate_name == crate_name) else {
            violations.push(format!(
                "stage seams `{crate_name}`: stage_seams names a crate with no stage_order row; fix the crate name or add its stage_order row"
            ));
            continue;
        };
        for stage in declared
            .floor
            .iter()
            .filter(|stage| !order.stages.contains(stage))
        {
            violations.push(format!(
                "stage seams `{crate_name}`: floor names `{stage}`, which is not a stage in its stage_order row"
            ));
        }
    }
    for (crate_name, rows) in interfaces
        .iter()
        .filter(|(name, _)| key.is_none_or(|key| name.as_str() == key))
    {
        let Some(order) = orders.iter().find(|order| &order.crate_name == crate_name) else {
            violations.push(format!(
                "stage interface `{crate_name}`: stage_reads names a crate with no stage_order row; fix the crate name or add its stage_order row"
            ));
            continue;
        };
        let floor = seams.get(crate_name).map(|seams| &seams.floor);
        let rank = |stage: &str| order.stages.iter().position(|named| named == stage);
        for (reader, declared) in rows {
            let Some(reader_rank) = rank(reader) else {
                violations.push(format!(
                    "stage interface `{crate_name}`: `{reader}` is not a stage in its stage_order row"
                ));
                continue;
            };
            let listed: Vec<&str> = declared
                .reads
                .iter()
                .chain(&declared.input)
                .map(String::as_str)
                .chain(declared.debt.stages())
                .collect();
            // An input may also owe a debt row (its interior); a stage read
            // whole needs neither.
            let mut once = BTreeSet::new();
            let whole_or_owed = declared
                .reads
                .iter()
                .map(String::as_str)
                .chain(declared.debt.stages());
            for read in whole_or_owed
                .chain(
                    declared
                        .input
                        .iter()
                        .map(String::as_str)
                        .filter(|read| !declared.debt.contains(read)),
                )
                .filter(|read| !once.insert(*read))
            {
                violations.push(format!(
                    "stage interface `{crate_name}`: {reader} lists `{read}` twice among its reads, input and debt; keep one"
                ));
            }
            if let policy::StageDebt::Bare(stages) = &declared.debt
                && !stages.is_empty()
            {
                violations.push(format!(
                    "stage interface `{crate_name}`: {reader} names its debt bare; write each as a row, `{} = {{ items = N, owner = \"<bead>\" }}`, counting the distinct items `atlas seams` lists",
                    stages[0]
                ));
            }
            if let Some(floor) = floor {
                // The floor holds names and laws every stage shares: it
                // reads nothing but the floor.
                if floor.contains(reader) && !listed.is_empty() {
                    violations.push(format!(
                        "stage interface `{crate_name}`: {reader} is a floor stage and lists {}; a floor stage reads only the floor, so move what it needs below it or take {reader} out of the floor",
                        listed.join(", ")
                    ));
                }
                let named: BTreeSet<&str> = listed.iter().copied().collect();
                for read in named
                    .iter()
                    .filter(|read| floor.contains(&(**read).to_owned()))
                {
                    violations.push(format!(
                        "stage interface `{crate_name}`: {reader} lists `{read}`, a floor stage every stage reads whole; drop it from the row"
                    ));
                }
                for read in &declared.reads {
                    violations.push(format!(
                        "stage interface `{crate_name}`: {reader} reads `{read}` whole, but in a crate with item-level seams only the floor is read whole; name `{read}` as an input (its boundary items) or a debt row"
                    ));
                }
            }
            let mut ranked = BTreeSet::new();
            for read in listed.into_iter().filter(|read| ranked.insert(*read)) {
                match rank(read) {
                    None => violations.push(format!(
                        "stage interface `{crate_name}`: {reader} reads `{read}`, which is not a stage in its stage_order row"
                    )),
                    Some(read_rank) if read_rank >= reader_rank => violations.push(format!(
                        "stage interface `{crate_name}`: {reader} reads `{read}`, which is not upstream of it; a stage reads only earlier stages"
                    )),
                    Some(_) => {}
                }
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crate_map(files: &[(&str, &str)]) -> CrateMap {
        CrateMap {
            name: "murail-fix".to_owned(),
            src_rel: "crates/murail-fix/src".to_owned(),
            mounted: Vec::new(),
            files: files
                .iter()
                .map(|(rel, text)| {
                    FileMap::of(format!("crates/murail-fix/src/{rel}"), (*text).to_owned())
                })
                .collect(),
        }
    }

    fn root_of(files: &[(&str, &str)]) -> RootTable {
        RootTable::of(&crate_map(files), TestSelection::Production)
    }

    /// A row reading `reads` whole and owing one item of each `debt` stage.
    fn row(reads: &[&str], debt: &[&str]) -> policy::StageReads {
        policy::StageReads {
            reads: reads.iter().map(|s| (*s).to_owned()).collect(),
            input: Vec::new(),
            debt: policy::StageDebt::Rows(
                debt.iter()
                    .map(|stage| {
                        let row: policy::DebtRow =
                            toml::from_str("items = 1\nowner = 'murail-zhe09.29'")
                                .expect("fixture row");
                        ((*stage).to_owned(), row)
                    })
                    .collect(),
            ),
        }
    }

    fn order() -> Vec<String> {
        ["load", "schedule", "engine"].map(str::to_owned).to_vec()
    }

    fn forward(report: &StageReport) -> Vec<(&str, &str)> {
        report
            .forward
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect()
    }

    fn unjudged(report: &StageReport) -> Vec<(&str, Unjudged)> {
        report
            .unjudged
            .iter()
            .map(|path| (path.path.as_str(), path.why))
            .collect()
    }

    #[test]
    fn paths_resolve_from_the_declaring_module() {
        let root = root_of(&[("load.rs", ""), ("schedule.rs", ""), ("engine.rs", "")]);
        let none = BTreeSet::new();
        let read =
            |module: &[&str], path: &str| read_path(module, path, PathForm::Use, &root, &none);
        assert_eq!(
            read(&["load", "plan"], "super::super::schedule::X"),
            Ok(vec![vec!["schedule".into(), "X".into()]])
        );
        assert_eq!(
            read(&["load", "plan"], "super::Cursor"),
            Ok(vec![vec!["load".into(), "Cursor".into()]])
        );
        assert_eq!(
            read(&["load"], "crate::engine::Engine"),
            Ok(vec![vec!["engine".into(), "Engine".into()]])
        );
        assert_eq!(read(&["load"], "std::fmt"), Ok(vec![]));
        assert_eq!(
            inline_paths("let x = crate::schedule::plan(super::y);", &none),
            vec!["crate::schedule::plan", "super::y"]
        );
    }

    #[test]
    fn backward_imports_are_judged_and_pass() {
        let report = StageReport::of(
            &crate_map(&[
                ("load.rs", "use crate::host::Clock;\n"),
                ("schedule/plan.rs", "use crate::load::{Loaded, self};\n"),
                ("engine.rs", "fn run() { crate::schedule::plan::run(); }\n"),
                ("host.rs", "use crate::engine::Engine;\n"),
            ]),
            &order(),
        );
        assert!(report.forward.is_empty());
        assert_eq!(report.judged, 2);
        assert_eq!(report.unplaced, BTreeSet::from(["host".to_owned()]));
        assert!(report.absent.is_empty());
    }

    #[test]
    fn a_stage_interface_refuses_only_an_undeclared_read() {
        // engine reads schedule (declared) and load (debt); schedule reads
        // load (undeclared): the one refusal is schedule → load.
        let mut report = StageReport::of(
            &crate_map(&[
                ("load.rs", ""),
                ("schedule.rs", "use crate::load::Cursor;\n"),
                (
                    "engine.rs",
                    "use crate::schedule::Plan;\nuse crate::load::Loaded;\n",
                ),
            ]),
            &order(),
        );
        let declared = row;
        report.interfaces = Some(BTreeMap::from([
            ("engine".to_owned(), declared(&["schedule"], &["load"])),
            ("load".to_owned(), declared(&["schedule"], &[])),
        ]));
        let undeclared: Vec<(&str, &str)> = report
            .undeclared_reads()
            .into_iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect();
        assert_eq!(undeclared, [("schedule", "load")]);
        assert_eq!(
            report.unused_reads(),
            [("load".to_owned(), "schedule".to_owned())]
        );
        assert_eq!(report.refusals("murail-fix").len(), 1);
        // A crate that declares no interfaces is not judged.
        report.interfaces = None;
        assert!(report.undeclared_reads().is_empty());
        assert!(report.refusals("murail-fix").is_empty());
    }

    #[test]
    fn a_stage_interface_row_the_orders_cannot_read_is_refused() {
        let orders = vec![StageOrder {
            crate_name: "murail-fix".to_owned(),
            stages: order(),
        }];
        let good = BTreeMap::from([(
            "murail-fix".to_owned(),
            BTreeMap::from([("engine".to_owned(), row(&["schedule"], &["load"]))]),
        )]);
        let unseamed = BTreeMap::new();
        assert!(interface_policy_violations(&orders, &good, &unseamed, None).is_empty());
        for bad in [
            BTreeMap::from([("murail-fx".to_owned(), BTreeMap::new())]),
            BTreeMap::from([(
                "murail-fix".to_owned(),
                BTreeMap::from([("engin".to_owned(), row(&[], &[]))]),
            )]),
            BTreeMap::from([(
                "murail-fix".to_owned(),
                BTreeMap::from([("engine".to_owned(), row(&["shedule"], &[]))]),
            )]),
            BTreeMap::from([(
                "murail-fix".to_owned(),
                BTreeMap::from([("load".to_owned(), row(&["engine"], &[]))]),
            )]),
            BTreeMap::from([(
                "murail-fix".to_owned(),
                BTreeMap::from([("engine".to_owned(), row(&["load"], &["load"]))]),
            )]),
        ] {
            assert_eq!(
                interface_policy_violations(&orders, &bad, &unseamed, None).len(),
                1,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_seamed_row_reads_only_the_floor_whole_and_names_its_debt_in_rows() {
        let orders = vec![StageOrder {
            crate_name: "murail-fix".to_owned(),
            stages: order(),
        }];
        let seams: BTreeMap<String, policy::StageSeams> = BTreeMap::from([(
            "murail-fix".to_owned(),
            toml::from_str("floor = ['load']").expect("fixture seams"),
        )]);
        let rows = |text: &str| {
            BTreeMap::from([(
                "murail-fix".to_owned(),
                toml::from_str::<BTreeMap<String, policy::StageReads>>(text).expect("fixture rows"),
            )])
        };
        // An input may owe a row for its interior.
        let good = rows(
            "engine = { input = ['schedule'], debt = { schedule = { items = 2, owner = 'murail-zhe09.29' } } }",
        );
        assert!(interface_policy_violations(&orders, &good, &seams, None).is_empty());
        for bad in [
            "engine = { reads = ['schedule'] }",
            "engine = { input = ['load'] }",
            "engine = { debt = ['schedule'] }",
            "engine = { input = ['schedule'], reads = [] , debt = { shedule = { items = 1, owner = 'murail-zhe09.29' } } }",
        ] {
            assert_eq!(
                interface_policy_violations(&orders, &rows(bad), &seams, None).len(),
                1,
                "{bad}"
            );
        }
        // A floor stage reads only the floor.
        let schedule_floor: BTreeMap<String, policy::StageSeams> = BTreeMap::from([(
            "murail-fix".to_owned(),
            toml::from_str("floor = ['schedule']").expect("fixture seams"),
        )]);
        let floor_reads = interface_policy_violations(
            &orders,
            &rows("schedule = { input = ['load'] }"),
            &schedule_floor,
            None,
        );
        assert_eq!(floor_reads.len(), 1, "{floor_reads:#?}");
        assert!(floor_reads[0].contains("a floor stage reads only the floor"));
        let unknown_floor = BTreeMap::from([(
            "murail-fix".to_owned(),
            toml::from_str::<policy::StageSeams>("floor = ['lod']").expect("fixture seams"),
        )]);
        assert_eq!(
            interface_policy_violations(&orders, &BTreeMap::new(), &unknown_floor, None).len(),
            1
        );
    }

    #[test]
    fn a_forward_import_is_reported_with_its_site() {
        // The check can go red: the load stage reaching into the engine.
        let report = StageReport::of(
            &crate_map(&[
                (
                    "load/cursor.rs",
                    "use std::fmt;\nuse super::super::{\n    engine::Engine,\n};\n",
                ),
                (
                    "load.rs",
                    "#[cfg(test)]\nmod tests {\n    use crate::engine::Engine;\n}\n",
                ),
                ("engine.rs", "pub struct Engine;\n"),
            ]),
            &order(),
        );
        let sites: Vec<_> = report
            .forward
            .iter()
            .map(|edge| {
                (
                    edge.file.as_str(),
                    edge.line,
                    edge.from.as_str(),
                    edge.to.as_str(),
                )
            })
            .collect();
        // Test-only imports stay outside the production reading.
        assert_eq!(
            sites,
            vec![("crates/murail-fix/src/load/cursor.rs", 2, "load", "engine")]
        );
        assert_eq!(report.absent, vec!["schedule".to_owned()]);
    }

    #[test]
    fn a_root_re_export_cannot_hide_a_forward_edge() {
        // `crate::Engine` resolves through lib.rs's `pub use` to the engine stage.
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "pub use engine::Engine;\nmod engine;\nmod load;\n",
                ),
                ("load.rs", "use crate::Engine;\n"),
                ("engine.rs", "pub struct Engine;\n"),
            ]),
            &order(),
        );
        assert_eq!(forward(&report), vec![("load", "engine")]);
    }

    #[test]
    fn a_root_glob_is_reported_not_skipped() {
        let report = StageReport::of(
            &crate_map(&[
                ("load.rs", "use crate::*;\n"),
                ("schedule/plan.rs", "use super::super::*;\nuse super::*;\n"),
            ]),
            &order(),
        );
        let sites: Vec<_> = report
            .unjudged
            .iter()
            .map(|path| (path.file.as_str(), path.line, path.why))
            .collect();
        assert_eq!(
            sites,
            vec![
                ("crates/murail-fix/src/load.rs", 1, Unjudged::RootGlob),
                (
                    "crates/murail-fix/src/schedule/plan.rs",
                    1,
                    Unjudged::RootGlob
                ),
            ]
        );
    }

    #[test]
    fn a_forward_edge_through_an_unplaced_module_is_reported() {
        // load reads witness, which reads engine: load reaches engine in two hops.
        let report = StageReport::of(
            &crate_map(&[
                ("load.rs", "use crate::witness::Map;\n"),
                ("witness.rs", "use crate::engine::Engine;\n"),
                ("engine.rs", "use crate::witness::Map;\n"),
            ]),
            &order(),
        );
        assert!(report.forward.is_empty());
        let hops: Vec<_> = report
            .through_unplaced
            .iter()
            .map(|edge| {
                (
                    edge.reader.from.as_str(),
                    edge.via.join(" → "),
                    edge.onward.to.as_str(),
                )
            })
            .collect();
        assert_eq!(hops, vec![("load", "witness".to_owned(), "engine")]);
    }

    // murail-7soto.4: unplaced → unplaced → later is followed to its end.
    #[test]
    fn a_forward_edge_through_an_unplaced_chain_is_reported() {
        let report = StageReport::of(
            &crate_map(&[
                ("load.rs", "use crate::witness::Map;\n"),
                ("witness.rs", "use crate::ledger::Row;\n"),
                ("ledger.rs", "use crate::engine::Engine;\n"),
                ("engine.rs", "pub struct Engine;\n"),
            ]),
            &order(),
        );
        let hops: Vec<_> = report
            .through_unplaced
            .iter()
            .map(|edge| {
                (
                    edge.reader.from.as_str(),
                    edge.via.join(" → "),
                    edge.onward.to.as_str(),
                )
            })
            .collect();
        assert_eq!(
            hops,
            vec![("load", "witness → ledger".to_owned(), "engine")]
        );
    }

    // murail-7soto.4: `pub(crate) use alias::X as Y;` where `alias` is itself
    // a root re-export of a module is followed to the module's stage.
    #[test]
    fn a_multi_step_root_alias_cannot_hide_a_forward_edge() {
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod engine;\nmod load;\npub(crate) use engine::program_map;\npub(crate) use program_map::ProgramMap as Chained;\npub(crate) use Chained as Twice;\n",
                ),
                ("load.rs", "use crate::Twice;\n"),
                ("engine/program_map.rs", "pub struct ProgramMap;\n"),
            ]),
            &order(),
        );
        assert_eq!(forward(&report), vec![("load", "engine")]);
        assert!(report.unjudged.is_empty());
    }

    // murail-7soto.2: an alias of the crate root carries its paths with it.
    #[test]
    fn a_crate_alias_cannot_hide_a_forward_edge() {
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod engine;\nmod load;\nmod schedule;\npub(crate) use crate as root;\n",
                ),
                (
                    "load.rs",
                    "use crate as c;\nfn run() { let _ = c::engine::Engine; }\n",
                ),
                (
                    "schedule/plan.rs",
                    "use super::super as up;\nuse up::engine::Engine;\nuse crate::root::engine::Engine;\n",
                ),
                ("engine.rs", "pub struct Engine;\n"),
            ]),
            &order(),
        );
        let sites: Vec<_> = report
            .forward
            .iter()
            .map(|edge| (edge.from.as_str(), edge.line, edge.path.as_str()))
            .collect();
        assert_eq!(
            sites,
            vec![
                ("load", 2, "c::engine::Engine"),
                ("schedule", 2, "up::engine::Engine"),
                ("schedule", 3, "crate::root::engine::Engine"),
            ]
        );
    }

    // murail-7soto.2: a root name a lib.rs glob may supply is unjudged.
    #[test]
    fn a_name_under_a_root_glob_re_export_is_unjudged() {
        let files = [
            (
                "lib.rs",
                "mod engine;\nmod load;\npub use engine::program::*;\npub use std::fmt::*;\npub fn helper() {}\n",
            ),
            ("load.rs", "use crate::Engine;\nuse crate::helper;\n"),
            ("engine/program.rs", "pub struct Engine;\n"),
        ];
        let report = StageReport::of(&crate_map(&files), &order());
        assert_eq!(
            unjudged(&report),
            vec![("crate::Engine", Unjudged::GlobbedRootName)]
        );
        // Without the crate-internal glob the name is unknown, still unjudged.
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod engine;\nmod load;\npub use std::fmt::*;\npub fn helper() {}\n",
                ),
                files[1],
                files[2],
            ]),
            &order(),
        );
        assert_eq!(
            unjudged(&report),
            vec![("crate::Engine", Unjudged::UnknownRootName)]
        );
    }

    // murail-7soto.5: a root re-export that reuses a module's name reads as
    // the re-export when the path ends (a call), the module when it continues.
    #[test]
    fn a_re_export_named_like_a_module_resolves_by_namespace() {
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod engine;\nmod load;\nmod schedule;\npub use engine::compile::load;\n",
                ),
                ("load.rs", "pub struct Cursor;\n"),
                (
                    "schedule.rs",
                    "fn run() { let _ = crate::load::Cursor; crate::load(1); }\n",
                ),
                ("engine/compile.rs", "pub fn load(_: u8) {}\n"),
            ]),
            &order(),
        );
        let sites: Vec<_> = report
            .forward
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.path.as_str()))
            .collect();
        assert_eq!(sites, vec![("schedule", "engine", "crate::load")]);
        assert_eq!(report.judged, 2);
    }

    #[test]
    fn a_use_of_a_re_export_named_like_a_module_imports_both() {
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod engine;\nmod load;\nmod schedule;\npub use engine::compile::load;\n",
                ),
                ("load.rs", ""),
                ("schedule.rs", "use crate::load;\n"),
                ("engine/compile.rs", "pub fn load() {}\n"),
            ]),
            &order(),
        );
        assert_eq!(forward(&report), vec![("schedule", "engine")]);
        assert_eq!(report.judged, 2);
    }

    #[test]
    fn unnamed_modules_and_inline_root_modules_are_reported_test_files_are_not() {
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod load;\nmod extra;\nmod inline {\n    pub fn f() {}\n}\n#[cfg(test)]\nmod fixtures;\n#[cfg(test)]\nmod tests {\n}\n",
                ),
                ("load.rs", ""),
                ("extra.rs", ""),
                ("fixtures.rs", "mod nested;\n"),
                ("fixtures/nested.rs", "use crate::engine::Engine;\n"),
            ])
            .with_test_files(),
            &order(),
        );
        assert_eq!(report.unplaced, BTreeSet::from(["extra".to_owned()]));
        assert_eq!(report.inline_modules, BTreeSet::from(["inline".to_owned()]));
        assert!(report.forward.is_empty() && report.unjudged.is_empty());
    }

    #[test]
    fn two_globs_in_one_use_tree_stay_two_bindings() {
        let root = root_of(&[
            (
                "lib.rs",
                "mod engine;\nmod load;\npub use {engine::*, load::*, std::fmt::*};\n",
            ),
            ("load.rs", ""),
            ("engine.rs", ""),
        ]);
        assert_eq!(
            root.globs,
            vec!["engine::*".to_owned(), "load::*".to_owned()]
        );
    }

    const SHARED_LIB: &str =
        "mod engine;\nmod load;\nmod schedule;\npub use engine::compile::load;\n";

    // The critic's canonical case: a braced `self` imports only the module.
    #[test]
    fn a_braced_self_imports_only_the_module() {
        let report = StageReport::of(
            &crate_map(&[
                ("lib.rs", SHARED_LIB),
                ("load.rs", ""),
                ("schedule.rs", "use crate::load::{self};\n"),
                ("engine/compile.rs", "pub fn load() {}\n"),
            ]),
            &order(),
        );
        assert!(report.forward.is_empty());
        assert_eq!(report.judged, 1);
    }

    #[test]
    fn a_shared_name_refusal_names_the_braced_self_remedy() {
        let report = StageReport::of(
            &crate_map(&[
                ("lib.rs", SHARED_LIB),
                ("load.rs", ""),
                ("schedule.rs", "use crate::load;\n"),
                ("engine/compile.rs", "pub fn load() {}\n"),
            ]),
            &order(),
        );
        let refusals = report.refusals("murail-fix");
        assert_eq!(refusals.len(), 1);
        assert!(refusals[0].contains("amend the stage_order row if the order is wrong"));
        assert!(refusals[0].contains("write `use crate::load::{self};`"));
    }

    #[test]
    fn a_test_gated_item_is_exempt_and_an_ungated_one_is_refused() {
        let gated = "#[cfg(test)]\nuse crate::engine::Engine;\npub struct Cursor;\nimpl Cursor {\n    #[cfg(test)]\n    fn probe() {\n        crate::engine::run();\n    }\n}\n#[cfg(any(test, feature = \"strategies\"))]\nuse crate::engine::Strategy;\n";
        let report = StageReport::of(
            &crate_map(&[("load.rs", gated), ("engine.rs", "")]),
            &order(),
        );
        // `any(test, feature)` also compiles for the feature: production.
        let sites: Vec<_> = report.forward.iter().map(|edge| edge.line).collect();
        assert_eq!(sites, vec![11]);
        let report = StageReport::of(
            &crate_map(&[
                ("load.rs", "use crate::engine::Engine;\n"),
                ("engine.rs", ""),
            ]),
            &order(),
        );
        assert_eq!(forward(&report), vec![("load", "engine")]);
    }

    #[test]
    fn a_lib_type_alias_or_item_body_cannot_launder_an_edge() {
        let lib = "mod engine;\nmod load;\nmod schedule;\npub type Alias = engine::Engine;\npub type Boxed = Box<engine::Engine>;\npub fn helper() -> u8 {\n    engine::run()\n}\npub const LIMIT: usize = engine::LIMIT;\npub static TABLE: [u8; 1] = [schedule::ROW];\npub fn quiet() {}\n";
        let report = StageReport::of(
            &crate_map(&[
                ("lib.rs", lib),
                (
                    "load.rs",
                    "use crate::Alias;\nuse crate::Boxed;\nfn f() { crate::helper(); crate::quiet(); }\nconst L: usize = crate::LIMIT;\nuse crate::TABLE;\n",
                ),
                ("schedule.rs", ""),
                ("engine.rs", ""),
            ]),
            &order(),
        );
        let sites: Vec<_> = report
            .forward
            .iter()
            .map(|edge| (edge.path.as_str(), edge.to.as_str()))
            .collect();
        assert_eq!(
            sites,
            vec![
                ("crate::Alias", "engine"),
                ("crate::Boxed", "engine"),
                ("crate::helper", "engine"),
                ("crate::LIMIT", "engine"),
                ("crate::TABLE", "schedule"),
            ]
        );
        assert!(report.unjudged.is_empty());
    }

    #[test]
    fn a_lib_type_or_impl_cannot_launder_an_edge() {
        let lib = "mod engine;\nmod load;\nmod schedule;\npub struct W(pub engine::Engine);\npub trait T {\n    fn e(&self) -> &schedule::Plan;\n}\npub struct V;\nimpl V {\n    pub fn new() -> engine::Engine {\n        todo!()\n    }\n}\npub struct A(pub Box<B>);\npub struct B(pub Option<A>);\npub enum Quiet {\n    One,\n}\n";
        let report = StageReport::of(
            &crate_map(&[
                ("lib.rs", lib),
                (
                    "load.rs",
                    "use crate::W;\nuse crate::T;\nuse crate::V;\nuse crate::A;\nuse crate::Quiet;\n",
                ),
                ("schedule.rs", ""),
                ("engine.rs", ""),
            ]),
            &order(),
        );
        let sites: Vec<_> = report
            .forward
            .iter()
            .map(|edge| (edge.path.as_str(), edge.to.as_str()))
            .collect();
        assert_eq!(
            sites,
            vec![
                ("crate::W", "engine"),
                ("crate::T", "schedule"),
                ("crate::V", "engine"),
            ]
        );
        // Mutually naming items end the walk; they are judged, not unjudged.
        assert!(report.unjudged.is_empty());
    }

    #[test]
    fn an_exported_macro_ranks_by_its_defining_module() {
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod engine;\nmod load;\nmacro_rules! local {\n    () => {};\n}\n",
                ),
                (
                    "load.rs",
                    "fn f() {\n    crate::knock!();\n    crate::local!();\n}\n",
                ),
                (
                    "engine/macros.rs",
                    "#[macro_export]\nmacro_rules! knock {\n    () => {};\n}\n",
                ),
            ]),
            &order(),
        );
        assert_eq!(forward(&report), vec![("load", "engine")]);
        assert!(report.unjudged.is_empty());
    }

    #[test]
    fn each_unjudged_refusal_names_its_own_remedy() {
        let report = StageReport::of(
            &crate_map(&[
                (
                    "lib.rs",
                    "mod engine;\nmod load;\npub use engine::program::*;\n",
                ),
                ("load.rs", "use crate::Engine;\nuse crate::*;\n"),
                ("engine/program.rs", ""),
            ]),
            &order(),
        );
        let refusals = report.refusals("murail-fix");
        assert_eq!(refusals.len(), 2);
        assert!(refusals[0].contains("replace the lib.rs glob with named re-exports"));
        assert!(refusals[1].contains("import each name by its true path"));
    }

    impl CrateMap {
        fn with_test_files(mut self) -> Self {
            let flags = crate::discovery::classify_test_fixture(
                self.files
                    .iter()
                    .map(|file| (file.rel.clone(), std::sync::Arc::clone(&file.source)))
                    .collect(),
            );
            for (file, test_only) in self.files.iter_mut().zip(flags) {
                file.test_file = test_only;
            }
            self
        }
    }
    #[test]
    fn imported_module_and_value_names_are_distinct_when_declarations_prove_it() {
        let map = crate_map(&[
            (
                "lib.rs",
                "pub use crate::load::choose as Shared;\npub use crate::engine::shared as Shared;\n",
            ),
            ("load.rs", "pub fn choose() {}\n"),
            ("engine/shared.rs", "pub fn work() {}\n"),
        ]);
        let root = RootTable::of(&map, TestSelection::Production);
        assert_eq!(
            root.read(&["Shared"], PathForm::Inline, 0).unwrap(),
            [vec!["load".to_owned(), "choose".to_owned()]]
        );
        assert_eq!(
            root.read(&["Shared", "work"], PathForm::Inline, 0).unwrap(),
            [vec![
                "engine".to_owned(),
                "shared".to_owned(),
                "work".to_owned()
            ]]
        );
        let imports = root.read(&["Shared"], PathForm::Use, 0).unwrap();
        assert_eq!(imports.len(), 2);
        assert!(imports.iter().any(|path| path == &["load", "choose"]));
        assert!(imports.iter().any(|path| path == &["engine", "shared"]));
    }
    #[test]
    fn external_or_chained_aliases_do_not_invent_a_value_namespace() {
        for value in ["external::choose", "crate::Alias"] {
            let lib = format!(
                "pub use {value} as Shared;\npub use crate::engine::shared as Shared;\npub use crate::load::choose as Alias;\n"
            );
            let root = root_of(&[
                ("lib.rs", &lib),
                ("load.rs", "pub fn choose() {}\n"),
                ("engine/shared.rs", "pub fn work() {}\n"),
            ]);
            assert_eq!(
                root.read(&["Shared"], PathForm::Inline, 0).unwrap_err(),
                Unjudged::AmbiguousRootName
            );
        }
    }
    #[test]
    fn a_test_gated_value_proves_only_the_test_population_namespace() {
        let map = crate_map(&[
            (
                "lib.rs",
                "pub use crate::load::choose as Shared;\npub use crate::engine::shared as Shared;\n",
            ),
            ("load.rs", "#[cfg(test)]\npub fn choose() {}\n"),
            ("engine/shared.rs", "pub fn work() {}\n"),
        ]);
        assert_eq!(
            RootTable::of(&map, TestSelection::Production)
                .read(&["Shared"], PathForm::Inline, 0)
                .unwrap_err(),
            Unjudged::AmbiguousRootName
        );
        assert!(
            RootTable::of(&map, TestSelection::All)
                .read(&["Shared"], PathForm::Inline, 0)
                .is_ok()
        );
    }
    #[test]
    fn an_import_that_already_names_both_namespaces_cannot_prove_a_pair() {
        let root = root_of(&[
            (
                "lib.rs",
                "pub use crate::engine::shared as Shared;\npub use crate::load::choose as Shared;\n",
            ),
            ("engine/mod.rs", "pub fn shared() {}\n"),
            ("engine/shared.rs", "pub fn work() {}\n"),
            ("load.rs", "pub fn choose() {}\n"),
        ]);
        assert_eq!(
            root.read(&["Shared"], PathForm::Inline, 0).unwrap_err(),
            Unjudged::AmbiguousRootName
        );
    }
}
