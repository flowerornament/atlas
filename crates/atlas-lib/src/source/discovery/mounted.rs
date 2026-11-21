//! Native source-tree mounts: separate population, shared captured source bytes.
use super::{Loaded, has_symlink, module_files, normalize_path, rel_display};
use crate::{
    Result,
    model::Diagnostic,
    source::{Captured, syntax::ModuleDeclaration},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::Path,
    sync::Arc,
};

/// A source-tree declaration's module location for a captured physical file.
/// This records a policy population; it does not establish a Cargo namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
pub struct FileMount {
    pub path: String,
    pub module: String,
    /// Whether this mount compiles the file only as test code: a test-gated
    /// declaration on its chain, or a chain starting in a test file (a test
    /// or bench target root, or a test-only file in src). One file may be
    /// mounted once in each population.
    pub test_only: bool,
    /// The attributes written on each declaration of the mount chain, as
    /// written: a caller that declares test-support features reads a gate
    /// needing test or only those features as test code.
    pub gates: Vec<String>,
    /// The file the mount chain starts from, which settles `test_only`
    /// once the package's test files are marked.
    #[serde(skip)]
    pub(super) origin: String,
}

/// A required mount that could not supply source; candidates remain explicit.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct UnavailableMount {
    pub declaring_file: String,
    pub module: String,
    pub candidates: Vec<String>,
}
/// Native mounted-file population and its unavailable inputs, from one traversal.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct MountedSources {
    pub files: Vec<FileMount>,
    pub unavailable: Vec<UnavailableMount>,
}

/// Native physical-path module classification, independent of Cargo reachability.
pub fn module_path_of(src_rel: &str, rel: &str) -> Option<String> {
    let inside = rel.strip_prefix(src_rel)?.strip_prefix('/')?;
    let stem = inside.strip_suffix(".rs")?;
    let stem = stem.strip_suffix("/mod").unwrap_or(stem);
    (!matches!(stem, "lib" | "main" | "mod")).then(|| stem.replace('/', "::"))
}
/// Adapted from native outside_mounts: start at explicit mounts leaving src,
/// and at the module declarations of target roots outside it (a test
/// target's `mod common;` is `tests/common.rs` or `tests/common/mod.rs`),
/// then follow their declarations. Each physical source is captured once.
pub(super) fn outside_mounts(
    root: &Path,
    src_rel: &str,
    files: &[Loaded],
    sources: &mut BTreeMap<String, Arc<Captured>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<MountedSources> {
    let inside = format!("{src_rel}/");
    let mut pending = target_root_children(root, &inside, files, sources, diagnostics)?;
    pending.extend(mounts_leaving_src(src_rel, &inside, files));
    follow_mounts(root, pending, sources, diagnostics)
}

/// A mount still to follow: the file, its module path, the file declaring
/// it, the files on the path that reached it, the file the chain starts
/// from, and whether a declaration on the chain is test-gated.
struct Pending {
    rel: String,
    module: String,
    declaring_file: String,
    ancestry: BTreeSet<String>,
    origin: String,
    gated: bool,
    gates: Vec<String>,
}
impl Pending {
    /// The first mount of a chain, declared in `declaring` by `declaration`.
    fn first(
        rel: String,
        module: String,
        declaring: &Loaded,
        declaration: &ModuleDeclaration,
    ) -> Self {
        Self {
            rel,
            module,
            declaring_file: declaring.rel.clone(),
            ancestry: BTreeSet::from([declaring.rel.clone()]),
            origin: declaring.rel.clone(),
            gated: declaration.test_gated,
            gates: declaring
                .source
                .facts()
                .map(|source| gate(source, declaration))
                .into_iter()
                .collect(),
        }
    }
}
/// The attributes written on the item that declares `declaration`.
fn gate(source: &crate::source::Source, declaration: &ModuleDeclaration) -> String {
    source
        .outline()
        .iter()
        .find(|item| item.syntax_span.start == declaration.item)
        .map(|item| item.attribute_text(source.text()).to_owned())
        .unwrap_or_default()
}

/// A target root outside src reads its modules beside it, as a crate root.
/// Only children that capture are followed.
fn target_root_children(
    root: &Path,
    inside: &str,
    files: &[Loaded],
    sources: &mut BTreeMap<String, Arc<Captured>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Pending>> {
    let mut pending = Vec::new();
    for file in files.iter().filter(|file| !file.rel.starts_with(inside)) {
        let Some(source) = file.source.facts() else {
            continue;
        };
        for declaration in source.file_module_declarations() {
            let name = declaration.qualified_name();
            // A module with no file here is reported by mark_test_files.
            for child in module_files(&file.rel, true, declaration) {
                if capture_optional(root, &child, sources, diagnostics)?.is_some() {
                    pending.push(Pending::first(child, name.clone(), file, declaration));
                }
            }
        }
    }
    Ok(pending)
}

/// The explicit `#[path]` or conditional-path mounts of files inside src
/// whose candidate files lie outside it, named below the declaring module.
fn mounts_leaving_src(src_rel: &str, inside: &str, files: &[Loaded]) -> Vec<Pending> {
    let mut pending = Vec::new();
    for file in files.iter().filter(|file| file.rel.starts_with(inside)) {
        let parent = module_path_of(src_rel, &file.rel);
        let Some(source) = file.source.facts() else {
            continue;
        };
        for declaration in source.file_module_declarations() {
            if declaration.path.is_none() && declaration.alternatives.is_empty() {
                continue;
            }
            let name = declaration.qualified_name();
            for rel in module_files(&file.rel, false, declaration) {
                if !rel.starts_with(inside) {
                    let module = parent
                        .as_ref()
                        .map_or_else(|| name.clone(), |parent| format!("{parent}::{name}"));
                    pending.push(Pending::first(rel, module, file, declaration));
                }
            }
        }
    }
    pending
}

/// Capture each pending mount and follow its module declarations, stopping
/// at a cycle on the path that reached it. A mount whose file cannot be
/// captured, or a declaration none of whose candidates can, is unavailable.
fn follow_mounts(
    root: &Path,
    mut pending: Vec<Pending>,
    sources: &mut BTreeMap<String, Arc<Captured>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<MountedSources> {
    let mut mounts = BTreeSet::new();
    let mut unavailable = Vec::new();
    while let Some(Pending {
        rel,
        module,
        declaring_file,
        mut ancestry,
        origin,
        gated,
        gates,
    }) = pending.pop()
    {
        if !ancestry.insert(rel.clone()) {
            continue;
        }
        let Some(source) = capture_optional(root, &rel, sources, diagnostics)? else {
            unavailable.push(UnavailableMount {
                declaring_file,
                module,
                candidates: vec![rel],
            });
            continue;
        };
        if !mounts.insert(FileMount {
            path: rel.clone(),
            module: module.clone(),
            test_only: gated,
            gates: gates.clone(),
            origin: origin.clone(),
        }) {
            continue;
        }
        let Some(source) = source.facts() else {
            continue;
        };
        for declaration in source.file_module_declarations() {
            let name = declaration.qualified_name();
            let children = module_files(&rel, false, declaration);
            let mut found = false;
            for child in &children {
                if capture_optional(root, child, sources, diagnostics)?.is_some() {
                    found = true;
                    pending.push(Pending {
                        rel: child.clone(),
                        module: format!("{module}::{name}"),
                        declaring_file: rel.clone(),
                        ancestry: ancestry.clone(),
                        origin: origin.clone(),
                        gated: gated || declaration.test_gated,
                        gates: gates
                            .iter()
                            .cloned()
                            .chain([gate(source, declaration)])
                            .collect(),
                    });
                }
            }
            if !found {
                unavailable.push(UnavailableMount {
                    declaring_file: rel.clone(),
                    module: format!("{module}::{name}"),
                    candidates: children,
                });
            }
        }
    }
    Ok(MountedSources {
        files: mounts.into_iter().collect(),
        unavailable,
    })
}

/// Settle each mount's population once `test_file(origin)` is known: a
/// gated chain, or one starting in a test file, is test-only. A file
/// mounted at one module in both ways keeps one mount per population.
pub(super) fn settle_populations(mounts: &mut MountedSources, test_file: impl Fn(&str) -> bool) {
    for mount in &mut mounts.files {
        mount.test_only = mount.test_only || test_file(&mount.origin);
    }
    mounts.files.sort();
    mounts
        .files
        .dedup_by(|a, b| a.path == b.path && a.module == b.module && a.test_only == b.test_only);
}

pub(super) fn module_candidates(directory: &str, name: &str) -> [String; 2] {
    let root = Path::new(directory);
    [
        root.join(format!("{name}.rs")),
        root.join(name).join("mod.rs"),
    ]
    .map(|path| super::path_key(&path))
}

fn capture_optional(
    root: &Path,
    rel: &str,
    sources: &mut BTreeMap<String, Arc<Captured>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<Arc<Captured>>> {
    if let Some(source) = sources.get(rel) {
        return Ok(Some(Arc::clone(source)));
    }
    let path = normalize_path(&root.join(rel));
    if !path.starts_with(root) || has_symlink(root, &path)? {
        diagnostics.push(Diagnostic {
            code: crate::model::DiagnosticCode::SourceExcluded,
            file: rel.to_owned(),
            line: None,
            package: None,
            message: "source mount outside workspace or through a symlink excluded".into(),
        });
        return Ok(None);
    }
    match fs::read_to_string(&path) {
        Ok(text) => {
            let source = Arc::new(crate::source::capture(root, rel, text));
            sources.insert(rel.to_owned(), Arc::clone(&source));
            Ok(Some(source))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(format!("capture source mount {}: {error}", rel_display(root, &path)).into())
        }
    }
}
