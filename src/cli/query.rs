//! Run a Datalog program over the facts: captured now, only the families
//! the program reads, or read from a saved `facts` export. The command
//! reports rows and nothing else; it exits 0 whenever the program ran,
//! whatever the rows, and refuses (exit 1) a program it cannot check, an
//! honesty finding not accepted, or a limit passed.
use crate::cli::output::form::As;
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
    #[arg(long, value_name = "FILE", value_parser = crate::cli::typed::file)]
    facts: Option<PathBuf>,
    #[command(flatten)]
    tests: super::tests::Tests,
    /// Read the semantic family (`semantic_call`, `semantic_unknown`) from
    /// this SCIP index, for example from `rust-analyzer scip`. A relative
    /// path is read from the current directory. Atlas runs no indexer.
    #[arg(long, value_name = "FILE", conflicts_with = "facts", value_parser = crate::cli::typed::file)]
    scip: Option<PathBuf>,
    #[command(flatten)]
    indexer: super::semantic::IndexerArgs,
    /// Report findings of this honesty check with the answer instead of
    /// refusing the program.
    #[arg(long, value_enum, value_delimiter = ',')]
    accept: Vec<Accept>,
    #[command(flatten)]
    output: crate::cli::output::form::Report,
    #[command(flatten)]
    rows: crate::cli::output::rows::Rows,
    /// Superseded by --rows.
    #[arg(long, value_name = "N", conflicts_with_all = ["rows", "all"])]
    show: Option<usize>,
    /// Refused: the rules restrict what they read (spec §3.8).
    #[arg(long = "in", hide = true)]
    in_path: Option<String>,
    #[arg(short = 'p', long, hide = true)]
    package: Option<String>,
    #[arg(long, hide = true)]
    vs: Option<String>,
    #[arg(long, hide = true)]
    at: Option<String>,
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

/// `query --help`'s closing note. Its list of prelude relations is read from
/// the prelude itself, by section, so it names every relation the prelude
/// defines.
pub(crate) fn after_help() -> String {
    format!(
        "Answers: any question the facts can answer, written as rules over named fields: join, recurse, negate and count.\nExample: cargo atlas query '?- file_calls{{from: A, to: B, calls: N}}, under(A, \"src\") order by N desc limit 10.'\nThe prelude (--prelude) restates commands as relations: {}. Fields are those of `facts --schema`.\nHonesty: a rule that drops a candidate's basis, or negates calls without reading unknown_call, is refused unless you --accept that check; the acceptance is printed with the answer. Rows are observations: an empty answer is not proof of absence.\nExit 0 whenever the program ran, whatever the rows. Recipes: cargo atlas help agent. The language: cargo atlas help --topic query.",
        prelude_sections()
            .iter()
            .map(|(section, names)| format!("{} ({section})", names.join(", ")))
            .collect::<Vec<_>>()
            .join("; ")
    )
}
/// The prelude's sections (`% --- name ---`) and the relations each defines,
/// in the order written.
pub(crate) fn prelude_sections() -> Vec<(String, Vec<String>)> {
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in query::PRELUDE.lines() {
        if let Some(title) = line.strip_prefix("% --- ") {
            sections.push((title.trim_end_matches(['-', ' ']).to_owned(), Vec::new()));
        } else if let Some((head, _)) = line.split_once('{')
            && !head.is_empty()
            && head.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            && let Some((_, names)) = sections.last_mut()
            && !names.iter().any(|name| name == head)
        {
            names.push(head.to_owned());
        }
    }
    sections
}

/// The restriction the scope-flag refusal shows; a test runs it in a query.
const RESTRICT: &str = "file{path: P}, under(P, \"src\")";

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
        (None, None) => Err(format!(
            "give a program, such as `{}`, or -f FILE",
            cargo_atlas::suggestion::Command::new("query")
                .arg("?- file_calls{from: A, to: B, calls: N}.")
        )
        .into()),
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

/// Capture the families a program reads. The semantic family needs an index
/// (`--scip FILE`, or `--semantic` to make one) and the qualified facts its
/// rows name functions by.
fn capture(
    metadata: &cargo_metadata::Metadata,
    families: &[Family],
    options: &Options,
    cache: &super::semantic::IndexCache,
) -> Result<Relations> {
    let semantic = families.contains(&Family::Semantic);
    if semantic && options.scip.is_none() && !options.indexer.semantic {
        return Err("the program reads the semantic family (semantic_call, semantic_unknown, any_call), which needs an index: give --scip FILE, or --semantic, which runs rust-analyzer and with it the project's build scripts".into());
    }
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let analysis = (families.contains(&Family::Qualified) || semantic)
        .then(|| Analysis::new(&snapshot).map(|a| a.with_tests(options.tests.selection())))
        .transpose()?;
    let facts = analysis
        .as_ref()
        .map(Analysis::representations)
        .transpose()?;
    let relations = match (families.contains(&Family::Lexical), &facts) {
        (false, Some(facts)) => Relations::capture_qualified(graph, facts),
        (_, facts) => Relations::capture(graph, facts.as_ref()),
    };
    let (Some(analysis), Some(facts), true) = (&analysis, &facts, semantic) else {
        return Ok(relations);
    };
    let root = metadata.workspace_root.as_std_path();
    let (index, _) = super::semantic::obtain(
        root,
        metadata,
        options.scip.as_deref(),
        &options.indexer,
        cache,
        || super::output::graph_captures(metadata, graph).collect(),
    )?
    .ok_or("no semantic index was asked for")?;
    relations.with_semantic(&analysis.semantic(&index)?, facts)
}

fn render(cell: &Cell) -> String {
    match cell {
        Cell::Text(text) => text.replace('\n', "\\n").replace('\t', "\\t"),
        Cell::Integer(n) => n.to_string(),
        Cell::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

/// `standing` are the facts' fixed limitations, the same on every run: the
/// text folds them into one line, and JSON keeps each.
fn text(answer: &query::Answer, source: &str, show: usize, standing: &[String]) -> String {
    let more = crate::cli::output::rows::rerun_all();
    let mut out = String::new();
    for (i, table) in answer.tables.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_table(&mut out, table, show, &more);
    }
    for accepted in &answer.accepted {
        let _ = writeln!(out, "\n{accepted}");
    }
    let (fixed, notes): (Vec<&String>, Vec<&String>) = answer
        .notes
        .iter()
        .partition(|note| standing.contains(note));
    if !notes.is_empty() || !fixed.is_empty() {
        out.push('\n');
        for note in notes {
            let _ = writeln!(out, "note: {note}");
        }
        if !fixed.is_empty() {
            let _ = writeln!(
                out,
                "note: the facts read carry {} standing limits (production source only, no macro expansion, conservative call resolution); cargo atlas help concepts explains them, and --as json lists each",
                fixed.len()
            );
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

/// One table as aligned columns, its first `show` rows, then a count that
/// says whether the display or the query's own limit held rows back.
fn write_table(out: &mut String, table: &query::Table, show: usize, more: &str) {
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
            "{}\n  ({total} rows)",
            crate::cli::output::rows::tail("  ", listed - shown, more)
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

pub(crate) fn run(
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    cache: &super::semantic::IndexCache,
) -> Result<()> {
    if options.in_path.is_some()
        || options.package.is_some()
        || options.vs.is_some()
        || options.at.is_some()
    {
        return Err(format!(
            "query reads the whole workspace's facts; restrict inside the rules (for example {RESTRICT})"
        )
        .into());
    }
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
    let Reading {
        relations,
        checked,
        source,
        mut notes,
    } = if let Some(path) = &options.facts {
        read_export(root, path, &program, &check_options, options)?
    } else {
        capture_now(metadata, &program, &check_options, options, cache)?
    };
    let limits = Limits::default()
        .rows(options.max_rows)
        .time(Duration::from_secs(options.timeout_seconds));
    let mut answer = query::run(&checked, &relations, limits)?;
    notes.append(&mut answer.notes);
    answer.notes = notes;
    print_answer(options, &source, &answer, &relations.limitations)
}

/// The facts a checked program runs over, where they came from, and notes
/// about them that belong with the answer.
struct Reading {
    relations: Relations,
    checked: query::Checked,
    source: String,
    notes: Vec<String>,
}

/// Facts from a saved export, with a note when files it describes differ
/// from the tree now.
fn read_export(
    root: &Path,
    path: &Path,
    program: &query::Program,
    check_options: &QueryOptions,
    options: &Options,
) -> Result<Reading> {
    if options.tests.asked() {
        return Err(format!(
            "--with-tests captures now; a saved export has the population it was exported with ({})",
            cargo_atlas::suggestion::Command::new("facts").arg("--with-tests")
        )
        .into());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let relations = Relations::read(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let checked = query::check(program, &relations.families, check_options)?;
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
    Ok(Reading {
        relations,
        checked,
        source: format!("{}", path.display()),
        notes,
    })
}

/// Facts captured now: only the families the checked program reads.
fn capture_now(
    metadata: &cargo_metadata::Metadata,
    program: &query::Program,
    check_options: &QueryOptions,
    options: &Options,
    cache: &super::semantic::IndexCache,
) -> Result<Reading> {
    let checked = query::check(program, &Family::ALL, check_options)?;
    let relations = capture(metadata, checked.families(), options, cache)?;
    let families: Vec<&str> = checked.families().iter().map(|f| f.name()).collect();
    let source = if families.is_empty() {
        "none read".to_owned()
    } else {
        let tests = options.tests.asked()
            && checked
                .families()
                .iter()
                .any(|family| *family != Family::Lexical);
        format!(
            "captured now ({}{})",
            families.join(", "),
            if tests { "; test code included" } else { "" }
        )
    };
    Ok(Reading {
        relations,
        checked,
        source,
        notes: Vec::new(),
    })
}

/// The answer as JSON Lines, one JSON document, or text tables.
fn print_answer(
    options: &Options,
    source: &str,
    answer: &query::Answer,
    standing: &[String],
) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    if options.output.chosen() == Some(As::Jsonl) {
        return write_jsonl(&mut stdout, source, answer);
    }
    if options.output.json() {
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
                facts: source,
                answer,
            })?
            .as_bytes(),
        )?;
    } else {
        let show = options.show.unwrap_or_else(|| options.rows.limit(20));
        stdout.write_all(text(answer, source, show, standing).as_bytes())?;
    }
    Ok(())
}

/// JSON Lines: one header object (the answer without its rows, each table's
/// rows replaced by its row count), then one `{"table": i, "row": [...]}`
/// object per row, in table order.
fn write_jsonl(out: &mut impl Write, source: &str, answer: &query::Answer) -> Result<()> {
    let mut header = serde_json::to_value(answer)?;
    let mut rows = Vec::new();
    if let Some(tables) = header.get_mut("tables").and_then(|t| t.as_array_mut()) {
        for (index, table) in tables.iter_mut().enumerate() {
            if let Some(serde_json::Value::Array(listed)) =
                table.as_object_mut().and_then(|t| t.remove("rows"))
            {
                rows.extend(listed.into_iter().map(|row| (index, row)));
            }
        }
    }
    if let Some(object) = header.as_object_mut() {
        object.insert("schema".into(), 1.into());
        object.insert("facts".into(), source.into());
    }
    serde_json::to_writer(&mut *out, &header)?;
    writeln!(out)?;
    for (table, row) in rows {
        serde_json::to_writer(&mut *out, &serde_json::json!({"table": table, "row": row}))?;
        writeln!(out)?;
    }
    Ok(())
}
