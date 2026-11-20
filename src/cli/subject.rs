//! The bare arguments (spec §3.2): which one is the project root and which the
//! command's subject, and what a scope subject names. This is the only place
//! that reads a bare argument. A bare argument is read as one kind, never two,
//! and no candidate is picked: an ambiguous scope is refused with its candidates.
//!
//! A positional ROOT is still read, for compatibility, when it can only mean a
//! project: the first of two bare words, or one bare word naming a Cargo
//! project outside the workspace Atlas would otherwise read.
use super::output::notice::Notices;
use cargo_atlas::{Result, model::SourceGraph};
use clap::Args;
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// The bare words of a command whose subject is a scope, and its scope flags.
#[derive(Args)]
pub(crate) struct Scope {
    /// A path, a unique file or directory name, or a package; defaults to the
    /// whole workspace.
    #[arg(value_name = "SCOPE")]
    first: Option<String>,
    #[arg(hide = true)]
    second: Option<String>,
    /// A path or a unique file or directory name, never a package.
    #[arg(long = "in", value_name = "PATH", conflicts_with_all = ["package", "scope"])]
    in_path: Option<String>,
    /// One Cargo package of the workspace.
    #[arg(short = 'p', long, value_name = "PACKAGE", conflicts_with = "scope")]
    package: Option<String>,
    /// Superseded by --in (or -p for a package); read as in 0.18.
    #[arg(long = "scope", id = "scope", value_name = "SCOPE")]
    written: Option<String>,
    #[arg(skip)]
    subject: Option<String>,
}
/// The bare words of `name` and `module`: a NAME, after an optional project
/// directory.
#[derive(Args)]
pub(crate) struct Name {
    #[arg(value_name = "NAME")]
    first: String,
    #[arg(hide = true)]
    second: Option<String>,
    #[arg(skip)]
    subject: String,
}
/// The bare words of `types`: the types, after an optional project directory.
#[derive(Args)]
pub(crate) struct Types {
    /// Type names, comma separated (as --type takes them).
    #[arg(value_name = "TYPES")]
    first: Option<String>,
    #[arg(hide = true)]
    second: Option<String>,
    #[arg(skip)]
    subject: Option<String>,
}
/// The bare words of a command with no subject: only a compatibility
/// positional ROOT, and for `flow` and `map` the 0.18 positional ANCHOR or
/// FOCUS after it.
#[derive(Args)]
pub(crate) struct Root {
    #[arg(hide = true)]
    first: Option<String>,
    #[arg(hide = true)]
    second: Option<String>,
    #[arg(skip)]
    subject: Option<String>,
}

/// The revision a review command compares against: the bare REVISION, or
/// `--vs REV`, an equal spelling (spec §3.4). Neither is superseded.
#[derive(Args)]
pub(crate) struct Revision {
    /// The revision to compare against: anything Git or jj resolves, such as
    /// HEAD~1 or @-. --vs REV is the same.
    #[arg(value_name = "REVISION", required_unless_present = "vs")]
    bare: Option<String>,
    /// The revision to compare against, as a flag.
    #[arg(long, value_name = "REV")]
    vs: Option<String>,
}
impl Revision {
    /// Refuse two different revisions. They are compared as written: two
    /// spellings of one commit are refused too, and say so.
    pub(crate) fn read(&self) -> Result<()> {
        match (&self.bare, &self.vs) {
            (Some(bare), Some(vs)) if bare != vs => Err(format!(
                "give the revision once: REV or --vs REV (got {bare} and --vs {vs})"
            )
            .into()),
            _ => Ok(()),
        }
    }
    pub(crate) fn reference(&self) -> &str {
        self.bare
            .as_deref()
            .or(self.vs.as_deref())
            .unwrap_or_default()
    }
}

/// Where the calling (or called) functions of `callers` and `callees` are:
/// `--in` or `-p`, never which function is asked about.
#[derive(Args)]
pub(crate) struct Within {
    /// Keep the functions in this path or unique file or directory name.
    #[arg(long = "in", value_name = "PATH", conflicts_with = "package")]
    in_path: Option<String>,
    /// Keep the functions in this Cargo package.
    #[arg(short = 'p', long, value_name = "PACKAGE")]
    package: Option<String>,
}
impl Within {
    /// Whether a function in `file` is kept.
    pub(crate) fn filter(
        &self,
        graph: &SourceGraph,
        metadata: &cargo_metadata::Metadata,
    ) -> Result<impl Fn(&str) -> bool> {
        let chosen = match (&self.in_path, &self.package) {
            (Some(path), _) => match path_or_basename(graph, metadata, path)? {
                Some(path) => Chosen::Path(path),
                None => {
                    return Err(format!(
                        "no path or file name {path:?} in this project; cargo atlas orient lists them"
                    )
                    .into());
                }
            },
            (None, Some(package)) => package_named(metadata, package)?,
            (None, None) => Chosen::Workspace,
        };
        let packages: std::collections::BTreeSet<String> = match &chosen {
            Chosen::Package(name) => graph
                .files
                .iter()
                .filter(|file| file.crate_name == *name)
                .map(|file| file.path.clone())
                .collect(),
            _ => std::collections::BTreeSet::new(),
        };
        Ok(move |file: &str| match &chosen {
            Chosen::Workspace => true,
            Chosen::Path(path) | Chosen::Written(path) => {
                cargo_atlas::architecture::contains(path, file)
            }
            Chosen::Package(_) => packages.contains(file),
        })
    }
}
/// The function a call question is about: the bare FN, or flow's spelling
/// `--path FILE --function NAME`.
#[derive(Args)]
pub(crate) struct Function {
    /// The function: FILE:LINE, a declaration id, Type::method, module::name
    /// or a name (every function with that name).
    #[arg(
        value_name = "FN",
        required_unless_present = "path",
        conflicts_with = "path"
    )]
    bare: Option<String>,
    /// The file that holds the function; needs --function.
    #[arg(long, value_name = "FILE", requires = "function")]
    path: Option<String>,
    /// The function's name in --path.
    #[arg(
        long = "function",
        id = "function",
        value_name = "NAME",
        requires = "path"
    )]
    named: Option<String>,
}
impl Function {
    /// The selector, and the file it is restricted to.
    pub(crate) fn selector(&self) -> (&str, Option<&str>) {
        match (&self.bare, &self.named) {
            (Some(bare), _) => (bare, None),
            (None, Some(name)) => (name, self.path.as_deref()),
            (None, None) => ("", None),
        }
    }
}

/// What the bare words may hold besides a project root.
#[derive(Clone, Copy)]
enum Kind {
    Scope,
    Name,
    /// `flagged`: the types were also chosen with --type or --query.
    Types {
        flagged: bool,
    },
    None,
    /// `flow`'s 0.18 positional ANCHOR, read only beside --architecture.
    Anchor {
        architecture: bool,
    },
    /// `map`'s 0.18 positional FOCUS.
    Focus,
}
/// Where a command reads a project from, for deciding whether a bare word
/// names another one.
pub(crate) struct Reader<'a> {
    pub command: &'static str,
    pub named_root: Option<&'a Path>,
    pub notices: &'a mut Notices,
}
impl Scope {
    pub(crate) fn read(&mut self, reader: &mut Reader<'_>) -> Result<Option<PathBuf>> {
        let (root, subject) = split(
            reader,
            Kind::Scope,
            self.first.as_deref(),
            self.second.as_deref(),
        )?;
        if subject.is_some() && (self.in_path.is_some() || self.package.is_some()) {
            return Err("give the scope once: a bare argument or --in/-p".into());
        }
        if subject.is_some() && self.written.is_some() {
            return Err("give the scope once: a bare argument or --scope".into());
        }
        self.subject = subject;
        Ok(root)
    }
    /// What the scope names in `graph`, read in the order of spec R1.
    pub(crate) fn choose(
        &self,
        graph: &SourceGraph,
        metadata: &cargo_metadata::Metadata,
    ) -> Result<Chosen> {
        if let Some(written) = &self.written {
            return Ok(Chosen::Written(written.clone()));
        }
        if let Some(package) = &self.package {
            return package_named(metadata, package);
        }
        if let Some(path) = &self.in_path {
            return match path_or_basename(graph, metadata, path)? {
                Some(path) => Ok(Chosen::Path(path)),
                None => Err(format!(
                    "no path or file name {path:?} in this project; cargo atlas orient lists them"
                )
                .into()),
            };
        }
        let Some(word) = self.subject.as_deref() else {
            return Ok(Chosen::Workspace);
        };
        let package = metadata
            .workspace_packages()
            .into_iter()
            .find(|package| package.name.as_str() == word)
            .map(|package| package_dir(metadata, package));
        match path_or_basename(graph, metadata, word) {
            Ok(Some(path)) if path.is_empty() => Ok(Chosen::Workspace),
            Ok(Some(path)) => match package {
                Some(dir) if dir != path => Err(format!(
                    "{word:?} is both the path {path} and the package {word} (at {}); use --in {path} or -p {word}",
                    if dir.is_empty() { "." } else { &dir }
                )
                .into()),
                _ => Ok(Chosen::Path(path)),
            },
            Ok(None) if package.is_some() => Ok(Chosen::Package(word.to_owned())),
            Ok(None) => Err(format!(
                "no path, file name or package {word:?} in this project; cargo atlas orient lists them"
            )
            .into()),
            Err(error) => Err(error),
        }
    }
}
impl Name {
    pub(crate) fn read(&mut self, reader: &mut Reader<'_>) -> Result<Option<PathBuf>> {
        let (root, subject) = split(
            reader,
            Kind::Name,
            Some(&self.first),
            self.second.as_deref(),
        )?;
        self.subject = subject.unwrap_or_default();
        Ok(root)
    }
    pub(crate) fn subject(&self) -> &str {
        &self.subject
    }
}
impl Types {
    /// `flagged`: the command also chose types with --type or --query.
    pub(crate) fn read(
        &mut self,
        reader: &mut Reader<'_>,
        flagged: bool,
    ) -> Result<Option<PathBuf>> {
        let (root, subject) = split(
            reader,
            Kind::Types { flagged },
            self.first.as_deref(),
            self.second.as_deref(),
        )?;
        self.subject = subject;
        Ok(root)
    }
    /// The type names given as the bare argument.
    pub(crate) fn names(&self) -> Vec<String> {
        self.subject
            .iter()
            .flat_map(|names| names.split(','))
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect()
    }
}
impl Root {
    pub(crate) fn read(&mut self, reader: &mut Reader<'_>) -> Result<Option<PathBuf>> {
        self.read_as(reader, Kind::None)
    }
    /// `flow`: a bare word that is not a project is the 0.18 positional ANCHOR,
    /// which needs --architecture.
    pub(crate) fn read_anchor(
        &mut self,
        reader: &mut Reader<'_>,
        architecture: bool,
    ) -> Result<Option<PathBuf>> {
        self.read_as(reader, Kind::Anchor { architecture })
    }
    /// `map`: a bare word that is not a project is its focus, as in 0.18.
    pub(crate) fn read_focus(&mut self, reader: &mut Reader<'_>) -> Result<Option<PathBuf>> {
        self.read_as(reader, Kind::Focus)
    }
    fn read_as(&mut self, reader: &mut Reader<'_>, kind: Kind) -> Result<Option<PathBuf>> {
        let (root, subject) = split(reader, kind, self.first.as_deref(), self.second.as_deref())?;
        self.subject = subject;
        Ok(root)
    }
    /// The anchor or focus written after the root, if any.
    pub(crate) fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }
}

/// The bare words read as (project root, subject), by spec R5–R7.
fn split(
    reader: &mut Reader<'_>,
    kind: Kind,
    first: Option<&str>,
    second: Option<&str>,
) -> Result<(Option<PathBuf>, Option<String>)> {
    let command = reader.command;
    let Some(first) = first else {
        return Ok((None, None));
    };
    if let Some(second) = second {
        // R5: of two bare words, the first can only be the project.
        if !Path::new(first).is_dir() {
            return Err(format!(
                "{first:?} is not a directory, so it cannot be the project root; {command} takes one subject"
            )
            .into());
        }
        reader.notices.superseded("positional ROOT", "--root");
        if matches!(kind, Kind::Types { flagged: true }) {
            return Err("give the types once: a bare TYPES or --type/--query".into());
        }
        if matches!(kind, Kind::None) {
            return Err(format!(
                "{command} takes no subject, only the project directory: drop {second:?}"
            )
            .into());
        }
        if matches!(kind, Kind::Anchor { .. }) {
            reader.notices.superseded("positional ANCHOR", "--anchor");
        }
        return Ok((Some(PathBuf::from(first)), Some(second.to_owned())));
    }
    let subject = Ok((None, Some(first.to_owned())));
    let root = |reader: &mut Reader<'_>| {
        reader.notices.superseded("positional ROOT", "--root");
        Ok((Some(PathBuf::from(first)), None))
    };
    match kind {
        // R7: one bare word is the NAME, or the types.
        Kind::Name | Kind::Types { flagged: false } => return subject,
        // `types ROOT --type X`, the 0.18 spelling.
        Kind::Types { flagged: true } if Path::new(first).is_dir() => return root(reader),
        Kind::Types { flagged: true } => {
            return Err("give the types once: a bare TYPES or --type/--query".into());
        }
        Kind::Scope | Kind::None | Kind::Anchor { .. } | Kind::Focus => {}
    }
    // R6: a project directory outside the workspace is the root.
    match (names_a_project(first, reader.named_root), kind) {
        // `census .` is the whole workspace, as before.
        (Project::Same, Kind::Scope) => Ok((None, None)),
        (Project::Other | Project::Same, _) => root(reader),
        (Project::No, Kind::Anchor { architecture: true }) => {
            reader.notices.superseded("positional ANCHOR", "--anchor");
            subject
        }
        (Project::No, Kind::Anchor { architecture: false }) => Err(format!(
            "flow takes no bare subject without --architecture; give the body with --path FILE --function NAME, or read another project with --root {first}"
        )
        .into()),
        (Project::No, Kind::None) => Err(format!(
            "{command} takes no subject; to read another project use --root {}",
            if Path::new(first).join("Cargo.toml").is_file() { first } else { "PATH" }
        )
        .into()),
        (Project::No, _) => subject,
    }
}
#[derive(PartialEq, Eq)]
enum Project {
    /// Not a project directory, or one inside the workspace being read.
    No,
    /// The workspace being read, named again.
    Same,
    /// Another project.
    Other,
}
/// Spec R6: `word` is a project root only when it is a directory with a
/// `Cargo.toml` outside the workspace read from `named_root` (or the current
/// directory). The workspace root itself also reads as the root: naming it
/// changes nothing, and refusing it would break `dump .`.
fn names_a_project(word: &str, named_root: Option<&Path>) -> Project {
    let path = Path::new(word);
    if !path.join("Cargo.toml").is_file() {
        return Project::No;
    }
    let Ok(path) = path.canonicalize() else {
        return Project::No;
    };
    let Some(workspace) = workspace_root(named_root.unwrap_or(Path::new("."))) else {
        return Project::Other;
    };
    if path == workspace {
        Project::Same
    } else if path.starts_with(&workspace) {
        Project::No
    } else {
        Project::Other
    }
}
/// The canonical root of the Cargo workspace that `dir` belongs to.
fn workspace_root(dir: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new(std::env::var_os("CARGO").unwrap_or("cargo".into()))
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let manifest = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    manifest.parent()?.canonicalize().ok()
}

/// What a scope names.
pub(crate) enum Chosen {
    Workspace,
    /// A discovered path, relative to the workspace root.
    Path(String),
    /// A workspace package, by name.
    Package(String),
    /// A `--scope` value, passed on as 0.18 read it.
    Written(String),
}
impl Chosen {
    /// For queries that read a package name or a path, as `census`, `orient`
    /// and `module` do.
    pub(crate) fn query(&self) -> Option<&str> {
        match self {
            Self::Workspace => None,
            Self::Path(text) | Self::Package(text) | Self::Written(text) => Some(text),
        }
    }
    /// For queries that read a path. A package reads as its directory when no
    /// other package's files are inside it; otherwise it is refused, since a
    /// path would read more than the package.
    pub(crate) fn path(
        &self,
        metadata: &cargo_metadata::Metadata,
        command: &str,
    ) -> Result<String> {
        match self {
            Self::Workspace => Ok(".".into()),
            Self::Path(text) | Self::Written(text) => Ok(text.clone()),
            Self::Package(name) => {
                let packages = metadata.workspace_packages();
                let dir = packages
                    .iter()
                    .find(|package| package.name.as_str() == name)
                    .map(|package| package_dir(metadata, package))
                    .ok_or_else(|| format!("no package {name:?} in this workspace"))?;
                let nested = packages.iter().find(|other| {
                    other.name.as_str() != name && inside(&dir, &package_dir(metadata, other))
                });
                match nested {
                    Some(other) => Err(format!(
                        "{command} reads paths, and package {name}'s directory {} also holds package {}; give the part you mean with --in PATH",
                        if dir.is_empty() { "." } else { &dir },
                        other.name
                    )
                    .into()),
                    None => Ok(if dir.is_empty() { ".".into() } else { dir }),
                }
            }
        }
    }
}
/// Whether relative directory `inner` is `outer` or below it ("" is the root).
fn inside(outer: &str, inner: &str) -> bool {
    outer.is_empty()
        || inner == outer
        || inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
}
/// The package's directory, relative to the workspace root ("" for the root).
fn package_dir(metadata: &cargo_metadata::Metadata, package: &cargo_metadata::Package) -> String {
    package
        .manifest_path
        .parent()
        .and_then(|dir| dir.strip_prefix(&metadata.workspace_root).ok())
        .map(|dir| dir.as_str().replace('\\', "/"))
        .unwrap_or_default()
}
fn package_named(metadata: &cargo_metadata::Metadata, name: &str) -> Result<Chosen> {
    let packages = metadata.workspace_packages();
    if packages.iter().any(|package| package.name.as_str() == name) {
        return Ok(Chosen::Package(name.to_owned()));
    }
    let mut names: Vec<&str> = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    names.sort_unstable();
    Err(format!(
        "no package {name:?} in this workspace; its packages: {}",
        listed(&names)
    )
    .into())
}
/// `word` as a discovered path (R1.1) or a unique file or directory name
/// (R1.2): `None` when it is neither, a refusal when the name is not unique.
/// An absolute path or one through `..` that lands in the workspace reads as
/// the same path relative to the workspace root.
fn path_or_basename(
    graph: &SourceGraph,
    metadata: &cargo_metadata::Metadata,
    word: &str,
) -> Result<Option<String>> {
    let relative = Path::new(word)
        .canonicalize()
        .ok()
        .filter(|_| Path::new(word).is_absolute() || word.starts_with(".."))
        .and_then(|path| {
            path.strip_prefix(metadata.workspace_root.as_std_path())
                .ok()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        });
    let word = relative.as_deref().unwrap_or(word);
    match cargo_atlas::discovery::resolve_scope(graph, Some(word)) {
        Ok(path) => Ok(Some(path)),
        Err(error) => match error.downcast::<cargo_atlas::discovery::ScopeError>() {
            Ok(scope) => match *scope {
                // `resolve_scope` matches directory names and file stems
                // (`--scope` in 0.18); R1.2 also reads a file's whole name.
                cargo_atlas::discovery::ScopeError::Absent { .. } => {
                    let mut files: Vec<String> = graph
                        .files
                        .iter()
                        .filter(|file| {
                            Path::new(&file.path)
                                .file_name()
                                .is_some_and(|name| name == word)
                        })
                        .map(|file| file.path.clone())
                        .collect();
                    files.sort_unstable();
                    files.dedup();
                    match files.len() {
                        0 => Ok(None),
                        1 => Ok(files.pop()),
                        _ => Err(Box::new(Ambiguous {
                            word: word.to_owned(),
                            candidates: files,
                        })),
                    }
                }
                cargo_atlas::discovery::ScopeError::Ambiguous { candidates, .. } => {
                    Err(Box::new(Ambiguous {
                        word: word.to_owned(),
                        candidates,
                    }))
                }
                _ => Err("scope could not be read".into()),
            },
            Err(error) => Err(error),
        },
    }
}
/// A scope name that matches more than one path (R3).
#[derive(Debug)]
pub(crate) struct Ambiguous {
    pub word: String,
    pub candidates: Vec<String>,
}
impl std::fmt::Display for Ambiguous {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.candidates.iter().map(String::as_str).collect();
        write!(
            formatter,
            "{:?} matches {} paths: {}; give one with --in PATH",
            self.word,
            self.candidates.len(),
            listed(&names)
        )
    }
}
impl std::error::Error for Ambiguous {}
/// Up to 8 names, then how many more.
fn listed(names: &[&str]) -> String {
    const SHOWN: usize = 8;
    let mut text = names[..names.len().min(SHOWN)].join(", ");
    if names.len() > SHOWN {
        let _ = write!(text, ", … ({} more)", names.len() - SHOWN);
    }
    text
}
