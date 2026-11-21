//! A command Atlas prints for someone to run: a refusal's hint, a list's
//! tail, `orient`'s next lines. Every printed command is built here and
//! rendered by one function, so each runs as written.
//!
//! A command reads the project it was printed for: the program that prints
//! it states once how this run chose its project (`--root PATH`), and every
//! command reading a project carries that choice. Before a program states
//! one, a command names no project, which is right when it runs from the
//! workspace root. Paths are workspace-root-relative keys.
use std::{fmt, sync::OnceLock};

/// The program's name, as every printed command and report title begins.
pub const PROGRAM: &str = "cargo atlas";

/// How this run chose its project, carried by every printed command that
/// reads one: the words to add, such as `["--root", "../app"]`.
static PROJECT: OnceLock<Vec<String>> = OnceLock::new();

/// State how this run chose its project, once, before anything is printed.
/// A later call is ignored: one run reads one project.
pub fn set_project(words: Vec<String>) {
    let _ = PROJECT.set(words);
}

/// A command after `cargo atlas`, as words.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct Command {
    words: Vec<Word>,
    /// Whether the run's project choice is added: not for `help`, which
    /// reads no project, nor for this run's own words, which hold it.
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
    /// `cargo atlas help ...`, which reads no project.
    pub fn help() -> Self {
        Self {
            words: vec![Word::Text("help".to_owned())],
            adds_project: false,
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
    /// A command from its words after `cargo atlas`, as [`Command::words`]
    /// gives them: a word in `<angle brackets>` is a placeholder.
    pub fn from_words(words: &[String]) -> Self {
        let [name, rest @ ..] = words else {
            return Self::new("");
        };
        let mut command = if name == "help" {
            Self::help()
        } else {
            Self::new(name)
        };
        for word in rest {
            command = match word.strip_prefix('<').and_then(|w| w.strip_suffix('>')) {
                Some(text) => command.placeholder(text),
                None => command.arg(word.as_str()),
            };
        }
        command
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
                Word::Text(text) | Word::Path(text) => text.clone(),
                Word::Placeholder(text) => format!("<{text}>"),
            })
            .collect()
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
            for word in PROJECT.get().into_iter().flatten() {
                write!(f, " {}", quoted(word))?;
            }
        }
        Ok(())
    }
}

/// A workspace path, by its root-relative key, as printed text writes it
/// for someone to type: the workspace root is `.`.
#[must_use]
pub fn shown(key: &str) -> String {
    if key.is_empty() {
        ".".to_owned()
    } else {
        key.to_owned()
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
        assert_eq!(
            Command::help().arg("--topic").arg("query").to_string(),
            "cargo atlas help --topic query"
        );
    }
}
