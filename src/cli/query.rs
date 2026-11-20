//! Run a Datalog program over the facts: captured now, only the families
//! the program reads, or read from a saved `facts` export. The command
//! reports rows and nothing else; it exits 0 whenever the program ran,
//! whatever the rows, and refuses (exit 1) a program it cannot check, an
//! honesty finding not accepted, or a limit passed.
use atlas_lib::{
    analysis::Analysis,
    discovery,
    query::{self, Check, Limits, Options as QueryOptions},
    relations::{Cell, Family, Relations},
};
use cargo_atlas::Result;
use clap::{Args, ValueEnum};
use std::{
    fmt::Write as _,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Accept {
    /// Run a program that drops a resolution basis column.
    Basis,
    /// Run a program that negates a relation without reading its unknowns.
    Absence,
}

#[derive(Args)]
pub(crate) struct Options {
    /// The program text: rules and `?-` queries. Use -f to read a file.
    program: Option<String>,
    /// Read the program from FILE (`-` for standard input).
    #[arg(
        short = 'f',
        long = "file",
        value_name = "FILE",
        conflicts_with = "program"
    )]
    file: Option<PathBuf>,
    /// Query a saved `cargo atlas facts` export instead of capturing now.
    #[arg(long, value_name = "FILE")]
    facts: Option<PathBuf>,
    /// Report findings of this honesty check with the answer instead of
    /// refusing the program.
    #[arg(long, value_enum, value_delimiter = ',')]
    accept: Vec<Accept>,
    #[arg(long)]
    json: bool,
    /// Rows to print per query in text output (JSON prints all).
    #[arg(long, default_value_t = 100)]
    show: usize,
    /// Refuse once the rules derive more rows than this.
    #[arg(long, default_value_t = 20_000_000)]
    max_rows: usize,
    /// Refuse once evaluation has run this long.
    #[arg(long, default_value_t = 120)]
    timeout_seconds: u64,
    /// Print the prelude's rules and exit.
    #[arg(long)]
    prelude: bool,
}

fn program_text(options: &Options) -> Result<String> {
    match (&options.program, &options.file) {
        (Some(text), _) => Ok(text.clone()),
        (None, Some(path)) if path == Path::new("-") => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            Ok(text)
        }
        (None, Some(path)) => std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()).into()),
        (None, None) => Err(
            "give a program, such as `cargo atlas query '?- file_calls{from: A, to: B, calls: N}.'`, or -f FILE"
                .into(),
        ),
    }
}

/// Files whose bytes no longer match the export's fingerprints.
fn changed_since(root: &Path, relations: &Relations) -> (usize, Vec<String>) {
    let mut seen = std::collections::BTreeMap::new();
    for name in ["file", "qualified_file"] {
        let Some(relation) = relations.get(name) else {
            continue;
        };
        let (Some(path), Some(fingerprint)) = (
            relation.schema.position("path"),
            relation.schema.position("fingerprint"),
        ) else {
            continue;
        };
        for row in &relation.rows {
            if let (Some(p), Some(f)) = (row[path].text(), row[fingerprint].text()) {
                seen.insert(p.to_owned(), f.to_owned());
            }
        }
    }
    let changed = seen
        .iter()
        .filter(|(path, fingerprint)| {
            !std::fs::read(root.join(path))
                .is_ok_and(|bytes| discovery::fingerprint(&bytes) == **fingerprint)
        })
        .map(|(path, _)| path.clone())
        .collect();
    (seen.len(), changed)
}

fn capture(metadata: &cargo_metadata::Metadata, families: &[Family]) -> Result<Relations> {
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = if families.contains(&Family::Qualified) {
        Some(Analysis::new(&snapshot)?.representations()?)
    } else {
        None
    };
    Ok(match (families.contains(&Family::Lexical), &facts) {
        (false, Some(facts)) => Relations::capture_qualified(graph, facts),
        (_, facts) => Relations::capture(graph, facts.as_ref()),
    })
}

fn render(cell: &Cell) -> String {
    match cell {
        Cell::Text(text) => text.replace('\n', "\\n").replace('\t', "\\t"),
        Cell::Integer(n) => n.to_string(),
        Cell::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

fn text(answer: &query::Answer, source: &str, show: usize) -> String {
    let mut out = String::new();
    for (i, table) in answer.tables.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = writeln!(out, "{}", table.query);
        let rows: Vec<Vec<String>> = table
            .rows
            .iter()
            .take(show)
            .map(|row| row.iter().map(render).collect())
            .collect();
        let mut widths: Vec<usize> = table.columns.iter().map(|c| c.chars().count()).collect();
        for row in &rows {
            for (w, cell) in widths.iter_mut().zip(row) {
                *w = (*w).max(cell.chars().count());
            }
        }
        let line = |cells: &[String]| {
            let last = cells.len().saturating_sub(1);
            cells
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(c, (cell, w))| {
                    if c == last {
                        cell.clone()
                    } else {
                        format!("{cell:<w$}")
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
        };
        if !table.columns.is_empty() {
            let _ = writeln!(out, "  {}", line(&table.columns));
        }
        for row in &rows {
            let _ = writeln!(out, "  {}", line(row));
        }
        let shown = rows.len();
        let _ = match (table.total, table.rows.len()) {
            (0, _) => writeln!(out, "  (no rows)"),
            (total, listed) if shown < listed => writeln!(
                out,
                "  ({total} rows, {shown} shown; raise --show or use --json)"
            ),
            (total, listed) if listed < total => {
                writeln!(
                    out,
                    "  ({total} rows, the first {listed} by the query's limit)"
                )
            }
            (total, _) => writeln!(out, "  ({total} rows)"),
        };
    }
    for accepted in &answer.accepted {
        let _ = writeln!(out, "\n{accepted}");
    }
    if !answer.notes.is_empty() {
        out.push('\n');
        for note in &answer.notes {
            let _ = writeln!(out, "note: {note}");
        }
    }
    let _ = writeln!(
        out,
        "\nFacts: {source}. Derived {} rows in {} ms. Rows are observations, not proof of use or absence.",
        answer.derived,
        answer.elapsed.as_millis()
    );
    out
}

pub(crate) fn run(metadata: &cargo_metadata::Metadata, options: &Options) -> Result<()> {
    if options.prelude {
        std::io::stdout()
            .lock()
            .write_all(query::PRELUDE.as_bytes())?;
        return Ok(());
    }
    let program = query::parse(&program_text(options)?)?;
    let mut check_options = QueryOptions::default();
    for accept in &options.accept {
        check_options = check_options.accept(match accept {
            Accept::Basis => Check::Basis,
            Accept::Absence => Check::Absence,
        });
    }
    let root = metadata.workspace_root.as_std_path();
    let (relations, checked, source, mut notes) = if let Some(path) = &options.facts {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let relations = Relations::read(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let checked = query::check(&program, &relations.families, &check_options)?;
        let (files, changed) = changed_since(root, &relations);
        let mut notes = Vec::new();
        if !changed.is_empty() {
            let mut listed: Vec<&str> = changed.iter().map(String::as_str).take(5).collect();
            if changed.len() > listed.len() {
                listed.push("…");
            }
            notes.push(format!(
                "{} of the export's {files} files differ from the tree now (missing or changed): {}; its rows describe the files as they were",
                changed.len(),
                listed.join(", ")
            ));
        }
        (relations, checked, format!("{}", path.display()), notes)
    } else {
        let checked = query::check(&program, &Family::ALL, &check_options)?;
        let relations = capture(metadata, checked.families())?;
        let families: Vec<&str> = checked.families().iter().map(|f| f.name()).collect();
        let source = if families.is_empty() {
            "none read".to_owned()
        } else {
            format!("captured now ({})", families.join(", "))
        };
        (relations, checked, source, Vec::new())
    };
    let limits = Limits::default()
        .rows(options.max_rows)
        .time(Duration::from_secs(options.timeout_seconds));
    let mut answer = query::run(&checked, &relations, limits)?;
    notes.append(&mut answer.notes);
    answer.notes = notes;
    let mut stdout = std::io::stdout().lock();
    if options.json {
        #[derive(serde::Serialize)]
        struct Document<'a> {
            schema: u32,
            facts: &'a str,
            #[serde(flatten)]
            answer: &'a query::Answer,
        }
        stdout.write_all(
            super::json(&Document {
                schema: 1,
                facts: &source,
                answer: &answer,
            })?
            .as_bytes(),
        )?;
    } else {
        stdout.write_all(text(&answer, &source, options.show).as_bytes())?;
    }
    Ok(())
}
