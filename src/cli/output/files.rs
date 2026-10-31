//! One preflight for artifact, companion and evidence writes; no input is an output.
use atlas::Result;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    let mut file = builder.tempfile_in(parent)?;
    file.write_all(bytes)?;
    if let Some(permissions) = permissions {
        file.as_file().set_permissions(permissions)?;
    }
    file.persist(path)?;
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
    let parent = canonical_destination(parent)?;
    match path.components().next_back() {
        Some(std::path::Component::ParentDir) => Ok(parent
            .parent()
            .ok_or("output escapes filesystem root")?
            .to_owned()),
        Some(std::path::Component::CurDir) => Ok(parent),
        Some(std::path::Component::Normal(name)) => Ok(parent.join(name)),
        _ => Err("output needs a filename".into()),
    }
}

/// Validate the complete write set before creating directories or changing bytes.
pub(crate) fn preflight(files: &[PathBuf], protected: &[PathBuf]) -> Result<()> {
    let protected = protected
        .iter()
        .map(|path| absolute(path))
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    let mut destinations = std::collections::BTreeSet::new();
    for file in files {
        let path = absolute(file)?;
        if protected.contains(&path) || !destinations.insert(path) {
            return Err(format!(
                "refusing output collision: destination collides with source, query or companion: {}",
                file.display()
            ).into());
        }
        match fs::metadata(file) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(format!(
                    "output destination is not a regular file: {}",
                    file.display()
                )
                .into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        for parent in file.ancestors().skip(1) {
            match fs::metadata(parent) {
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(
                        format!("output parent is not a directory: {}", parent.display()).into(),
                    );
                }
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    for file in &destinations {
        if file
            .ancestors()
            .skip(1)
            .any(|parent| destinations.contains(parent))
        {
            return Err(
                "output collision: a destination collides with another destination's directory"
                    .into(),
            );
        }
    }
    Ok(())
}
pub(crate) fn publish(files: Vec<(PathBuf, Vec<u8>)>, protected: &[PathBuf]) -> Result<()> {
    preflight(
        &files
            .iter()
            .map(|(file, _)| file.clone())
            .collect::<Vec<_>>(),
        protected,
    )?;
    for (file, bytes) in files {
        write(&file, &bytes)?;
    }
    Ok(())
}
