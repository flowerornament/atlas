//! Bounded text (spec §3.6): each list in text shows at most N rows, and a
//! list that left rows out ends with one tail line naming a command that shows
//! the rest. JSON is never bounded, and neither is text with `--all`.
use clap::Args;

/// How many rows each text list shows.
#[derive(Args, Clone, Copy, Default)]
pub(crate) struct Rows {
    /// Rows shown in each list of the text output; a list that leaves rows
    /// out ends with the command that shows them. JSON always holds every row.
    #[arg(long, value_name = "N")]
    rows: Option<usize>,
    /// Show every row of every list in the text output.
    #[arg(long, conflicts_with = "rows")]
    all: bool,
}
impl Rows {
    /// The rows a list shows: `--rows N`, every row with `--all`, otherwise
    /// the command's `default`.
    pub(crate) fn limit(self, default: usize) -> usize {
        if self.all {
            usize::MAX
        } else {
            self.rows.unwrap_or(default)
        }
    }
    /// The rows a list inside each of `outer` shown rows gets: `--rows N` as
    /// given, otherwise a share of `budget` lines, at least 3.
    pub(crate) fn nested(self, outer: usize, budget: usize) -> usize {
        self.limit((budget / outer.max(1)).max(3))
    }
}
/// The tail of a list that left `hidden` rows out, with `indent` before it:
/// `  … 12 more; cargo atlas spans --all`.
pub(crate) fn tail(indent: &str, hidden: usize, command: &str) -> String {
    format!("{indent}… {hidden} more; {command}")
}
/// This run's words after `cargo atlas`: Cargo's own `atlas` argument is
/// removed, as `cli::run` removes it before parsing.
pub(crate) fn this_run() -> Vec<String> {
    let mut words: Vec<String> = std::env::args().skip(1).collect();
    if words.first().is_some_and(|word| word == "atlas") {
        words.remove(0);
    }
    words
}
/// `cargo atlas WORDS`, each word quoted as a shell reads it back.
pub(crate) fn command(words: &[String]) -> String {
    let words: Vec<String> = words.iter().map(|word| quoted(word)).collect();
    format!("cargo atlas {}", words.join(" "))
}
/// This run's command line with `--rows N` dropped and `--all` added.
pub(crate) fn rerun_all() -> String {
    let mut kept = Vec::new();
    let mut words = this_run().into_iter();
    while let Some(word) = words.next() {
        // `--show` is query's 0.18 spelling of `--rows`.
        if word == "--rows" || word == "--show" {
            words.next();
        } else if !word.starts_with("--rows=") && !word.starts_with("--show=") && word != "--all" {
            kept.push(word);
        }
    }
    kept.push("--all".to_owned());
    command(&kept)
}
/// `word` as a shell reads it back: as is when it is plain, otherwise in
/// single quotes (a query program has spaces, quotes and braces).
fn quoted(word: &str) -> String {
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
/// `cargo atlas COMMAND SUBJECT [FLAGS] --all`, keeping this run's
/// `--root`: the narrower command a nested list's tail names.
pub(crate) fn narrower(question: &str, subject: &str, flags: &[&str]) -> String {
    let run = this_run();
    let root = run
        .iter()
        .position(|word| word == "--root")
        .and_then(|at| run.get(at + 1).cloned())
        .or_else(|| {
            run.iter()
                .find_map(|word| word.strip_prefix("--root=").map(str::to_owned))
        });
    let mut words: Vec<String> = [question, subject]
        .into_iter()
        .chain(flags.iter().copied())
        .map(str::to_owned)
        .collect();
    if let Some(root) = root {
        words.extend(["--root".to_owned(), root]);
    }
    words.push("--all".to_owned());
    command(&words)
}
/// Print up to `limit` of `rows` with `line`, then the tail if any were left
/// out.
pub(crate) fn print_bounded<T>(
    rows: &[T],
    limit: usize,
    indent: &str,
    command: impl FnOnce() -> String,
    mut line: impl FnMut(&T),
) {
    for row in rows.iter().take(limit) {
        line(row);
    }
    if rows.len() > limit {
        println!("{}", tail(indent, rows.len() - limit, &command()));
    }
}
