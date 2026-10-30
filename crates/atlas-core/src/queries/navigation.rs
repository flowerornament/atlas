//! Population-aware native navigation over captured source, without project enrichment.
//! Visibility selects subjects; source-line accounting has its independent native boundary.
use crate::{
    Result,
    discovery::Snapshot,
    model::Diagnostic,
    population::{TestSelection, VisibilitySelection},
    source::ItemKind,
    workspace::{FileMap, Workspace},
};
use serde::Serialize;
use std::collections::BTreeMap;
mod cards;
mod changes;
mod dump;
mod relations;
pub use cards::{Edge, Module, ModuleContext, Name, Site};
pub use changes::{ChangedDeclaration, DeclarationDiff, DeclarationVersion, Incident, Side};
pub use dump::{BindingEvidence, Definition, Dump, Export, ExportKind};

/// Reusable navigation over one capture. No query reopens source files.
pub struct Navigation<'a> {
    snapshot: &'a Snapshot,
    workspace: Workspace,
    visibility: VisibilitySelection,
    relations: std::cell::OnceCell<std::result::Result<relations::Relations, String>>,
}
impl<'a> Navigation<'a> {
    pub fn new(snapshot: &'a Snapshot, visibility: VisibilitySelection) -> Result<Self> {
        Ok(Self {
            snapshot,
            workspace: Workspace::of(snapshot)?,
            visibility,
            relations: std::cell::OnceCell::new(),
        })
    }
    /// Workspace -> package -> directory -> file, preserving package-file observations.
    pub fn census(&self, key: Option<&str>) -> Result<Census> {
        let key = key.map(|key| key.trim_end_matches('/'));
        let (scope, rows) = match key {
            None => (
                "workspace (per crate)".into(),
                self.workspace
                    .crates
                    .iter()
                    .map(|package| {
                        let mut tally = Tally::default();
                        for file in &package.files {
                            tally.add_file(file, self.visibility);
                        }
                        Row {
                            key: package.name.clone(),
                            tally,
                        }
                    })
                    .collect(),
            ),
            Some(key) => {
                if let Some(package) = self.workspace.crate_named(key) {
                    (
                        format!("{key} (per module under {}/)", package.src_rel),
                        self.directory_rows(&package.src_rel),
                    )
                } else if self
                    .workspace
                    .crates
                    .iter()
                    .flat_map(|p| &p.files)
                    .any(|f| f.rel == key)
                {
                    return Ok(Census::Module(self.module(key)?));
                } else {
                    let rows = self.directory_rows(key);
                    if rows.is_empty() {
                        return Err(format!("`{key}` is neither a workspace crate, a source directory, nor a module path").into());
                    }
                    (format!("{key}/ (per child)"), rows)
                }
            }
        };
        let mut total = Tally::default();
        for row in &rows {
            total.absorb(row.tally);
        }
        Ok(Census::Rows(CensusRows {
            schema: 1,
            scope,
            population: self.visibility,
            rows,
            total,
            basis: self.basis(),
        }))
    }
    fn directory_rows(&self, prefix: &str) -> Vec<Row> {
        let mut children = BTreeMap::<String, Tally>::new();
        for file in self.workspace.crates.iter().flat_map(|p| &p.files) {
            let Some(rest) = file
                .rel
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_prefix('/'))
            else {
                continue;
            };
            let key = rest
                .split_once('/')
                .map_or_else(|| rest.to_owned(), |(segment, _)| format!("{segment}/"));
            children
                .entry(key)
                .or_default()
                .add_file(file, self.visibility);
        }
        children
            .into_iter()
            .map(|(key, tally)| Row { key, tally })
            .collect()
    }
    fn basis(&self) -> Basis {
        Basis {
            sources: self
                .snapshot
                .files(crate::population::FileSelection::SourceTree)
                .map(|file| (file.path.clone(), file.content_fingerprint.clone()))
                .collect(),
            diagnostics: self.snapshot.source_diagnostics().to_vec(),
            evidence: "Native source-tree lexical subjects; spelling references and written-use fans are approximate. Self and macro expansion are not resolved.",
        }
    }
}

/// Physical byte identities; package contexts remain explicit in report rows.
#[derive(Serialize)]
pub struct Basis {
    pub sources: BTreeMap<String, String>,
    pub diagnostics: Vec<Diagnostic>,
    pub evidence: &'static str,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Census {
    Rows(CensusRows),
    Module(Module),
}
#[derive(Serialize)]
pub struct CensusRows {
    pub schema: u32,
    pub scope: String,
    pub population: VisibilitySelection,
    pub rows: Vec<Row>,
    pub total: Tally,
    pub basis: Basis,
}
#[derive(Serialize)]
pub struct Row {
    pub key: String,
    #[serde(flatten)]
    pub tally: Tally,
}
/// Native units: aliases are use declarations; macros and lines ignore visibility.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub types: usize,
    pub fns: usize,
    pub aliases: usize,
    pub macros: usize,
    pub documented: usize,
    pub nontest_loc: usize,
    pub token_lines: usize,
}
impl Tally {
    fn add_file(&mut self, file: &FileMap, visibility: VisibilitySelection) {
        self.nontest_loc +=
            file.source.test_regions().non_test_loc() * usize::from(!file.test_file);
        self.token_lines += file.token_lines();
        for item in file.items(TestSelection::OutsideTestModules) {
            match item.kind {
                ItemKind::MacroRules => self.macros += 1,
                ItemKind::PubUse if visibility.admits(item.visibility) => self.aliases += 1,
                kind if census_kind(kind) && visibility.admits(item.visibility) => {
                    if kind == ItemKind::Fn {
                        self.fns += 1;
                    } else {
                        self.types += 1;
                    }
                    self.documented += usize::from(item.doc.is_some());
                }
                _ => {}
            }
        }
    }
    fn absorb(&mut self, other: Self) {
        self.types += other.types;
        self.fns += other.fns;
        self.aliases += other.aliases;
        self.macros += other.macros;
        self.documented += other.documented;
        self.nontest_loc += other.nontest_loc;
        self.token_lines += other.token_lines;
    }
    pub const fn doc_pct(self) -> Option<usize> {
        let items = self.types + self.fns;
        (self.documented * 100).checked_div(items)
    }
}
const fn census_kind(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Struct
            | ItemKind::Enum
            | ItemKind::Union
            | ItemKind::Trait
            | ItemKind::TypeAlias
            | ItemKind::Fn
    )
}
