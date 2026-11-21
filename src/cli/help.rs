//! `cargo atlas help`: a command's help, or a topic from `docs/` compiled
//! into the binary, so a reader with only the binary has the guides. A topic
//! prints its file exactly; a test holds them equal.
use super::args::Cli;
use cargo_atlas::Result;
use clap::{Args, CommandFactory};
use std::{fmt::Write as _, io::Write};

/// The topics, in the order `help topics` lists them: name, one line, text.
pub(crate) const TOPICS: &[(&str, &str, &str)] = &[
    (
        "agent",
        "the briefing for an agent's first session: what answers mean, commands, recipes",
        include_str!("../../docs/agent.md"),
    ),
    (
        "skill",
        "the Agent Skill file: save it as ~/.claude/skills/atlas/SKILL.md (or ~/.agents/skills/atlas/SKILL.md)",
        include_str!("../../skills/atlas/SKILL.md"),
    ),
    (
        "getting-started",
        "a fifteen-minute tour on a tiny example project",
        include_str!("../../docs/getting-started.md"),
    ),
    (
        "concepts",
        "how Atlas reads a project: evidence, scope, population, output",
        include_str!("../../docs/concepts.md"),
    ),
    (
        "query",
        "the query language: rules, the prelude, honesty checks, output",
        include_str!("../../docs/query.md"),
    ),
    (
        "facts",
        "every relation `facts` exports and `query` reads, column by column",
        include_str!("../../docs/facts.md"),
    ),
    (
        "diagrams",
        "every drawing, as text or an image",
        include_str!("../../docs/diagrams.md"),
    ),
    (
        "checks",
        "architecture policies and what delta, policy, stages and seams check",
        include_str!("../../docs/checks.md"),
    ),
    (
        "extensions",
        "adapters, the adapter protocol and custom renderers",
        include_str!("../../docs/extensions.md"),
    ),
    (
        "library",
        "using atlas-lib from Rust",
        include_str!("../../docs/library.md"),
    ),
];

#[derive(Args)]
pub(crate) struct Options {
    /// A command, a topic, or `topics` to list the topics.
    #[arg(value_name = "COMMAND|TOPIC")]
    name: Option<String>,
    /// Print this topic; needed for query and facts, which are also commands.
    #[arg(long, value_name = "TOPIC", conflicts_with = "name")]
    topic: Option<String>,
}

pub(crate) fn run(options: &Options) -> Result<()> {
    let name = match (&options.topic, options.name.as_deref()) {
        (Some(topic), _) => topic.as_str(),
        (None, None) => return as_flag(&[]),
        (None, Some("topics")) => {
            std::io::stdout().lock().write_all(listing().as_bytes())?;
            return Ok(());
        }
        // A command's help comes first: `help X` meaning `X --help` is the
        // convention. A guide of the same name is `--topic X`.
        (None, Some(name)) if Cli::command().find_subcommand(name).is_some() => {
            return as_flag(&[name]);
        }
        (None, Some(name)) => name,
    };
    let text = topic(name).ok_or_else(|| unknown(name))?;
    std::io::stdout().lock().write_all(text.as_bytes())?;
    Ok(())
}
/// Print what `--help` after `words` prints, by asking the parser for it.
fn as_flag(words: &[&str]) -> Result<()> {
    let argv = std::iter::once("cargo-atlas")
        .chain(words.iter().copied())
        .chain(["--help"]);
    match Cli::command().try_get_matches_from(argv) {
        // As clap does for `--help`: a reader that closed the pipe early
        // (`| head`) is not an error.
        Err(help) if help.kind() == clap::error::ErrorKind::DisplayHelp => {
            let _ = help.print();
            Ok(())
        }
        Err(error) => Err(error.into()),
        Ok(_) => Err("--help printed nothing".into()),
    }
}
fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(topic, _, _)| *topic == name)
        .map(|(_, _, text)| *text)
}
fn unknown(name: &str) -> String {
    format!(
        "no command or topic {name:?}; cargo atlas help topics lists the topics, cargo atlas --help the commands"
    )
}
/// `help topics`: one line per topic, and how to read one.
fn listing() -> String {
    let width = TOPICS
        .iter()
        .map(|(name, _, _)| name.len())
        .max()
        .unwrap_or(0);
    let mut out = String::from(
        "Topics (cargo atlas help TOPIC; query and facts are also commands, so: cargo atlas help --topic query):\n",
    );
    for (name, line, _) in TOPICS {
        let _ = writeln!(out, "  {name:width$}  {line}");
    }
    out
}
