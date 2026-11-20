//! Datalog over the flat relations of [`crate::relations`], so a caller can
//! ask a question no command asks: join, recurse, negate and count over the
//! same observations the commands report, by name.
//!
//! A program is rules and queries over named fields:
//!
//! ```text
//! calls_into{from: A, to: B} :- call{from: F, to: G}, function{id: F, path: A},
//!                                function{id: G, path: B}, A != B.
//! ?- calls_into{from: A, to: B}, under(A, "src") order by A limit 20.
//! ```
//!
//! [`parse`] reads the text, [`check`] resolves every relation and field
//! against the facts schema and the [prelude](PRELUDE), shows each rule safe
//! and the program stratified, and [`run`] evaluates it bottom-up. The
//! answer is rows: a query has no pass or fail, and an empty answer is not a
//! finding by itself. Rows are sets, so a query keeps the witness ids the
//! facts carry when it needs to count observations rather than values.
//!
//! The engine evaluates what the facts observed and nothing more. A row
//! derived through a candidate mention is a candidate, and a missing call is
//! not evidence that a function is unused while unresolved calls could
//! match; the facts keep both in their own columns and relations, and a
//! query decides whether to read them.
mod check;
mod eval;
mod syntax;

pub use check::Checked;
pub use syntax::Program;

use crate::relations::{Cell, Family, Relations};
use serde::Serialize;
use std::{fmt, time::Duration};

/// Rules every program may read: command-equivalent questions over the
/// facts, written in the language itself. `cargo atlas query --prelude`
/// prints them.
pub const PRELUDE: &str = include_str!("prelude.dl");

/// Where a clause was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Origin {
    /// The caller's program.
    Program,
    /// [`PRELUDE`].
    Prelude,
}

/// A position in a program's text, counted from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

/// The stage that refused a program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ErrorKind {
    /// The text is not a program.
    Syntax,
    /// A relation, field or value does not fit the facts or a rule head.
    Schema,
    /// A variable is not bound by a positive relation, or is used once.
    Safety,
    /// A relation depends on itself through negation or `count`.
    Stratification,
    /// Evaluation passed a limit before finishing.
    Limit,
    /// The facts given do not hold what the program reads.
    Facts,
}

/// Why a program was refused, with the place in its text when there is one.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub origin: Option<Origin>,
    pub location: Option<Location>,
    /// The source line the location points into.
    pub line: Option<String>,
}
impl Error {
    pub(crate) fn plain(kind: ErrorKind, message: String) -> Self {
        Self {
            kind,
            message,
            origin: None,
            location: None,
            line: None,
        }
    }
    pub(crate) fn located(
        kind: ErrorKind,
        origin: Origin,
        source: &str,
        at: Location,
        message: String,
    ) -> Self {
        Self {
            kind,
            message,
            origin: Some(origin),
            location: Some(at),
            line: source
                .lines()
                .nth(at.line.saturating_sub(1))
                .map(str::to_owned),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(at) = self.location else {
            return f.write_str(&self.message);
        };
        let name = match self.origin {
            Some(Origin::Prelude) => "prelude",
            _ => "query",
        };
        write!(f, "{name}:{}:{}: {}", at.line, at.column, self.message)?;
        if let Some(line) = &self.line {
            let pad: String = line
                .chars()
                .take(at.column.saturating_sub(1))
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect();
            write!(f, "\n  | {line}\n  | {pad}^")?;
        }
        Ok(())
    }
}
impl std::error::Error for Error {}

/// Bounds on one evaluation. Each refuses with the relation that grew, so
/// an unrooted recursion fails fast instead of filling memory.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Limits {
    /// Derived rows in all, and rows one rule may produce in one pass.
    pub rows: usize,
    /// Wall time for evaluation (not for capturing the facts).
    pub time: Duration,
    /// Rounds of recursion in one stratum.
    pub rounds: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            rows: 20_000_000,
            time: Duration::from_secs(120),
            rounds: 100_000,
        }
    }
}
impl Limits {
    #[must_use]
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows;
        self
    }
    #[must_use]
    pub fn time(mut self, time: Duration) -> Self {
        self.time = time;
        self
    }
    #[must_use]
    pub fn rounds(mut self, rounds: usize) -> Self {
        self.rounds = rounds;
        self
    }
}

/// One query's rows, in its `order by` order and then by every column.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Table {
    /// The query as written.
    pub query: String,
    pub columns: Vec<String>,
    /// `text`, `integer` or `boolean`, per column.
    pub kinds: Vec<&'static str>,
    pub rows: Vec<Vec<Cell>>,
    /// Rows before `limit`.
    pub total: usize,
}

/// The answer to every query of a program.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct Answer {
    pub tables: Vec<Table>,
    /// Rows the rules derived, queries included.
    pub derived: usize,
    #[serde(serialize_with = "millis")]
    pub elapsed: Duration,
    /// Honesty findings the caller accepted, so they stay visible.
    pub accepted: Vec<String>,
    /// What the answer cannot show, from the facts it read.
    pub notes: Vec<String>,
}

fn millis<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Read a program's text.
pub fn parse(source: &str) -> Result<Program, Error> {
    syntax::parse(source, Origin::Program)
}

/// Resolve `program` against the facts schema and the prelude for facts of
/// the given families, and keep only the rules its queries need.
pub fn check(program: &Program, families: &[Family]) -> Result<Checked, Error> {
    let prelude = syntax::parse(PRELUDE, Origin::Prelude)?;
    check::check(program, &prelude, families)
}

/// Evaluate a checked program over `relations`, which must hold the
/// relations it reads ([`Checked::families`]).
pub fn run(checked: &Checked, relations: &Relations, limits: Limits) -> Result<Answer, Error> {
    eval::run(checked, relations, limits)
}

#[cfg(test)]
mod tests;
