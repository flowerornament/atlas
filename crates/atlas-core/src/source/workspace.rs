//! Package views borrow the snapshot's authority; no discovery or source parse here.
use crate::{
    discovery::{Snapshot, module_path_of},
    population::{FileSelection, TestSelection},
    source::Source,
};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) struct Workspace {
    pub crates: Vec<CrateMap>,
    pub unavailable: Vec<crate::model::Diagnostic>,
}
pub(crate) struct CrateMap {
    pub name: String,
    pub src_rel: String,
    pub files: Vec<FileMap>,
    pub mounted: Vec<ModuleMount>,
}
pub(crate) struct ModuleMount {
    pub module: String,
    pub file: FileMap,
}
pub(crate) struct FileMap {
    pub rel: String,
    pub source: Arc<Source>,
    pub test_file: bool,
}
impl Workspace {
    pub fn of(snapshot: &Snapshot) -> crate::Result<Self> {
        let root = snapshot.metadata().workspace_root.as_std_path();
        let mut files = BTreeMap::<_, Vec<_>>::new();
        for file in snapshot.files(FileSelection::SourceTree) {
            files
                .entry(file.crate_name.as_str())
                .or_default()
                .push(file);
        }
        let mut crates = Vec::new();
        for package in snapshot.metadata().workspace_packages() {
            let directory = package
                .manifest_path
                .as_std_path()
                .parent()
                .ok_or("manifest lacks parent")?
                .join("src");
            let src_rel = directory
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let mut captured = Vec::new();
            for file in files.get(package.name.as_str()).into_iter().flatten() {
                captured.push(FileMap {
                    rel: file.path.clone(),
                    source: snapshot
                        .retain_source(&file.path)
                        .ok_or("captured file lacks source facts")?,
                    test_file: file.test_only,
                });
            }
            captured.sort_by(|a, b| std::path::Path::new(&a.rel).cmp(std::path::Path::new(&b.rel)));
            let mounts = snapshot
                .mounted_sources(package.name.as_str())
                .ok_or("package lacks captured native mount population")?;
            let mounted = mounts
                .files
                .iter()
                .map(|mount| {
                    Ok(ModuleMount {
                        module: mount.module.clone(),
                        file: FileMap {
                            rel: mount.path.clone(),
                            source: snapshot
                                .retain_source(&mount.path)
                                .ok_or("mount lacks captured source")?,
                            test_file: true,
                        },
                    })
                })
                .collect::<crate::Result<Vec<_>>>()?;
            crates.push(CrateMap {
                name: package.name.to_string(),
                src_rel,
                files: captured,
                mounted,
            });
        }
        crates.sort_by(|a, b| a.name.cmp(&b.name));
        let unavailable = snapshot
            .graph()
            .diagnostics
            .iter()
            .filter(|issue| snapshot.source_facts(&issue.file).is_none())
            .cloned()
            .collect();
        Ok(Self {
            crates,
            unavailable,
        })
    }
    pub fn crate_named(&self, name: &str) -> Option<&CrateMap> {
        self.crates.iter().find(|package| package.name == name)
    }
}
impl CrateMap {
    pub fn production_modules(&self) -> impl Iterator<Item = (Option<String>, &FileMap)> {
        self.files
            .iter()
            .filter(|file| !file.test_file)
            .map(|file| (module_path_of(&self.src_rel, &file.rel), file))
    }
}
impl FileMap {
    /// Native size unit: nonblank projected lines outside test modules.
    /// Individually gated helpers retain the native accounting population.
    pub fn token_lines(&self) -> usize {
        self.code_lines(TestSelection::OutsideTestModules)
            .filter(|(_, code)| !code.trim().is_empty())
            .count()
    }
    /// Native lexical token unit, including group delimiters, outside test modules.
    pub fn token_count(&self) -> crate::Result<usize> {
        fn count(stream: proc_macro2::TokenStream) -> usize {
            stream
                .into_iter()
                .map(|token| match token {
                    proc_macro2::TokenTree::Group(group) => 2 + count(group.stream()),
                    _ => 1,
                })
                .sum()
        }
        let text = self
            .source
            .text()
            .lines()
            .enumerate()
            .filter(|(index, _)| {
                !self.test_file
                    && TestSelection::OutsideTestModules.admits_line(&self.source, index + 1)
            })
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n");
        text.parse::<proc_macro2::TokenStream>()
            .map(count)
            .map_err(|error| format!("tokenize {}: {error}", self.rel).into())
    }
    pub fn production_code_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.code_lines(TestSelection::Production)
    }
    pub fn code_lines(&self, selection: TestSelection) -> impl Iterator<Item = (usize, &str)> {
        self.source
            .projection()
            .code_lines()
            .enumerate()
            .filter(move |(index, _)| {
                selection.admits_file(self.test_file)
                    && selection.admits_line(&self.source, index + 1)
            })
            .map(|(index, line)| (index + 1, line))
    }
    pub fn production_items(&self) -> impl Iterator<Item = &crate::source::Item> {
        self.items(TestSelection::Production)
    }
    pub fn items(&self, selection: TestSelection) -> impl Iterator<Item = &crate::source::Item> {
        self.source.outline().iter().filter(move |item| {
            item.kind.declares()
                && selection.admits_file(self.test_file)
                && selection.admits_line(&self.source, item.line)
        })
    }
    #[cfg(test)]
    pub fn of(rel: String, text: String) -> Self {
        Self {
            source: Arc::new(Source::of(std::path::Path::new(&rel), text)),
            rel,
            test_file: false,
        }
    }
}
