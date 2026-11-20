//! Export the captured observations as flat relations, or print their schema.
//! The command adds no evidence of its own: it flattens the lexical graph and
//! the qualified facts that other commands report in their own shapes.
use atlas_lib::{
    analysis::Analysis,
    discovery,
    relations::{Family, Relations, schema},
};
use cargo_atlas::Result;
use clap::{Args, ValueEnum};
use std::{io::Write, path::PathBuf};

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
    #[arg(long, value_enum, default_value = "json")]
    format: Format,
    /// Write to this file instead of standard output.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Print the relations, columns, bases and unknowns companions as JSON,
    /// without reading the source.
    #[arg(long)]
    schema: bool,
}

pub(crate) fn schema() -> Result<String> {
    #[derive(serde::Serialize)]
    struct Document {
        schema: u32,
        families: [Family; 2],
        relations: &'static [schema::Relation],
    }
    super::json(&Document {
        schema: schema::VERSION,
        families: Family::ALL,
        relations: schema::RELATIONS,
    })
}

pub(crate) fn run(metadata: &cargo_metadata::Metadata, options: &Options) -> Result<()> {
    if options.schema {
        std::io::stdout().lock().write_all(schema()?.as_bytes())?;
        return Ok(());
    }
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let facts = if options.families.contains(&FamilyArg::Qualified) {
        Some(Analysis::new(&snapshot)?.representations()?)
    } else {
        None
    };
    let mut relations = Relations::capture(graph, facts.as_ref());
    if !options.families.contains(&FamilyArg::Lexical) {
        relations
            .relations
            .retain(|relation| relation.schema.family != Family::Lexical);
        relations
            .families
            .retain(|family| *family != Family::Lexical);
    }
    let mut bytes = Vec::new();
    match options.format {
        Format::Json => {
            relations.write_json(&mut bytes)?;
            bytes.push(b'\n');
        }
        Format::Jsonl => relations.write_jsonl(&mut bytes)?,
    }
    match &options.out {
        Some(path) => super::output::write_protected(
            path,
            bytes,
            &super::output::protected_inputs(metadata, graph, []),
        ),
        None => Ok(std::io::stdout().lock().write_all(&bytes)?),
    }
}
