//! Native source-tree mounts: separate population, shared captured source bytes.
use super::{
    Loaded, child_directory, has_symlink, normalize_path, rel_display, resolve_module_path,
};
use crate::{Result, model::Diagnostic, source::Source};
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
pub struct FileMount {
    pub path: String,
    pub module: String,
}

/// A required mount that could not supply source; candidates remain explicit.
#[derive(Debug, Clone, Serialize)]
pub struct UnavailableMount {
    pub declaring_file: String,
    pub module: String,
    pub candidates: Vec<String>,
}
/// Native mounted-file population and its unavailable inputs, from one traversal.
#[derive(Debug, Default)]
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
/// then follow their declarations. Each physical source is captured once.
pub(super) fn outside_mounts(
    root: &Path,
    src_rel: &str,
    files: &[Loaded],
    sources: &mut BTreeMap<String, Arc<Source>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<MountedSources> {
    let inside = format!("{src_rel}/");
    let mut pending = Vec::new();
    for file in files.iter().filter(|file| file.rel.starts_with(&inside)) {
        let parent = module_path_of(src_rel, &file.rel);
        for (name, path, _) in file.source.file_module_declarations() {
            if let Some(path) = path {
                let rel = resolve_module_path(&file.rel, path);
                if !rel.starts_with(&inside) {
                    let module = parent
                        .as_ref()
                        .map_or_else(|| name.clone(), |parent| format!("{parent}::{name}"));
                    pending.push((
                        rel,
                        module,
                        file.rel.clone(),
                        BTreeSet::from([file.rel.clone()]),
                    ));
                }
            }
        }
    }
    let mut mounts = BTreeSet::new();
    let mut unavailable = Vec::new();
    while let Some((rel, module, declaring_file, mut ancestry)) = pending.pop() {
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
        }) {
            continue;
        }
        for (name, path, _) in source.file_module_declarations() {
            let children = path.as_ref().map_or_else(
                || module_candidates(&child_directory(&rel), name).to_vec(),
                |path| vec![resolve_module_path(&rel, path)],
            );
            let mut found = false;
            for child in &children {
                if capture_optional(root, child, sources, diagnostics)?.is_some() {
                    found = true;
                    pending.push((
                        child.clone(),
                        format!("{module}::{name}"),
                        rel.clone(),
                        ancestry.clone(),
                    ));
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

pub(super) fn module_candidates(directory: &str, name: &str) -> [String; 2] {
    let root = Path::new(directory);
    [
        root.join(format!("{name}.rs")),
        root.join(name).join("mod.rs"),
    ]
    .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn capture_optional(
    root: &Path,
    rel: &str,
    sources: &mut BTreeMap<String, Arc<Source>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<Arc<Source>>> {
    if let Some(source) = sources.get(rel) {
        return Ok(Some(Arc::clone(source)));
    }
    let path = normalize_path(&root.join(rel));
    if !path.starts_with(root) || has_symlink(root, &path)? {
        diagnostics.push(Diagnostic {
            file: rel.to_owned(),
            line: None,
            package: None,
            message: "source mount outside workspace or through a symlink excluded".into(),
        });
        return Ok(None);
    }
    match fs::read_to_string(&path) {
        Ok(text) => {
            let source = Arc::new(Source::of(Path::new(rel), text));
            sources.insert(rel.to_owned(), Arc::clone(&source));
            Ok(Some(source))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(format!("capture source mount {}: {error}", rel_display(root, &path)).into())
        }
    }
}
