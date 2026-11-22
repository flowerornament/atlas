//! Read only the explicitly selected architecture declaration file.
use cargo_atlas::{Result, architecture::Architecture};
use std::{fs, path::Path};
pub(super) fn architecture(
    root: &Path,
    path: Option<&Path>,
) -> Result<(Option<String>, Option<Architecture>)> {
    let text = path.map(fs::read_to_string).transpose()?;
    let profile = text
        .as_deref()
        .map(|s| Architecture::parse(s, root))
        .transpose()?;
    Ok((text, profile))
}
