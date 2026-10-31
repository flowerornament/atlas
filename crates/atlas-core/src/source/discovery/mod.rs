//! Physical-file discovery and Atlas's inherited test-module boundary.
use crate::{
    Result,
    model::{Declaration, Diagnostic, SourceFile, SourceGraph},
};
use cargo_metadata::{Metadata, MetadataCommand, TargetKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

mod mounted;
pub use mounted::module_path_of;
pub use mounted::{FileMount, MountedSources, UnavailableMount};

struct Loaded {
    rel: String,
    source: Arc<crate::source::Source>,
    test_file: bool,
}

/// Scan every physical Rust file under each workspace member's src directory.
/// Cargo target roots outside src are included and diagnosed as limited mounts.
/// Symlinked directories are not followed; generated/macro-expanded source is absent.
pub fn scan_workspace(root: &Path) -> Result<SourceGraph> {
    let metadata = MetadataCommand::new().current_dir(root).no_deps().exec()?;
    scan_metadata(&metadata)
}

/// Scan the population from an already resolved Cargo workspace.
pub fn scan_metadata(metadata: &Metadata) -> Result<SourceGraph> {
    read_metadata(metadata, &mut BTreeMap::new()).map(|(graph, _, _)| graph)
}

/// Source bytes and their derived lexical graph from one discovery pass.
pub struct Snapshot {
    graph: SourceGraph,
    sources: BTreeMap<String, Arc<crate::source::Source>>,
    metadata: Metadata,
    mounted: BTreeMap<String, MountedSources>,
    source_diagnostics: Vec<Diagnostic>,
}

impl Snapshot {
    /// Cargo context captured with these source bytes; later input mutations do not alter it.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    /// Discovery, membership and import-syntax diagnostics before any relationship query.
    pub fn source_diagnostics(&self) -> &[Diagnostic] {
        &self.source_diagnostics
    }
    /// Derived graph for the captured source bytes.
    pub fn graph(&self) -> &SourceGraph {
        &self.graph
    }
    /// Lookup in the capture's sorted package observations. Prefer production
    /// without rebuilding the physical inventory for every compared file.
    pub fn physical_file(&self, path: &str) -> Option<&SourceFile> {
        let start = self
            .graph
            .files
            .partition_point(|file| file.path.as_str() < path);
        self.graph.files[start..]
            .iter()
            .take_while(|file| file.path == path)
            .min_by_key(|file| file.test_only)
    }
    /// Select package-file observations without changing source or test evidence.
    pub fn files(
        &self,
        selection: crate::population::FileSelection,
    ) -> impl Iterator<Item = &SourceFile> {
        let roots: BTreeMap<_, _> = self
            .metadata
            .workspace_packages()
            .into_iter()
            .filter_map(|package| {
                Some((
                    package.name.as_str(),
                    normalize_path(package.manifest_path.as_std_path().parent()?).join("src"),
                ))
            })
            .collect();
        self.graph.files.iter().filter(move |file| match selection {
            crate::population::FileSelection::Discovered => true,
            crate::population::FileSelection::SourceTree => {
                roots.get(file.crate_name.as_str()).is_some_and(|root| {
                    self.metadata
                        .workspace_root
                        .as_std_path()
                        .join(&file.path)
                        .starts_with(root)
                })
            }
        })
    }
    /// Native mounted population, including unavailable required inputs.
    /// None means the named package is not part of this snapshot.
    pub fn mounted_sources(&self, package: &str) -> Option<&MountedSources> {
        self.mounted.get(package)
    }
    pub(crate) fn captured_sources(
        &self,
    ) -> impl Iterator<Item = (String, Arc<crate::source::Source>)> + '_ {
        self.sources
            .iter()
            .map(|(path, source)| (path.clone(), Arc::clone(source)))
    }
    /// Rich lexical facts by normalized workspace-relative physical path.
    pub fn source_facts(&self, path: &str) -> Option<&crate::source::Source> {
        self.sources.get(path).map(AsRef::as_ref)
    }
    pub(crate) fn retain_source(&self, path: &str) -> Option<Arc<crate::source::Source>> {
        self.sources.get(path).map(Arc::clone)
    }
    /// Captured source bytes by normalized workspace-relative physical path.
    pub fn source(&self, path: &str) -> Option<&str> {
        self.sources.get(path).map(|source| source.text())
    }
}

/// Discover and read each source file once, retaining the bytes for other analyses.
pub fn snapshot_metadata(metadata: &Metadata) -> Result<Snapshot> {
    capture_metadata(metadata.clone())
}
/// Capture Cargo context and source in one owned read; no caller metadata copy is needed.
pub fn snapshot_workspace(root: &Path) -> Result<Snapshot> {
    let metadata = MetadataCommand::new().current_dir(root).no_deps().exec()?;
    capture_metadata(metadata)
}
fn capture_metadata(metadata: Metadata) -> Result<Snapshot> {
    let mut sources = BTreeMap::new();
    let (graph, mounted, source_diagnostics) = read_metadata(&metadata, &mut sources)?;
    Ok(Snapshot {
        graph,
        sources,
        metadata,
        mounted,
        source_diagnostics,
    })
}

fn read_metadata(
    metadata: &Metadata,
    captured: &mut BTreeMap<String, Arc<crate::source::Source>>,
) -> Result<(
    SourceGraph,
    BTreeMap<String, MountedSources>,
    Vec<Diagnostic>,
)> {
    let root = metadata.workspace_root.as_std_path().canonicalize()?;
    let mut packages = metadata.workspace_packages();
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    let member_names: BTreeSet<_> = packages.iter().map(|p| p.name.to_string()).collect();
    let mut import_roots = BTreeMap::new();
    for package in &packages {
        let mut roots: BTreeSet<String> =
            member_names.iter().map(|n| n.replace('-', "_")).collect();
        roots.extend(
            packages
                .iter()
                .flat_map(|p| p.targets.iter())
                .filter(|t| {
                    t.kind.iter().any(|k| {
                        matches!(
                            k,
                            TargetKind::Lib | TargetKind::RLib | TargetKind::ProcMacro
                        )
                    })
                })
                .map(|t| t.name.replace('-', "_")),
        );
        roots.extend(
            package
                .dependencies
                .iter()
                .filter(|d| member_names.contains(&d.name))
                .map(|d| d.rename.as_deref().unwrap_or(&d.name).replace('-', "_")),
        );
        import_roots.insert(package.name.to_string(), roots.into_iter().collect());
    }
    let mut files = Vec::new();
    let mut source_mounts = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for package in packages {
        let Some(dir) = package.manifest_path.as_std_path().parent() else {
            return Err("manifest has no parent".into());
        };
        let mut paths = BTreeSet::new();
        let src = dir.join("src");
        if src.is_dir() {
            collect_rs_files(&src, &mut paths, &root, &mut diagnostics)?;
        }
        let targets: Vec<_> = package
            .targets
            .iter()
            .map(|target| (target_key(&root, target.src_path.as_std_path()), target))
            .collect();
        let mut target_counts = BTreeMap::<&str, usize>::new();
        for (path, _) in &targets {
            *target_counts.entry(path).or_default() += 1;
        }
        let entry_points: Vec<_> = targets
            .iter()
            .map(|(path, target)| {
                (
                    path.clone(),
                    target
                        .kind
                        .iter()
                        .any(|kind| matches!(kind, TargetKind::Test | TargetKind::Bench)),
                    (target_counts[path.as_str()] > 1).then(|| target_label(target)),
                )
            })
            .collect();
        for target in &package.targets {
            let normalized = normalize_path(target.src_path.as_std_path());
            let path = normalized.as_path();
            if !path.starts_with(&src) && path.extension().is_some_and(|e| e == "rs") {
                diagnostics.push(Diagnostic {
                    package: Some(package.name.to_string()),
                    file: rel_display(&root, path),
                    line: None,
                    message: "target root outside src included; sibling modules outside src are not recursively discovered".into(),
                });
                paths.insert(path.to_path_buf());
            }
        }
        let mut loaded = Vec::new();
        for path in paths {
            let rel = rel_display(&root, &path);
            if !path.starts_with(&root) || has_symlink(&root, &path)? {
                diagnostics.push(Diagnostic {
                    package: None,
                    file: rel,
                    line: None,
                    message: "source mount outside workspace or through a symlink excluded".into(),
                });
                continue;
            }
            let source = if let Some(source) = captured.get(&rel) {
                Arc::clone(source)
            } else {
                let source = Arc::new(crate::source::Source::of(
                    Path::new(&rel),
                    fs::read_to_string(&path)?,
                ));
                captured.insert(rel.clone(), Arc::clone(&source));
                source
            };
            loaded.push(Loaded {
                rel,
                source,
                test_file: false,
            });
        }
        let outside = mounted::outside_mounts(
            &root,
            &rel_display(&root, &src),
            &loaded,
            captured,
            &mut diagnostics,
        )?;
        let mut known: BTreeSet<_> = loaded.iter().map(|file| file.rel.clone()).collect();
        for issue in &outside.unavailable {
            diagnostics.push(Diagnostic {
                file: issue.declaring_file.clone(),
                line: None,
                package: Some(package.name.to_string()),
                message: format!(
                    "required source-tree mount {} could not be captured: {}",
                    issue.module,
                    issue.candidates.join(", ")
                ),
            });
        }
        for mount in &outside.files {
            if known.insert(mount.path.clone()) {
                loaded.push(Loaded {
                    rel: mount.path.clone(),
                    source: Arc::clone(&captured[&mount.path]),
                    test_file: true,
                });
            }
        }
        source_mounts.insert(package.name.to_string(), outside);
        let mounts = mark_test_files(&mut loaded, &entry_points, &package.name, &mut diagnostics);
        for (file_index, file) in loaded.into_iter().enumerate() {
            let source = &file.source;
            for import in source.imports().filter(|import| {
                crate::population::TestSelection::Production.admits_line(source, import.line)
            }) {
                if let Err(error) = &import.bindings {
                    diagnostics.push(Diagnostic {
                        file: file.rel.clone(),
                        line: Some(import.line),
                        package: Some(package.name.to_string()),
                        message: format!("use syntax could not be read: {error}"),
                    });
                }
            }
            let imports = source
                .outline()
                .iter()
                .filter(|item| {
                    matches!(
                        item.kind,
                        crate::scan::ItemKind::Use | crate::scan::ItemKind::PubUse
                    ) && crate::population::TestSelection::Production.admits_line(source, item.line)
                })
                .map(|item| Arc::clone(&item.declaration_block))
                .collect();
            let declarations = source
                .outline()
                .iter()
                .filter(|item| item.kind.declares())
                .map(|item| Declaration {
                    id: format!("{}:{}:{}", file.rel, item.line, item.name),
                    name: item.name.clone(),
                    kind: item.kind.tag().to_owned(),
                    visibility: item.visibility.tag().to_owned(),
                    line: item.line,
                    end_line: *item.span.end(),
                    summary: item.doc.clone(),
                    test_only: file.test_file
                        || !crate::population::TestSelection::Production
                            .admits_line(source, item.line),
                    block: Arc::clone(&item.declaration_block),
                })
                .collect();
            files.push(SourceFile {
                path: file.rel,
                crate_name: package.name.to_string(),
                content_fingerprint: fingerprint(source.text().as_bytes()),
                test_only: file.test_file,
                mounts: mounts.get(&file_index).cloned().unwrap_or_default(),
                imports,
                declarations,
            });
        }
    }
    files.sort_by(|a, b| (&a.path, &a.crate_name).cmp(&(&b.path, &b.crate_name)));
    let mut shared = BTreeMap::<String, usize>::new();
    for file in &files {
        *shared.entry(file.path.clone()).or_default() += 1;
    }
    for file in &mut files {
        if shared[&file.path] > 1 {
            for declaration in &mut file.declarations {
                declaration.id = format!("{}@{}", declaration.id, file.crate_name);
            }
        }
    }
    diagnostics.sort_by(|a, b| {
        (&a.file, a.line, &a.package, &a.message).cmp(&(&b.file, b.line, &b.package, &b.message))
    });
    diagnostics.dedup_by(|a, b| {
        a.file == b.file && a.line == b.line && a.package == b.package && a.message == b.message
    });
    let source_diagnostics = diagnostics.clone();
    let mut graph = SourceGraph {
        schema: 1,
        root: ".".into(),
        files,
        import_roots,
        relationships: Vec::new(),
        diagnostics,
    };
    crate::relationships::derive_captured(&mut graph, captured);
    Ok((graph, source_mounts, source_diagnostics))
}

fn has_symlink(root: &Path, path: &Path) -> Result<bool> {
    let mut current = root.to_path_buf();
    for component in path.strip_prefix(root)?.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}

fn collect_rs_files(
    dir: &Path,
    paths: &mut BTreeSet<PathBuf>,
    root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            diagnostics.push(Diagnostic {
                package: None,
                file: rel_display(root, &path),
                line: None,
                message: "symlink excluded from physical source walk".into(),
            });
        } else if ty.is_dir() {
            collect_rs_files(&path, paths, root, diagnostics)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("rs"))
        {
            paths.insert(path);
        }
    }
    Ok(())
}
pub(crate) fn target_label(target: &cargo_metadata::Target) -> String {
    format!(
        "{}:{}",
        target
            .kind
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("+"),
        target.name
    )
}

/// One lexical identity for Cargo roots in discovery and qualified namespaces.
pub(crate) fn target_key(root: &Path, target: &Path) -> String {
    rel_display(&normalize_path(root), &normalize_path(target))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some_and(|name| name != "..") {
                    normalized.pop();
                } else if !normalized.has_root() {
                    normalized.push("..");
                }
            }
            part => normalized.push(part.as_os_str()),
        }
    }
    normalized
}

fn rel_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn child_directory(rel: &str) -> String {
    let (directory, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    match file {
        "lib.rs" | "main.rs" | "mod.rs" => directory.to_owned(),
        _ => Path::new(directory)
            .join(file.trim_end_matches(".rs"))
            .to_string_lossy()
            .replace('\\', "/"),
    }
}

fn resolve_module_path(declaring_file: &str, explicit_path: &str) -> String {
    let directory = Path::new(declaring_file)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    normalize_path(&directory.join(explicit_path))
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ModuleMount {
    file: usize,
    beside_file: bool,
}

/// Follow test-gated file declarations and their transitive children. A path
/// mount changes the child's location, never its inherited test boundary.
/// Production mounts take precedence when a file has both kinds of parent.
fn mark_test_files(
    files: &mut [Loaded],
    entry_points: &[(String, bool, Option<String>)],
    package: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<usize, Vec<crate::model::Mount>> {
    let by_path: BTreeMap<_, _> = files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.rel.clone(), index))
        .collect();
    let mut children = BTreeMap::new();
    let mut mounted = BTreeSet::new();
    for (index, file) in files.iter().enumerate() {
        let declarations = file.source.file_module_declarations();
        for item in file.source.outline().iter().filter(|item| {
            item.kind == crate::scan::ItemKind::Mod
                && !item.top_level
                && item.declaration_block.trim_end().ends_with(';')
        }) {
            diagnostics.push(Diagnostic {
                package: Some(package.to_owned()),
                file: file.rel.clone(),
                line: Some(item.line),
                message: "file module inside an inline module is not mounted by the rustfmt-shaped file-module reader".into(),
            });
        }
        for (name, explicit, _) in declarations {
            let candidates = explicit.as_ref().map_or_else(
                || {
                    let parent = file.rel.rsplit_once('/').map_or("", |(parent, _)| parent);
                    [parent.to_owned(), child_directory(&file.rel)]
                        .into_iter()
                        .flat_map(|directory| mounted::module_candidates(&directory, name))
                        .collect::<Vec<_>>()
                },
                |path| vec![resolve_module_path(&file.rel, path)],
            );
            if !candidates
                .iter()
                .any(|candidate| by_path.contains_key(candidate))
            {
                diagnostics.push(Diagnostic {
                    package: Some(package.to_owned()),
                    file: file.rel.clone(),
                    line: None,
                    message: format!(
                        "module {name} has no discovered file mount (missing, outside the inventory, or excluded): {}",
                        candidates.join(", ")
                    ),
                });
            }
        }
        for beside_file in [false, true] {
            let mount = ModuleMount {
                file: index,
                beside_file,
            };
            let directory = if beside_file {
                file.rel
                    .rsplit_once('/')
                    .map_or("", |(directory, _)| directory)
                    .to_owned()
            } else {
                child_directory(&file.rel)
            };
            let mut edges = Vec::new();
            for (name, explicit_path, test_only) in declarations {
                let paths = if let Some(path) = explicit_path {
                    vec![resolve_module_path(&file.rel, path)]
                } else {
                    mounted::module_candidates(&directory, name).to_vec()
                };
                for path in paths {
                    if let Some(&child) = by_path.get(&path) {
                        mounted.insert(child);
                        edges.push((
                            ModuleMount {
                                file: child,
                                beside_file: explicit_path.is_some(),
                            },
                            *test_only,
                            name.clone(),
                        ));
                    }
                }
            }
            children.insert(mount, edges);
        }
    }
    // Unmounted files remain production census inputs. Cargo supplies the
    // actual entry points, including roots also imported by another module.
    let mut pending: Vec<_> = (0..files.len())
        .filter(|index| {
            !mounted.contains(index)
                && !files[*index].test_file
                && !entry_points
                    .iter()
                    .any(|(path, _, _)| files[*index].rel == *path)
        })
        .map(|file| {
            (
                ModuleMount {
                    file,
                    beside_file: false,
                },
                false,
            )
        })
        .chain(
            entry_points
                .iter()
                .filter_map(|(path, tests, _)| {
                    by_path.get(path).copied().map(|file| (file, *tests))
                })
                .map(|(file, tests)| {
                    (
                        ModuleMount {
                            file,
                            beside_file: true,
                        },
                        tests,
                    )
                }),
        )
        .collect();
    let mut reached = BTreeSet::new();
    let mut tests = BTreeSet::new();
    let mut production = BTreeSet::new();
    while let Some((mount, in_tests)) = pending.pop() {
        if reached.insert((mount, in_tests)) {
            if in_tests {
                tests.insert(mount.file);
            } else {
                production.insert(mount.file);
            }
            pending.extend(
                children[&mount]
                    .iter()
                    .map(|(child, gated, _)| (*child, in_tests || *gated)),
            );
        }
    }
    for (index, file) in files.iter_mut().enumerate() {
        file.test_file = (file.test_file || tests.contains(&index)) && !production.contains(&index);
    }
    let mut paths = BTreeMap::<usize, BTreeSet<crate::model::Mount>>::new();
    for (root, root_test, target) in entry_points {
        let Some(&file) = by_path.get(root) else {
            continue;
        };
        let mut pending = vec![(
            ModuleMount {
                file,
                beside_file: true,
            },
            Vec::<String>::new(),
            *root_test,
            BTreeSet::new(),
        )];
        while let Some((mount, modules, in_tests, mut ancestry)) = pending.pop() {
            if !ancestry.insert(mount) {
                continue;
            }
            paths
                .entry(mount.file)
                .or_default()
                .insert(crate::model::Mount {
                    root: root.clone(),
                    modules: modules.clone(),
                    test_only: in_tests,
                    target: target.clone(),
                });
            for (child, gated, name) in &children[&mount] {
                let mut namespace = modules.clone();
                namespace.push(name.clone());
                pending.push((*child, namespace, in_tests || *gated, ancestry.clone()));
            }
        }
    }
    paths
        .into_iter()
        .map(|(index, paths)| (index, paths.into_iter().collect()))
        .collect()
}

pub fn fingerprint(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64:{value:016x}")
}

/// A scope that is absent differs from a scope with multiple possible meanings.
#[derive(Debug)]
pub enum ScopeError {
    Absent {
        scope: String,
    },
    Ambiguous {
        scope: String,
        candidates: Vec<String>,
    },
}
impl std::fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent { scope } => write!(formatter, "scope {scope:?} matches no source path"),
            Self::Ambiguous { scope, candidates } => write!(
                formatter,
                "scope {scope:?} is ambiguous: {}",
                candidates.join(", ")
            ),
        }
    }
}
impl std::error::Error for ScopeError {}

/// Resolve an exact physical path or an unambiguous file/directory basename.
pub fn resolve_scope(graph: &SourceGraph, scope: Option<&str>) -> Result<String> {
    let Some(scope) = scope.filter(|s| !s.is_empty() && *s != ".") else {
        return Ok(String::new());
    };
    let scope = scope.trim_start_matches("./").trim_end_matches('/');
    if scope.is_empty() {
        return Ok(String::new());
    }
    if graph
        .files
        .iter()
        .any(|f| crate::architecture::contains(scope, &f.path))
    {
        return Ok(scope.trim_end_matches('/').into());
    }
    let mut candidates = BTreeSet::new();
    for file in &graph.files {
        let path = Path::new(&file.path);
        if path.file_stem().is_some_and(|s| s == scope) {
            candidates.insert(file.path.clone());
        }
        let mut current = path.parent();
        while let Some(parent) = current {
            if parent.file_name().is_some_and(|s| s == scope) {
                candidates.insert(parent.to_string_lossy().replace('\\', "/"));
            }
            current = parent.parent();
        }
    }
    let mut candidates = candidates.into_iter();
    match (candidates.next(), candidates.next()) {
        (Some(only), None) => Ok(only),
        (None, _) => Err(ScopeError::Absent {
            scope: scope.into(),
        }
        .into()),
        (Some(first), Some(second)) => Err(ScopeError::Ambiguous {
            scope: scope.into(),
            candidates: [first, second].into_iter().chain(candidates).collect(),
        }
        .into()),
    }
}

#[cfg(test)]
pub(crate) fn classify_test_fixture(files: Vec<(String, Arc<crate::source::Source>)>) -> Vec<bool> {
    let mut loaded: Vec<_> = files
        .into_iter()
        .map(|(rel, source)| Loaded {
            rel,
            source,
            test_file: false,
        })
        .collect();
    mark_test_files(&mut loaded, &[], "fixture", &mut Vec::new());
    loaded.into_iter().map(|file| file.test_file).collect()
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn cargo_target_parent_components_have_one_physical_identity() {
        let root = Path::new("/workspace");
        let target = Path::new("/workspace/xtask/../scripts/population.rs");
        assert_eq!(
            rel_display(root, &normalize_path(target)),
            "scripts/population.rs"
        );
    }
}
