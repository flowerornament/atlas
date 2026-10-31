//! Crate-root inventories and the existing textual binding reader.
//! Construction owns root facts; this file keeps their lookup behavior together.
use super::paths::{
    PathForm, SourceLine, crate_paths, identifier_paths, inline_paths, logical_lines, read_path,
    root_aliases,
};
use super::{CrateMap, Edge, FileMap, ItemKind, Unjudged, UnjudgedPath};
use crate::{discovery::module_path_of, population::TestSelection};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};
/// Bound of the native textual alias reading; beyond it, the path is unjudged.
const ALIAS_DEPTH: usize = 16;
/// The crate root as lib.rs presents it: the top-level modules, the names
/// lib.rs binds by `use` (local name to source path), the crate modules it
/// glob re-exports, the names it defines itself, the paths its fn, const,
/// static and type-alias items name, the `#[macro_export]` macros with their
/// defining modules, and its production inline modules, whose code the
/// reading does not see.
pub(in crate::checks) struct RootTable {
    pub(super) modules: BTreeSet<String>,
    pub(super) bindings: BTreeMap<String, BTreeSet<String>>,
    pub(super) unreadable_imports: Vec<(usize, String)>,
    pub(super) globs: Vec<String>,
    pub(super) items: BTreeSet<String>,
    pub(super) item_paths: BTreeMap<String, Vec<String>>,
    /// The lib.rs items being read, so mutually naming items end the walk
    /// instead of exhausting [`ALIAS_DEPTH`].
    pub(super) reading: RefCell<BTreeSet<String>>,
    pub(super) exported_macros: BTreeMap<String, String>,
    pub(super) inline_modules: BTreeSet<String>,
    /// `extern crate self as NAME;`: NAME is the crate root in every file.
    pub(super) self_names: BTreeSet<String>,
    /// Every file module's path from the root (`["cover", "timing"]`).
    pub(super) module_paths: BTreeSet<Vec<String>>,
    /// Directly declared value-only names, not inferred import or type kinds.
    pub(super) value_paths: BTreeSet<Vec<String>>,
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
    pub(super) fn read(
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
    pub(super) fn names_root(&self, segments: &[&str], depth: usize) -> bool {
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
    pub(super) fn edges(
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
