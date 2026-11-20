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
mod query;
mod review;
mod semantic;
mod stages;
mod subject;
mod types;
use args::{Cli, Command};
use cargo_atlas::Result;
use clap::{CommandFactory, FromArgMatches};
pub(crate) fn run() -> Result<()> {
    let mut args: Vec<_> = std::env::args_os().collect();
    // Cargo runs `cargo atlas ARGS` as `cargo-atlas atlas ARGS`.
    if args.get(1).is_some_and(|arg| arg == "atlas") {
        args.remove(1);
    }
    let matches = Cli::command().get_matches_from(&args);
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    let options = matches.subcommand().map(|(_, options)| options);
    let mut notices = output::notice::Notices::default();
    if let Some(options) = options {
        output::form::note_superseded(options, &mut notices);
    }
    if args::command_word(&args).is_some_and(|word| word == "representations") {
        notices.superseded("representations", "types");
    }
    // The bare words are read before the notice is printed, since a
    // positional ROOT among them is itself a superseded spelling.
    let selected_root = cli.selected_root(&mut notices);
    notices.print();
    let selected_root = selected_root?;
    if let Some(options) = options {
        output::form::agree(options)?;
    }
    if matches!(&cli.command, Command::Related(_)) {
        eprintln!(
            "atlas: related is a compatibility alias; use name --relations (lexical co-mentions)."
        );
    }
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
        Command::Flow(options) => flow::run(&root, &metadata, &options, &cache.semantic()),
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
        Command::Dump(args) => navigation::run_dump(&root, &metadata, &args, &cache.semantic()),
        Command::Facts(options) => facts::run(&metadata, &options),
        Command::Query(options) => query::run(&metadata, &options),
        Command::Census(args) => navigation::run_census(&metadata, &args),
        Command::Name(args) => navigation::run_name(&metadata, &args),
        Command::Module(args) => navigation::run_module(&metadata, &args),
        Command::Related(args) => navigation::run_related(&metadata, &args),
    };
    cache.finish(result.map_err(with_scope_hint))
}

/// An ambiguous scope names its candidates; end the refusal with this run's
/// command narrowed to the first of them, ready to run.
fn with_scope_hint(
    error: Box<dyn std::error::Error + Send + Sync>,
) -> Box<dyn std::error::Error + Send + Sync> {
    let command = if let Some(subject::Ambiguous { word, candidates }) =
        error.downcast_ref::<subject::Ambiguous>()
    {
        // A bare word becomes `--in`; a flag's value is replaced in place.
        candidates.first().and_then(|first| {
            args::this_command_with(word, first)
                .filter(|_| args::follows_flag(word))
                .or_else(|| args::this_command_with(word, &format!("--in {first}")))
        })
    } else if let Some(cargo_atlas::discovery::ScopeError::Ambiguous { scope, candidates }) =
        error.downcast_ref::<cargo_atlas::discovery::ScopeError>()
    {
        candidates
            .first()
            .and_then(|first| args::this_command_with(scope, first))
    } else {
        return error;
    };
    match command {
        Some(command) => format!("{error}; run: {command}").into(),
        None => error,
    }
}

fn json(value: &impl serde::Serialize) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}
