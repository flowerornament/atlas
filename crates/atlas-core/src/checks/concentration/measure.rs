//! Native body-line, module-home and alias units over retained source facts.
use super::super::policy::{Alias, AliasKind, ConcentrationPolicy, LineRow};
use super::super::{FileMap, Workspace};
use super::{HeaderCitation, NameHomes, Rows};
use crate::{
    discovery::child_directory,
    model::Diagnostic,
    population::TestSelection,
    source::{ItemKind, Visibility},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
/// One tree's concentration subjects over a declared population.
#[derive(Default, Debug, Serialize)]
pub struct Measure {
    /// Every entry file, with its body lines.
    pub entry: BTreeMap<String, usize>,
    /// Every production file, with its body lines.
    pub file: BTreeMap<String, usize>,
    /// Each non-conventional module name with more than one home, each home
    /// a module id (`murail-compiler::select::selection`), so a file layout
    /// change (`plan.rs` to `plan/mod.rs`) moves no home.
    pub homes: BTreeMap<String, BTreeSet<String>>,
    pub aliases: BTreeSet<Alias>,
    /// Per population crate, its uncited folder roots.
    pub uncited: BTreeMap<String, BTreeSet<String>>,
    /// Every production package context of each physical path.
    pub crates_of: BTreeMap<String, BTreeSet<String>>,
    /// Every production module id.
    pub modules: BTreeSet<String>,
    /// Production `#[path]` module declarations, as `file:line`.
    pub remapped: Vec<String>,
    pub sources: BTreeMap<String, String>,
    pub unavailable: Vec<Diagnostic>,
}

impl Measure {
    pub(super) fn of(
        ws: &Workspace,
        policy: &ConcentrationPolicy,
        headers: &BTreeMap<String, HeaderCitation>,
        test_support_features: &[&str],
    ) -> Self {
        let mut measure = Self {
            entry: BTreeMap::new(),
            file: BTreeMap::new(),
            homes: BTreeMap::new(),
            aliases: BTreeSet::new(),
            uncited: BTreeMap::new(),
            crates_of: BTreeMap::new(),
            modules: BTreeSet::new(),
            remapped: Vec::new(),
            sources: BTreeMap::new(),
            unavailable: Vec::new(),
        };
        for crate_map in ws
            .crates
            .iter()
            .filter(|crate_map| policy.in_population(&crate_map.name))
        {
            let files: Vec<(Option<String>, &FileMap)> = crate_map.production_modules().collect();
            // How many production files sit under each directory.
            let mut under: BTreeMap<&str, usize> = BTreeMap::new();
            for (_, file) in &files {
                let mut directory = file.rel.as_str();
                while let Some((parent, _)) = directory.rsplit_once('/') {
                    *under.entry(parent).or_default() += 1;
                    directory = parent;
                }
            }
            // Alias identity follows Rust modules, including inline declarations;
            // file/body and collision measurements below remain physical files.
            let logical_modules: BTreeSet<String> = files
                .iter()
                .flat_map(|(module, file)| {
                    module
                        .iter()
                        .cloned()
                        .chain(declared_modules(file, module.as_deref()))
                })
                .collect();
            let modules: BTreeSet<&str> = logical_modules.iter().map(String::as_str).collect();
            let top_modules: BTreeSet<&str> = modules
                .iter()
                .map(|module| module.split("::").next().unwrap_or(module))
                .collect();
            let uncited = measure.uncited.entry(crate_map.name.clone()).or_default();
            for (module, file) in &files {
                let id = module.as_ref().map_or_else(
                    || crate_map.name.clone(),
                    |module| format!("{}::{module}", crate_map.name),
                );
                let fingerprint = crate::discovery::fingerprint(file.source.text().as_bytes());
                measure
                    .sources
                    .insert(file.rel.clone(), fingerprint.clone());
                let lines = body_lines(file);
                measure.modules.insert(id.clone());
                measure
                    .remapped
                    .extend(remapped_modules(file, test_support_features));
                measure.file.insert(file.rel.clone(), lines);
                measure
                    .crates_of
                    .entry(file.rel.clone())
                    .or_default()
                    .insert(crate_map.name.clone());
                let children = child_directory(&file.rel);
                let own = usize::from(
                    file.rel.rsplit_once('/').map(|(parent, _)| parent) == Some(children.as_str()),
                );
                let entry = under.get(children.as_str()).copied().unwrap_or(0) > own;
                if entry {
                    measure.entry.insert(file.rel.clone(), lines);
                }
                if entry || module.is_none() {
                    match headers.get(&file.rel) {
                        Some(reading) if reading.content_fingerprint == fingerprint => {
                            if !reading.cited {
                                uncited.insert(file.rel.clone());
                            }
                        }
                        supplied => measure.unavailable.push(Diagnostic {
                            package: Some(crate_map.name.clone()),
                            file: file.rel.clone(),
                            line: None,
                            message: if supplied.is_some() {
                                "header-citation observation has stale source custody".into()
                            } else {
                                "required project header-citation observation unavailable".into()
                            },
                        }),
                    }
                }
                if let Some(module) = module {
                    let name = module.rsplit("::").next().unwrap_or(module);
                    if !policy.conventional_names.iter().any(|known| known == name) {
                        measure
                            .homes
                            .entry(name.to_owned())
                            .or_default()
                            .insert(id.clone());
                    }
                }
                match aliases(file, &id, module.is_none(), &modules, &top_modules) {
                    Ok(aliases) => measure.aliases.extend(aliases),
                    Err(why) => measure.unavailable.push(Diagnostic {
                        package: Some(crate_map.name.clone()),
                        file: file.rel.clone(),
                        line: None,
                        message: why,
                    }),
                }
            }
        }
        measure.homes.retain(|_, homes| homes.len() > 1);
        measure
    }

    pub(super) fn over(lines: &BTreeMap<String, usize>, limit: usize) -> usize {
        lines
            .values()
            .map(|lines| lines.saturating_sub(limit))
            .sum()
    }

    /// The rows the scan would write: what a fresh section would record.
    pub fn rows(&self, policy: &ConcentrationPolicy) -> Rows {
        let over = |lines: &BTreeMap<String, usize>, limit| {
            lines
                .iter()
                .filter(|(_, count)| **count > limit)
                .map(|(path, lines)| LineRow {
                    path: path.clone(),
                    lines: *lines,
                })
                .collect()
        };
        Rows {
            entry: over(&self.entry, policy.entry_lines),
            file: over(&self.file, policy.file_lines),
            name: self
                .homes
                .iter()
                .map(|(name, homes)| NameHomes {
                    name: name.clone(),
                    homes: homes.clone(),
                })
                .collect(),
            alias: self.aliases.clone(),
            header_uncited: self.uncited.clone(),
        }
    }
}

/// A file's body lines: its token lines (the `[size]` unit) less the
/// navigation lines, so that dividing a body never taxes it. Navigation is
/// each `mod x;` declaration and each `use` or `pub use` declaration, with
/// their attributes and docs; the `//!` header is a comment and never counted.
/// `[size]` still counts every line.
pub(super) fn body_lines(file: &FileMap) -> usize {
    let navigation: Vec<_> = file
        .source
        .outline()
        .iter()
        .filter(|item| !file.source.test_regions().covers(item.line))
        .filter(|item| match item.kind {
            ItemKind::Use | ItemKind::PubUse => true,
            ItemKind::Mod => item.declaration_block.trim_end().ends_with(';'),
            _ => false,
        })
        .map(|item| &item.span)
        .collect();
    file.code_lines(TestSelection::OutsideTestModules)
        .filter(|(line, code)| {
            !code.trim().is_empty() && !navigation.iter().any(|span| span.contains(line))
        })
        .count()
}

/// A file's production `#[path]` module declarations, as `file:line`. A
/// remapped module hides from the layout the scan reads, so only a module
/// gated to test builds or to a test-support feature may use one.
fn remapped_modules(file: &FileMap, features: &[&str]) -> Vec<String> {
    let modules: Vec<_> = file
        .source
        .outline()
        .iter()
        .filter(|item| item.kind == ItemKind::Mod && !file.source.test_regions().covers(item.line))
        .collect();
    if modules.is_empty() {
        return Vec::new();
    }
    let text: Vec<&str> = file.source.text().lines().collect();
    modules
        .into_iter()
        .filter_map(|item| {
            let attributes: Vec<&str> = (*item.span.start()..item.line)
                .filter_map(|line| text.get(line - 1).map(|code| code.trim()))
                .collect();
            let remap = attributes.iter().any(|line| line.starts_with("#[path"));
            let test_support = file.source.individually_test_gated(item.line)
                || crate::source::attributes_require_test_or_features(
                    &attributes.join("\n"),
                    features,
                );
            (remap && !test_support).then(|| format!("{}:{}", file.rel, item.line))
        })
        .collect()
}

/// Module declarations reachable through module scopes, from the scanner's
/// production outline. Function, trait and macro bodies do not declare paths
/// reachable from the crate root. Test regions share the file census's boundary.
fn declared_modules(file: &FileMap, module: Option<&str>) -> Vec<String> {
    let mut paths = Vec::new();
    let mut enclosing: Vec<&crate::source::Item> = Vec::new();
    for item in file.source.outline() {
        while enclosing
            .last()
            .is_some_and(|outer| !outer.span.contains(&item.line))
        {
            enclosing.pop();
        }
        if item.kind == ItemKind::Mod
            && !file.test_file
            && !file.source.test_regions().covers(item.line)
            && enclosing.iter().all(|outer| outer.kind == ItemKind::Mod)
        {
            let mut path: Vec<&str> = module.into_iter().collect();
            path.extend(enclosing.iter().map(|outer| outer.name.as_str()));
            path.push(&item.name);
            paths.push(path.join("::"));
        }
        enclosing.push(item);
    }
    paths
}

/// The re-exports of one production file that alias an item: renames
/// anywhere, and at a crate root the module relays, shadows and internal
/// `use` bindings. The `macro_rules` export idiom (`pub(crate) use name;`
/// beside its macro) is exempt.
fn aliases(
    file: &FileMap,
    module: &str,
    root: bool,
    modules: &BTreeSet<&str>,
    top_modules: &BTreeSet<&str>,
) -> Result<Vec<Alias>, String> {
    let macros: BTreeSet<_> = file
        .items(TestSelection::OutsideTestModules)
        .filter(|item| item.kind == ItemKind::MacroRules)
        .map(|item| item.name.as_str())
        .collect();
    let mut aliases = Vec::new();
    for item in file
        .items(TestSelection::OutsideTestModules)
        .filter(|item| item.kind == ItemKind::PubUse)
    {
        let import = file
            .source
            .import_at(item.line)
            .ok_or_else(|| format!("captured import unavailable at {}:{}", file.rel, item.line))?;
        let bindings = import.bindings.as_ref().map_err(|why| {
            format!(
                "required alias syntax unavailable at {}:{}: {why}",
                file.rel, item.line
            )
        })?;
        let at_root = root && item.top_level;
        let restricted = item.visibility != Visibility::Pub;
        for binding in bindings {
            let mut kinds = Vec::new();
            let named = !matches!(binding.local_name.as_str(), "*" | "_");
            if named && binding.local_name != binding.source {
                kinds.push(AliasKind::Rename);
            }
            if at_root && named {
                let rooted = binding
                    .source_path
                    .strip_prefix("crate::")
                    .or_else(|| binding.source_path.strip_prefix("self::"))
                    .unwrap_or(&binding.source_path);
                if rooted.contains("::") && modules.contains(rooted) {
                    kinds.push(AliasKind::ModuleRelay);
                }
                if top_modules.contains(binding.local_name.as_str()) && rooted != binding.local_name
                {
                    kinds.push(AliasKind::RootShadow);
                }
                let idiom = !binding.source_path.contains("::")
                    && macros.contains(binding.source_path.as_str());
                if restricted && !idiom {
                    kinds.push(AliasKind::RootRestricted);
                }
            }
            aliases.extend(kinds.into_iter().map(|kind| Alias {
                module: module.to_owned(),
                kind,
                binding: binding.local_name.clone(),
                source: binding.source_path.clone(),
            }));
        }
    }
    Ok(aliases)
}
