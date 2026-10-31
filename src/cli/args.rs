//! Command grammar and compatibility positional roots; operational work belongs to each command.
use super::{
    comparison, delta, dependencies, documentation, flow, map, navigation, overview, policy,
    program, representations, review, stages,
};
use atlas::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
#[derive(Parser)]
#[command(
    version,
    about = "Source-backed Rust navigation, workflows, and architecture maps",
    after_help = "Navigation: census, name (optional --relations), module, dump.\nReview: spans, nontest-loc, concentration, diff, moved, visibility, comments.\nChecks: policy, delta, stages, seams (explicit policy required).\nQualified diagrams: overview, representations, dependencies, compare.\nSource-body syntax: flow. Declared/lexical summaries: program, map.\nAll commands accept --root PATH (default: current directory); positional ROOT remains compatible.\nUse --scope where available to select source separately from the project root.\nLexical co-mentions do not establish qualified uses or runtime order."
)]
pub(super) struct Cli {
    /// Cargo project directory; defaults to the current directory. Positional ROOT remains supported.
    #[arg(long = "root", global = true, value_name = "PATH")]
    project_root: Option<PathBuf>,
    #[command(subcommand)]
    pub(super) command: Command,
}
impl Cli {
    pub(super) fn selected_root(&self) -> Result<PathBuf> {
        let positional = self.command.root();
        if let (Some(named), Some(positional)) = (self.project_root.as_deref(), positional)
            && named.canonicalize()? != positional.canonicalize()?
        {
            return Err(
                "positional ROOT conflicts with --root; choose one project directory".into(),
            );
        }
        Ok(self
            .project_root
            .as_deref()
            .or(positional)
            .unwrap_or_else(|| Path::new("."))
            .to_owned())
    }
}
#[derive(Subcommand)]
pub(super) enum Command {
    /// Judge architecture debt between pinned baseline and candidate source captures.
    #[command(
        after_help = "Question: what architecture debt changes between two exact revisions? Population: the authored policy selects core packages and rule populations. Evidence: paired source measurements and supplied authority facts; required missing evidence is UNAVAILABLE. Scope: --policy and the pinned revisions. Example: atlas delta HEAD~1 --candidate HEAD --policy .config/architecture.toml --json"
    )]
    Delta(delta::Options),
    /// Read production paths against an explicit stage order and declared interfaces.
    #[command(
        after_help = "Question: which written production paths cross the declared stage interfaces? Population: the policy selects packages and ordered stages. Evidence: lexical paths, not runtime calls or complete policy PASS. Scope: --policy. Example: atlas stages --policy .config/architecture.toml --json"
    )]
    Stages(stages::Options),
    /// Validate authored policy against captured source and required observations.
    #[command(
        after_help = "Question: are authored policy inputs valid against captured source? Population: native source trees with each rule's production/test selection. Evidence: configuration and source observations; this does not judge measured debt or a landing delta. Required missing authority is UNAVAILABLE. Scope: --policy. Example: atlas policy --policy .config/architecture.toml --json"
    )]
    Policy(policy::Options),
    /// Read item seams, writer calls and test coupling; project boundaries required.
    #[command(
        after_help = "Question: where do items, writers and tests cross declared seams? Population: selected source trees, including rule-selected tests and mounted code. Evidence: written paths and project-supplied boundaries; missing authority is UNAVAILABLE. Scope: --policy; --items exposes observations. Example: atlas seams --policy .config/architecture.toml --items --json"
    )]
    Seams(stages::SeamOptions),
    /// Audit documentation with explicit population and caller-supplied citation markers.
    #[command(
        after_help = "Question: which selected declarations or files lack the requested line documentation? Population: --population subjects outside test modules; directly gated helpers remain. Evidence: /// and //! lines, not expanded doc attributes or semantic citation resolution. Scope: --scope. No architecture verdict is assigned. Example: atlas comments --audit --scope src --population pub-ish --json"
    )]
    Comments(documentation::Options),
    /// Compare production module shape: lines, block depth and item breadth; no verdict.
    #[command(
        after_help = "Question: what captured source shape or declarations changed? Population: non-test-module source; --items defaults to all written visibilities, with --population available. Evidence: lexical spans and known incident co-mentions, not behavioral equivalence. Scope: --scope, including removed paths. Inline bodies on the signature-ending line remain captured. Example: atlas diff HEAD~1 --scope src --items --graph --json"
    )]
    Diff(review::DiffOptions),
    /// Compare written visibility by crate/class/name; repeated names remain approximate.
    #[command(
        after_help = "Question: which matched declarations changed their written visibility? Population: production declarations, excluding test-implying gates. Evidence: crate/class/name lexical matching; duplicate keys remain approximate, not proof of API compatibility. Scope: --scope narrows output while matching spans the crate. Example: atlas visibility HEAD~1 --scope src --json"
    )]
    Visibility(review::Options),
    /// Classify normalized token bodies: exact, rerooted, changed or unreadable; no behavior proof.
    #[command(
        after_help = "Question: which declarations kept their normalized body across a move? Population: production declarations, excluding test-implying gates. Evidence: exact tokens or tokens after import rerooting; duplicate keys remain approximate and no behavioral equivalence is claimed. Scope: --scope narrows output while matching spans the crate. Example: atlas moved HEAD~1 --scope src --json"
    )]
    Moved(review::Options),
    /// Show attached lexical item spans and uncovered code; includes private imports.
    #[command(
        after_help = "Question: where are the attached declarations and uncovered code? Population: all captured item spans, including test items; line and shape metrics separately exclude test modules. Evidence: lexical item and source-line boundaries, without macro expansion. Scope: positional PATH or --scope; no match refuses. Example: atlas spans --scope src --json"
    )]
    Spans(review::SourceReviewArgs),
    /// Report native top-level block depth and item breadth, without assigning a score.
    #[command(
        after_help = "Question: how much source lies in each top-level block? Population: all visibilities outside test modules, with directly gated helpers retained. Evidence: native declaration-to-next-declaration block depth and item breadth; not cognitive complexity or a quality verdict. Scope: positional PATH or --scope. Example: atlas concentration --scope src --json"
    )]
    Concentration(review::SourceReviewArgs),
    /// Count lines outside native cfg-test module boundaries.
    #[command(
        after_help = "Question: how many physical and token-bearing lines lie outside test modules? Population: native cfg-test module exclusion, including attached attributes; directly gated helpers remain. Evidence: captured lexical lines, not executed coverage. Scope: positional PATH or --scope. Example: atlas nontest-loc --scope src --json"
    )]
    NontestLoc(review::SourceReviewArgs),
    /// Compare source contracts, constructors and preparation across revisions.
    #[command(
        after_help = "Question: which representation observations changed between revisions? Population: production declarations selected by the optional saved query. Evidence: qualified signature, field and construction observations, not inferred behavior. Scope: --before, optional --after and --query. Example: atlas compare --before HEAD~1 --backend ascii"
    )]
    Compare(comparison::Options),
    /// Derive a default representation picture from MODULES/SURFACES in source.
    #[command(
        after_help = "Question: which representation contracts follow the declared stage surfaces? Population: production declarations reached from MODULES/SURFACES or a supplied declaration adapter. Evidence: signatures and source declarations, not execution order. Scope: selected Cargo root and adapter input. Missing declarations refuse. Example: atlas overview --backend pele --out target/atlas/overview.svg"
    )]
    Overview(overview::Options),
    /// Qualified observed file relationships, with calls/signatures/fields kept distinct.
    #[command(
        after_help = "Question: which files are related by observed calls, signatures or retained fields? Population: captured production source. Evidence: selected qualified relationship kinds with unresolved and ambiguous targets retained; not a complete Rust call graph. Scope: --scope and relation selection. Example: atlas dependencies --relation all --json"
    )]
    Dependencies(dependencies::Options),
    /// Derive representation contracts, direct fields and preparation calls from a saved question.
    #[command(
        after_help = "Question: how are the selected representations produced, retained and prepared? Population: production types and functions selected by --query. Evidence: qualified signatures, direct fields and written construction/call sites; no inferred ownership or freshness. Scope: the saved query. Example: atlas representations --query examples/queries/atlas.json --backend ascii"
    )]
    Representations(representations::Options),
    /// Follow selected entry functions and fold declared stage boundaries.
    #[command(
        after_help = "Question: how do selected entry bodies cross declared boundaries? Population: source functions selected by the architecture profile or --path/--function. Evidence: written calls and profile grouping, not feasible execution paths. Scope: --architecture and entry selection. Example: atlas program --architecture architecture.json --backend ascii"
    )]
    Program(program::Options),
    /// Project declared boundaries or approximate lexical type relationships.
    #[command(
        after_help = "Question: how do declared groups or lexical type mentions connect? Population: selected workspace files and profile groups. Evidence: declared permissions or approximate lexical co-mentions; neither establishes runtime flow. Scope: --scope, --level, --view and optional --architecture. Example: atlas map --scope src --backend ascii"
    )]
    Map(map::Options),
    /// Read coordinating function bodies; source order is not a resolved call graph.
    #[command(
        after_help = "Question: what source-control steps occur in a selected function body? Population: selected written bodies; --depth optionally expands matched source calls. Evidence: syntax order, branches, loops and explicit returns; not feasible paths, macro execution or unwinding. Scope: --path/--function or a declared anchor. Example: atlas flow --path src/main.rs --function main --depth 1 --backend ascii"
    )]
    Flow(flow::Options),
    /// Count physical files, declarations, and lexical evidence.
    #[command(
        after_help = "Question: what files and declarations exist in this source scope? Population: unflagged 0.5 lexical graph, or explicit --population native-style subjects outside test modules; macro and line units are independent. Evidence: captured declarations and lexical counts, not executed or resolved APIs. Scope: --scope. Example: atlas census --scope src --population pub-ish --json"
    )]
    Census(navigation::ScopeArgs),
    /// Find declarations by exact name or source-site ID, keeping ambiguity explicit.
    #[command(
        after_help = "Question: where is this name declared or mentioned? Population: unflagged 0.5 graph, or explicit --population native-style subjects outside test modules. Evidence: written names, lexical candidate bindings and approximate co-mentions, not Rust resolution. Scope: NAME or path::NAME; ambiguity remains explicit. Example: atlas name Graph --population pub-ish --json"
    )]
    Name(navigation::RelatedNameArgs),
    /// Inspect a physical module, directory, or Cargo package.
    #[command(
        after_help = "Question: what is in this file, folder or package and how is it mentioned? Population: unflagged 0.5 graph, or --population/--items/--graph native-style source outside test modules. Evidence: declarations, normalized imports and approximate co-mentions, not resolved reachability. Scope: NAME/path; native mode uses a physical path. Example: atlas module src/lib.rs --population pub-ish --items --graph --json"
    )]
    Module(navigation::ModuleArgs),
    /// Compatibility spelling of name --relations (approximate lexical evidence).
    #[command(hide = true)]
    #[command(
        after_help = "Compatibility alias for name --relations. Population and evidence follow the lexical name query; no runtime reachability is established. Scope: written NAME. Example: atlas name Graph --relations --json"
    )]
    Related(navigation::NameArgs),
    /// Dump the complete source graph as JSON.
    #[command(
        after_help = "Question: what complete source observations can a consumer inspect? Population: unflagged 0.5 graph, or explicit --population native declarations, facade/reexport candidates and co-mentions outside test modules. Evidence: lexical observations with ambiguity and limits; not a compiler symbol table. Scope: selected Cargo root. Example: atlas dump --population all --json"
    )]
    Dump(navigation::DumpArgs),
}
impl Command {
    pub(super) fn root(&self) -> Option<&Path> {
        match self {
            Self::Representations(a) => a.root.as_deref(),
            Self::Dependencies(a) => a.root.as_deref(),
            Self::Overview(a) => a.root.as_deref(),
            Self::Compare(a) => a.root.as_deref(),
            Self::Program(a) => a.root.as_deref(),
            Self::Map(a) => a.root.as_deref(),
            Self::Flow(a) => a.root.as_deref(),
            Self::Census(a) => a.root.as_deref(),
            Self::Name(a) => a.root.as_deref(),
            Self::Module(a) => a.root.as_deref(),
            Self::Related(a) => a.root.as_deref(),
            Self::Dump(a) => a.root.as_deref(),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use atlas::diagram::{Level, View};
    #[test]
    fn operational_commands_advertise_the_shared_root_control() {
        use clap::CommandFactory;
        let mut cli = Cli::command();
        cli.build();
        for command in cli.get_subcommands_mut() {
            if command.get_name() == "help" {
                continue;
            }
            let help = command.render_long_help().to_string();
            assert!(help.contains("--root"), "{}", command.get_name());
        }
    }
    #[test]
    fn named_root_does_not_consume_a_required_name() {
        let cli =
            Cli::try_parse_from(["atlas", "name", "--root", ".", "Graph"]).expect("canonical name");
        assert!(cli.command.root().is_none());
        let Command::Name(args) = cli.command else {
            panic!("name command")
        };
        assert_eq!(args.name, "Graph");
        let cli = Cli::try_parse_from(["atlas", "--root", ".", "module", "src"])
            .expect("canonical module");
        assert!(cli.command.root().is_none());
        let Command::Module(args) = cli.command else {
            panic!("module command")
        };
        assert_eq!(args.name, "src");
    }
    #[test]
    fn named_selectors_keep_the_same_question_without_positional_ambiguity() {
        let cli = Cli::try_parse_from(["atlas", "map", "--root", ".", "--scope", "admit"])
            .expect("canonical map");
        let Command::Map(args) = cli.command else {
            panic!("map command")
        };
        assert_eq!(args.query().focus.as_deref(), Some("admit"));
        let cli = Cli::try_parse_from(["atlas", "census", "--root", ".", "--scope", "src"])
            .expect("canonical census");
        assert!(cli.command.root().is_none());
        assert!(Cli::try_parse_from(["atlas", "census", ".", "src", "--scope", "src"]).is_err());
        assert!(Cli::try_parse_from(["atlas", "flow", ".", "one", "--anchor", "two"]).is_err());
    }
    #[test]
    fn defaults_and_removed_prototype_aliases() {
        for (args, expected) in [
            (vec!["atlas", "map", "."], (Level::Modules, View::Types)),
            (
                vec!["atlas", "map", ".", "--architecture", "a.json"],
                (Level::Stages, View::Dependencies),
            ),
            (
                vec!["atlas", "map", ".", "admit", "--architecture", "a.json"],
                (Level::Modules, View::Types),
            ),
        ] {
            let Command::Map(map) = Cli::try_parse_from(args).unwrap().command else {
                panic!()
            };
            assert_eq!((map.query().level, map.query().view), expected);
        }
        assert!(Cli::try_parse_from(["atlas", "map", ".", "--detail", "files"]).is_err());
        assert!(Cli::try_parse_from(["atlas", "flow", ".", "--path", "src/lib.rs"]).is_err());
    }
}
