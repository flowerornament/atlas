//! A command Atlas prints for someone to run: a refusal's hint, a list's
//! tail, `orient`'s next lines. Every printed command is built here and
//! rendered by one function, so each runs as written.
//!
//! A command runs from where the run was: the program that prints it states
//! once how this run chose its project (`--root PATH`), the workspace root
//! and the current directory. Every command reading a project carries that
//! choice, and every path is written relative to the current directory, as
//! the reader's shell reads it. Before a program states them, a command
//! names no project and writes workspace keys, which is right from the
//! workspace root.
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// The program's name, as every printed command and report title begins.
pub const PROGRAM: &str = "cargo atlas";

/// Where this run's printed commands run from.
struct Run {
    /// The words that name its project, such as `["--root", "../app"]`.
    project: Vec<String>,
    /// The canonical workspace root.
    root: PathBuf,
    /// The canonical current directory.
    here: PathBuf,
}
static RUN: OnceLock<Run> = OnceLock::new();

/// State, once and before anything is printed, how this run chose its
/// project (`project`, the words to repeat), its canonical workspace `root`
/// and the canonical directory it runs in. A later call is ignored: one run
/// reads one project from one place.
pub fn set_run(project: Vec<String>, root: PathBuf, here: PathBuf) {
    let _ = RUN.set(Run {
        project,
        root,
        here,
    });
}

/// A command after `cargo atlas`, as words.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct Command {
    words: Vec<Word>,
    /// Whether the run's project choice is added: not to this run's own
    /// words, which hold it.
    adds_project: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Word {
    Text(String),
    /// A workspace path, by its key: printed as [`shown`] writes it.
    Path(String),
    /// A placeholder (`<bin root>`): the command does not run as printed.
    Placeholder(String),
}
impl Command {
    /// `cargo atlas NAME`, a command that reads the project.
    pub fn new(name: &str) -> Self {
        Self {
            words: vec![Word::Text(name.to_owned())],
            adds_project: true,
        }
    }
    /// This run's own words, edited (`--all` added, an argument respelled):
    /// they already name the project as this run did.
    pub fn rerun(words: &[String]) -> Self {
        Self {
            words: words.iter().cloned().map(Word::Text).collect(),
            adds_project: false,
        }
    }
    /// Add one word, quoted when a shell would split it.
    pub fn arg(mut self, word: impl Into<String>) -> Self {
        self.words.push(Word::Text(word.into()));
        self
    }
    /// Add a workspace path, by its root-relative key.
    pub fn path(mut self, key: impl Into<String>) -> Self {
        self.words.push(Word::Path(key.into()));
        self
    }
    /// Add several words.
    pub fn args<S: Into<String>>(mut self, words: impl IntoIterator<Item = S>) -> Self {
        self.words
            .extend(words.into_iter().map(|word| Word::Text(word.into())));
        self
    }
    /// Add a placeholder the reader fills in; it is printed unquoted.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.words.push(Word::Placeholder(text.into()));
        self
    }
    /// Whether it runs as printed: no word is a placeholder.
    #[must_use]
    pub fn runnable(&self) -> bool {
        !self
            .words
            .iter()
            .any(|word| matches!(word, Word::Placeholder(_)))
    }
    /// The words after `cargo atlas`, as data: a placeholder in `<angle
    /// brackets>`, and no project choice.
    #[must_use]
    pub fn words(&self) -> Vec<String> {
        self.words
            .iter()
            .map(|word| match word {
                Word::Text(text) => text.clone(),
                Word::Path(key) if key.is_empty() => ".".to_owned(),
                Word::Path(key) => key.clone(),
                Word::Placeholder(text) => format!("<{text}>"),
            })
            .collect()
    }
}
/// As data, a command is its words: placeholders marked, no project.
impl serde::Serialize for Command {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.words().serialize(serializer)
    }
}
impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PROGRAM)?;
        for word in &self.words {
            match word {
                Word::Text(text) => write!(f, " {}", quoted(text))?,
                Word::Path(key) => write!(f, " {}", quoted(&shown(key)))?,
                Word::Placeholder(text) => write!(f, " <{text}>")?,
            }
        }
        if self.adds_project {
            for word in RUN.get().into_iter().flat_map(|run| &run.project) {
                write!(f, " {}", quoted(word))?;
            }
        }
        Ok(())
    }
}

/// A workspace path, by its root-relative key, as someone types it where
/// this run is: relative to the current directory (`.` for it).
#[must_use]
pub fn shown(key: &str) -> String {
    match RUN.get() {
        Some(run) => relative(&run.here, &run.root.join(key)),
        None if key.is_empty() => ".".to_owned(),
        None => key.to_owned(),
    }
}
/// `target` as a path from the directory `here`, both canonical.
fn relative(here: &Path, target: &Path) -> String {
    let shared = here
        .components()
        .zip(target.components())
        .take_while(|(a, b)| a == b)
        .count();
    let mut path: PathBuf = here.components().skip(shared).map(|_| "..").collect();
    path.extend(target.components().skip(shared));
    if path.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        path.to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
    }
}

/// `word` as a shell reads it back: as is when it is plain, otherwise in
/// single quotes (a query program has spaces, quotes and braces).
#[must_use]
pub fn quoted(word: &str) -> String {
    // `~` is plain inside a word (`HEAD~1`); only a leading one expands.
    let plain = !word.is_empty()
        && !word.starts_with('~')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./:@=,+%^~".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_written_from_the_current_directory() {
        let root = Path::new("/w");
        for (here, key, written) in [
            ("/w", "src/lib.rs", "src/lib.rs"),
            ("/w/crates/a", "crates/a/src", "src"),
            ("/w/crates/a", "src/lib.rs", "../../src/lib.rs"),
            ("/w/crates/a", "crates/a", "."),
            ("/elsewhere", "src", "../w/src"),
        ] {
            assert_eq!(
                relative(Path::new(here), &root.join(key)),
                written,
                "{here} {key}"
            );
        }
    }

    #[test]
    fn a_command_quotes_what_a_shell_would_split_and_marks_placeholders() {
        let command = Command::new("module")
            .arg("src/a b")
            .placeholder("bin root");
        assert_eq!(
            command.to_string(),
            "cargo atlas module 'src/a b' <bin root>"
        );
        assert!(!command.runnable());
        assert_eq!(command.words(), ["module", "src/a b", "<bin root>"]);
    }
}
