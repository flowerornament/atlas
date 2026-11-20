//! `--scip <FILE>`: read a SCIP index the caller supplies as semantic call
//! evidence. Atlas never runs an indexer here, so no project build code runs.
//! The reading is added beside the command's own evidence; an index that
//! cannot be read or used is reported unavailable, never a failed command.
use atlas_lib::analysis::semantic::{Reading, SemanticIndex};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

/// The supplied index. A relative path is resolved against the workspace
/// root, like the other input paths.
pub(super) fn index(root: &Path, file: &Path) -> SemanticIndex {
    let path = root.join(file);
    match std::fs::read(&path) {
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

/// A report with its semantic reading appended as the `semantic` field.
#[derive(Serialize)]
pub(super) struct WithSemantic<'a, T> {
    #[serde(flatten)]
    pub(super) report: &'a T,
    pub(super) semantic: &'a Reading,
}
