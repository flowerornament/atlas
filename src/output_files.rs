//! Shared filesystem primitives; each command owns its output/evidence policy.
use atlas::Result;
use std::{
    fs,
    path::{Path, PathBuf},
};
pub(crate) fn write(path: &Path, bytes: Vec<u8>) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}
pub(crate) fn absolute(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    canonical_destination(&path)
}
fn canonical_destination(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let parent = path.parent().ok_or("output needs a parent directory")?;
    let name = path.file_name().ok_or("output needs a filename")?;
    Ok(canonical_destination(parent)?.join(name))
}
