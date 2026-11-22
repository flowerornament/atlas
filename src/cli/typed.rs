//! A path someone types means what their shell means by it: it is read from
//! the current directory. That holds for a scope word, `--in`, `--path`, the
//! FILE of a selector and every file option (`--out`, `--policy`, `--scip`)
//! alike. A source path is then the workspace key of what it names, and one
//! outside the workspace is refused. A word that names no path from here
//! may still be a name the command looks up (a unique file or directory
//! name, a package).
use cargo_atlas::Result;
use std::path::{Component, Path, PathBuf};

/// A file option's value, made absolute from the current directory: clap's
/// value parser for every option that names a file.
pub(crate) fn file(word: &str) -> std::io::Result<PathBuf> {
    std::path::absolute(word)
}

/// Whether `word` is a path from here: written with a separator, as `.` or
/// `..`, or naming something that exists in the current directory.
pub(crate) fn names_path(word: &str) -> bool {
    word.contains('/') || matches!(word, "." | "..") || Path::new(word).exists()
}

/// `word`, read from the current directory, as a workspace key: relative to
/// the canonical `root`, `""` for the root itself. It need not exist.
pub(crate) fn key(word: &str, root: &Path) -> Result<String> {
    let mut path = PathBuf::new();
    for component in std::env::current_dir()?
        .canonicalize()?
        .join(word)
        .components()
    {
        match component {
            Component::ParentDir => {
                path.pop();
            }
            Component::CurDir => {}
            other => path.push(other),
        }
    }
    let path = path.canonicalize().unwrap_or(path);
    path.strip_prefix(root)
        .map(cargo_atlas::discovery::path_key)
        .map_err(|_| {
            format!(
                "{word} names {}, outside the workspace at {}",
                path.display(),
                root.display()
            )
            .into()
        })
}

/// `word` as a workspace key when it is a path from here; `None` when it is
/// not one, and may be a name.
pub(crate) fn path(word: &str, root: &Path) -> Result<Option<String>> {
    names_path(word).then(|| key(word, root)).transpose()
}

/// A selector with its FILE part (`FILE:LINE`, `FILE:LINE:COLUMN`,
/// `FILE:Name`) as a workspace key when that part is a path from here; any
/// other selector (`Type::method`, a name) as written.
pub(crate) fn selector(selector: &str, root: &Path) -> Result<String> {
    match selector.split_once(':') {
        Some((file, rest)) if !rest.starts_with(':') && names_path(file) => {
            Ok(format!("{}:{rest}", key(file, root)?))
        }
        _ => Ok(selector.to_owned()),
    }
}
