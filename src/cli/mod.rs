//! Resolve one project root, then dispatch to the command that owns the workflow.
//! Commands choose their evidence model; legacy navigation is resolved inside navigation.
mod args;
mod cache;
mod comparison;
mod delta;
mod dependencies;
mod documentation;
mod facts;
mod flow;
mod map;
mod navigation;
mod orient;
mod output;
mod overview;
mod policy;
mod profile;
mod program;
mod review;
mod stages;
mod types;
use args::{Cli, Command};
use cargo_atlas::Result;
use clap::Parser;
pub(crate) fn run() -> Result<()> {
    let mut args: Vec<_> = std::env::args_os().collect();
    // Cargo runs `cargo atlas ARGS` as `cargo-atlas atlas ARGS`.
    if args.get(1).is_some_and(|arg| arg == "atlas") {
        args.remove(1);
    }
    let cli = Cli::parse_from(args);
    if matches!(&cli.command, Command::Related(_)) {
        eprintln!(
            "atlas: related is a compatibility alias; use name --relations (lexical co-mentions)."
        );
    }
    let selected_root = cli.selected_root()?;
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(&selected_root)
        .no_deps()
        .exec()?;
    let root = metadata.workspace_root.as_std_path().canonicalize()?;
    let cache = cache::open(cli.no_cache, &root)?;
    let result = match cli.command {
        Command::Delta(options) => delta::run(&root, &metadata, &options),
        Command::Policy(options) => policy::run(&root, &metadata, &options),
        Command::Stages(options) => {
            stages::run(&root, &metadata, &options, stages::Question::Stages)
        }
        Command::Seams(options) => stages::seams(&root, &metadata, &options),
        Command::Comments(options) => documentation::run(&metadata, &options),
        Command::Diff(options) => review::diff(&root, &metadata, &options),
        Command::Visibility(options) => {
            review::run(&root, &metadata, &options, review::Question::Visibility)
        }
        Command::Moved(options) => review::run(&root, &metadata, &options, review::Question::Moved),
        Command::Compare(options) => comparison::run(&root, &metadata, &options),
        Command::Overview(options) => overview::run(&root, &metadata, options),
        Command::Dependencies(options) => dependencies::run(&root, &metadata, &options),
        Command::Types(options) => types::run(&root, &metadata, &options),
        Command::Flow(options) => flow::run(&root, &metadata, &options),
        Command::Spans(options) => {
            review::source_review(&metadata, &options, review::SourceQuestion::Spans)
        }
        Command::NontestLoc(options) => {
            review::source_review(&metadata, &options, review::SourceQuestion::Loc)
        }
        Command::Concentration(options) => {
            review::source_review(&metadata, &options, review::SourceQuestion::Concentration)
        }
        Command::Program(options) => program::run(&root, &metadata, &options),
        Command::Map(options) => map::run(&root, &metadata, &options),
        Command::Orient(options) => orient::run(&metadata, &selected_root, &options),
        Command::Dump(args) => navigation::run_dump(&metadata, &args),
        Command::Facts(options) => facts::run(&metadata, &options),
        Command::Census(args) => navigation::run_census(&metadata, &args),
        Command::Name(args) => navigation::run_name(&metadata, &args),
        Command::Module(args) => navigation::run_module(&metadata, &args),
        Command::Related(args) => navigation::run_related(&metadata, &args),
    };
    cache.finish(result)
}

fn json(value: &impl serde::Serialize) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}
