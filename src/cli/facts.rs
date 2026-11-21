//! Export the captured observations as flat relations, or print their schema.
//! The command adds no evidence of its own: it flattens the lexical graph and
//! the qualified facts that other commands report in their own shapes.
use super::output::form::{As, formats};
use atlas_lib::{
    analysis::Analysis,
    discovery,
    model::SourceGraph,
    relations::{Family, Relations, schema},
};
use cargo_atlas::Result;
use clap::{Args, ValueEnum};
use std::{fmt::Write as _, io::Write, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    /// One document: each relation's columns and rows (rows are arrays).
    Json,
    /// A header line, then one object per row with a `relation` key.
    Jsonl,
}
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FamilyArg {
    Lexical,
    Qualified,
    /// Needs --scip FILE or --semantic.
    Semantic,
}
#[derive(Args)]
pub(crate) struct Options {
    /// Families to export; the qualified family needs the slower analysis.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "lexical,qualified"
    )]
    families: Vec<FamilyArg>,
    /// `json` (the default) is one document: each relation's columns and
    /// rows (rows are arrays). `jsonl` is a header line, then one object per
    /// row with a `relation` key. With --schema, `text` (the default) is a
    /// readable listing and `json` the schema as data.
    #[arg(
        long = "as",
        id = "as",
        value_name = "FORMAT",
        value_parser = formats(&[As::Text, As::Json, As::Jsonl])
    )]
    form: Option<As>,
    /// Superseded by --as, with the same values.
    #[arg(long, value_enum)]
    format: Option<Format>,
    /// The SCIP index the semantic family is read from (with --families
    /// ...,semantic). A relative path is resolved against the workspace root.
    #[arg(long, value_name = "FILE")]
    scip: Option<PathBuf>,
    #[command(flatten)]
    indexer: super::semantic::IndexerArgs,
    #[command(flatten)]
    tests: super::tests::Tests,
    /// Write to this file instead of standard output.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Print every relation and column, what each means, and each relation's
    /// basis and unknowns companion, without reading the source; --as json
    /// for the schema as data.
    #[arg(long)]
    schema: bool,
}

impl Options {
    fn format(&self) -> Format {
        match self.form {
            Some(As::Jsonl) => Format::Jsonl,
            Some(As::Json) => Format::Json,
            Some(As::Text | As::Svg | As::Html) | None => self.format.unwrap_or(Format::Json),
        }
    }
}

/// `facts --schema` as text: each relation with its family and meaning, its
/// columns with their kinds and meanings, and what it keeps apart.
fn schema_text() -> String {
    let mut out = format!(
        "Atlas facts, schema {}: {} relations in two families, lexical (names as written) and qualified (names followed through use statements and modules). --as json prints this as data; cargo atlas help --topic facts explains it.\n",
        schema::VERSION,
        schema::RELATIONS.len()
    );
    for relation in schema::RELATIONS {
        let _ = writeln!(
            out,
            "\n{} ({}): {}",
            relation.name,
            relation.family.name(),
            relation.description
        );
        let width = relation
            .columns
            .iter()
            .map(|c| c.name.len())
            .max()
            .unwrap_or(0);
        for column in relation.columns {
            let kind = atlas_lib::relations::code(&column.kind);
            let _ = writeln!(
                out,
                "  {:width$}  {kind:7}  {}",
                column.name, column.description
            );
        }
        if let Some(basis) = relation.basis {
            let _ = writeln!(
                out,
                "  basis column: {basis} (keep it, or pin it to \"unique\")"
            );
        }
        if let Some(unknowns) = relation.unknowns {
            let _ = writeln!(
                out,
                "  unresolved observations: {unknowns} (no row here is not evidence of none)"
            );
        }
        for note in relation.notes {
            let _ = writeln!(out, "  note: {note}");
        }
    }
    out
}

pub(crate) fn schema() -> Result<String> {
    #[derive(serde::Serialize)]
    struct Document {
        schema: u32,
        families: [Family; 3],
        relations: &'static [schema::Relation],
    }
    super::json(&Document {
        schema: schema::VERSION,
        families: Family::ALL,
        relations: schema::RELATIONS,
    })
}

pub(crate) fn run(
    metadata: &cargo_metadata::Metadata,
    options: &Options,
    cache: &super::semantic::IndexCache,
) -> Result<()> {
    if options.schema {
        // Text unless the schema is asked for as data.
        let data = matches!(options.form, Some(As::Json | As::Jsonl)) || options.format.is_some();
        let text = if data { schema()? } else { schema_text() };
        std::io::stdout().lock().write_all(text.as_bytes())?;
        return Ok(());
    }
    if options.form == Some(As::Text) {
        return Err(
            "facts prints JSON (--as json) or JSON Lines (--as jsonl); --as text is for --schema"
                .into(),
        );
    }
    let semantic = options.families.contains(&FamilyArg::Semantic);
    if semantic && options.scip.is_none() && !options.indexer.semantic {
        return Err("--families semantic needs an index: give --scip FILE, or --semantic, which runs rust-analyzer and with it the project's build scripts".into());
    }
    if !semantic && (options.scip.is_some() || options.indexer.semantic) {
        return Err("--scip and --semantic read the semantic family: add it to --families".into());
    }
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let analysis = (options.families.contains(&FamilyArg::Qualified) || semantic)
        .then(|| Analysis::new(&snapshot).map(|a| a.with_tests(options.tests.selection())))
        .transpose()?;
    let facts = analysis
        .as_ref()
        .map(Analysis::representations)
        .transpose()?;
    let relations = match (options.families.contains(&FamilyArg::Lexical), &facts) {
        (true, facts) => Relations::capture(graph, facts.as_ref()),
        (false, Some(facts)) => Relations::capture_qualified(graph, facts),
        (false, None) => return Err("--families names no family".into()),
    };
    let relations = match (&analysis, &facts, semantic) {
        (Some(analysis), Some(facts), true) => {
            let root = metadata.workspace_root.as_std_path();
            let (index, _) = super::semantic::obtain(
                root,
                metadata,
                options.scip.as_deref(),
                &options.indexer,
                cache,
                || super::output::graph_sources(metadata, graph).collect(),
            )?
            .ok_or("no semantic index was asked for")?;
            relations.with_semantic(&analysis.semantic(&index)?, facts)?
        }
        _ => relations,
    };
    emit(&relations, options, metadata, graph)
}

/// Write the export to `--out`, refusing to overwrite any scanned input, or
/// stream it to stdout.
fn emit(
    relations: &Relations,
    options: &Options,
    metadata: &cargo_metadata::Metadata,
    graph: &SourceGraph,
) -> Result<()> {
    let write = |out: &mut dyn Write| -> Result<()> {
        match options.format() {
            Format::Json => {
                relations.write_json(&mut *out)?;
                out.write_all(b"\n")?;
            }
            Format::Jsonl => relations.write_jsonl(&mut *out)?,
        }
        Ok(())
    };
    if let Some(path) = &options.out {
        let mut bytes = Vec::new();
        write(&mut bytes)?;
        super::output::write_protected(
            path,
            bytes,
            &super::output::protected_inputs(metadata, graph, []),
        )
    } else {
        // Stream: the document is never held as bytes as well as cells.
        let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
        write(&mut stdout)?;
        Ok(stdout.flush()?)
    }
}
