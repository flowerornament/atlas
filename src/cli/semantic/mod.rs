//! Semantic call evidence from a SCIP index: one the caller supplies with
//! `--scip <FILE>`, where Atlas runs no indexer and no project build code, or
//! one `--semantic` produces (see `indexer`). The reading is added beside the
//! command's own evidence; a supplied index that cannot be read or used is
//! reported unavailable, never a failed command.
mod indexer;

use atlas_lib::analysis::semantic::{Reading, SemanticIndex};
use cargo_atlas::Result;
pub(super) use indexer::{IndexCache, IndexerArgs, Indexing};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The index a command was asked to read: `--scip FILE`, `--semantic`, or
/// none. `sources` are the captured files `--semantic` digests.
pub(super) fn obtain(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    scip: Option<&Path>,
    indexer: &IndexerArgs,
    cache: &IndexCache,
    captured: impl FnOnce() -> Vec<(PathBuf, String)>,
) -> Result<Option<(SemanticIndex, Option<Indexing>)>> {
    if indexer.semantic {
        let (index, indexing) = indexer::index(root, metadata, &captured(), indexer, cache)?;
        Ok(Some((index, Some(indexing))))
    } else {
        Ok(scip.map(|file| (index(file), None)))
    }
}

/// The supplied index, read from the path as given: an option's path is
/// made absolute from the current directory when it is parsed.
pub(super) fn index(path: &Path) -> SemanticIndex {
    match std::fs::read(path) {
        Ok(bytes) => SemanticIndex::read(&bytes),
        Err(error) => {
            SemanticIndex::unavailable_because(format!("cannot read {}: {error}", path.display()))
        }
    }
}

/// Read the calls of selected bodies, without a qualified comparison.
pub(super) fn read_workflows(
    root: &Path,
    index: &SemanticIndex,
    flows: &[(String, atlas_lib::workflow::Workflow)],
) -> Reading {
    let mut texts = BTreeMap::new();
    for (_, flow) in flows {
        for call in &flow.calls {
            texts
                .entry(call.site.path.clone())
                .or_insert_with(|| std::fs::read_to_string(root.join(&call.site.path)).ok());
        }
    }
    let text = |path: &str| texts.get(path).and_then(|text| text.as_deref());
    index.read_calls(
        flows
            .iter()
            .flat_map(|(name, flow)| flow.calls.iter().map(move |call| (name.as_str(), call))),
        &text,
    )
}

/// A report with its semantic reading appended as the `semantic` field, and
/// with `--semantic`, how the index was obtained as `semantic_indexing`.
#[derive(Serialize)]
pub(super) struct WithSemantic<'a, T> {
    #[serde(flatten)]
    pub(super) report: &'a T,
    pub(super) semantic: &'a Reading,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) semantic_indexing: Option<&'a Indexing>,
}

/// A semantic reading with how its index was obtained, for evidence files.
#[derive(Serialize)]
pub(super) struct Evidence<'a> {
    #[serde(flatten)]
    pub(super) reading: &'a Reading,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) indexing: Option<&'a Indexing>,
}
