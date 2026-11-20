//! Command grammar and compatibility positional roots; operational work belongs to each command.
use super::{
    comparison, delta, dependencies, documentation, flow, map, navigation, overview, policy,
    program, review, stages, types,
};
use cargo_atlas::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
#[derive(Parser)]
#[command(
    version,
    bin_name = "cargo atlas",
    about = "Answer questions about how a Rust codebase is put together",
    after_help = "Find your way around: census, name, module, dump\nReview a change: diff, moved, visibility, compare\nMeasure: spans, concentration, nontest-loc, comments\nDraw diagrams: dependencies, types, flow, overview, program, map\nCheck architecture: delta, stages, policy, seams\n\nCommands read the Cargo project in the current directory, or the one given with --root.\nAdd --json for machine-readable output; flow and map write their JSON evidence beside the drawing instead. Atlas never changes the code it reads.\nGuides: https://github.com/flowerornament/atlas#documentation"
)]
pub(super) struct Cli {
    /// Cargo project directory; defaults to the current directory. Positional ROOT remains supported.
    #[arg(long = "root", global = true, value_name = "PATH")]
    project_root: Option<PathBuf>,
    /// Read every file afresh, without the syntax-facts cache (same as ATLAS_CACHE=off).
    #[arg(long, global = true)]
    pub(super) no_cache: bool,
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
    /// Count the files and declarations in a project.
    #[command(
        after_help = "Answers: what does this project contain?\nCounts: files and declarations, by kind. Without --population, every declaration Atlas found; with it, only items of that visibility outside test modules.\nEvidence: names written in the source (lexical). Counts are not proof that code is used.\nExample: cargo atlas census --scope src --population pub-ish"
    )]
    Census(navigation::ScopeArgs),
    /// Find where a name is declared, and what mentions it.
    #[command(
        after_help = "Answers: where is this name declared, and what mentions it?\nAccepts: a NAME; MODULE::NAME, where the modules are the end of its module path (file directories and name, then inline mod blocks), such as parse::Tree; or FILE:LINE, such as src/parse.rs:4. Both populations read it the same way. When a selector matches several declarations, all of them are shown.\nEvidence: names written in the source (lexical). Mentions are approximate, not resolved like the compiler does.\nExample: cargo atlas name Tree --relations"
    )]
    Name(navigation::RelatedNameArgs),
    /// List what a file, directory or package contains.
    #[command(
        after_help = "Answers: what is in this file, directory or package, and what mentions it?\nAccepts: a path or a package name. --items lists each declaration; --graph adds the mentions between them.\nEvidence: declarations, imports and approximate mentions (lexical).\nExample: cargo atlas module src/lib.rs --items --graph"
    )]
    Module(navigation::ModuleArgs),
    /// Compatibility spelling of name --relations (approximate lexical evidence).
    #[command(hide = true)]
    #[command(
        after_help = "Compatibility alias for name --relations. Population and evidence follow the lexical name query; no runtime reachability is established. Scope: written NAME. Example: cargo atlas name Graph --relations --json"
    )]
    Related(navigation::NameArgs),
    /// Print everything Atlas found in the source, as JSON.
    #[command(
        after_help = "Answers: what are all the declarations, re-exports and mentions Atlas found?\nUse it to build your own tools on Atlas's output. Ambiguous and unresolved names are kept.\nEvidence: names written in the source (lexical); not a compiler symbol table.\nExample: cargo atlas dump --population all --json"
    )]
    Dump(navigation::DumpArgs),
    /// Compare file sizes and shapes, or declarations, with an earlier commit.
    #[command(
        after_help = "Answers: how did each file's size and shape change since REVISION?\nWith --items: which declarations were added, removed or changed. Add --graph for their mentions.\nCounts: code outside test modules. Removed files are included.\nEvidence: source lines and declarations; not proof that behavior is the same.\nExample: cargo atlas diff HEAD~1 --scope src --items"
    )]
    Diff(review::DiffOptions),
    /// List item bodies that are unchanged, moved or changed since an earlier commit.
    #[command(
        after_help = "Answers: which item bodies stayed the same, moved, or changed since REVISION?\nExact: same tokens, ignoring docs, visibility and layout. Rerooted: the same once module paths such as crate:: and super:: are ignored.\nItems are matched by package, kind and name; repeated names are matched approximately. This does not prove behavior is the same.\nExample: cargo atlas moved HEAD~1 --scope src"
    )]
    Moved(review::Options),
    /// List items that became more or less public since an earlier commit.
    #[command(
        after_help = "Answers: which items changed their written visibility (pub, pub(crate), private) since REVISION?\nItems are matched by package, kind and name; repeated names are matched approximately.\nThis compares what is written, not Rust's effective visibility or API compatibility.\nExample: cargo atlas visibility HEAD~1 --scope src"
    )]
    Visibility(review::Options),
    /// Show where each item starts and ends, and any code outside items.
    #[command(
        after_help = "Answers: which lines does each item cover, and which code lies outside any item?\nIncludes private imports and test items. Macros are not expanded.\nExample: cargo atlas spans --scope src"
    )]
    Spans(review::SourceReviewArgs),
    /// Measure each file's largest block and number of items.
    #[command(
        after_help = "Answers: how big is each file's largest top-level block, and how many top-level items does it have?\nA block runs from one top-level declaration to the next. Code in test modules is left out.\nThis is a measurement, not a quality score.\nExample: cargo atlas concentration --scope src"
    )]
    Concentration(review::SourceReviewArgs),
    /// Count lines of code outside test modules.
    #[command(
        after_help = "Answers: how many lines, and how many lines with code, are outside test modules?\nA #[cfg(test)] module is left out with its attributes. A single #[cfg(test)] function outside a test module is still counted.\nExample: cargo atlas nontest-loc --scope src"
    )]
    NontestLoc(review::SourceReviewArgs),
    /// Report missing documentation comments.
    #[command(
        after_help = "Answers: which files lack a //! header, which headers cite nothing, and which items lack a /// comment?\n--citation-markers lists the strings that count as a citation (such as docs/ or RFC-); without it, citations are not checked.\nReads //! and /// comments only, not #[doc] attributes or block comments. No pass or fail.\nExample: cargo atlas comments --audit --scope src --population pub-ish"
    )]
    Comments(documentation::Options),
    /// Draw which files call each other or use each other's types.
    #[command(
        after_help = "Answers: which files are linked by function calls, types in signatures, or types in fields?\nChoose with --relation calls|signatures|fields|all.\nEvidence: qualified (names followed through use statements and modules). Calls Atlas cannot resolve are counted and kept in the JSON, never treated as absent.\nExample: cargo atlas dependencies --relation calls --backend pele"
    )]
    Dependencies(dependencies::Options),
    /// Draw which functions take, return and build selected types.
    #[command(
        alias = "representations",
        after_help = "Answers: for the selected types, which functions take them, return them and build them, and which types hold them in fields?\nSelect types with --type or a saved question (--query). Choose a view with --panel.\nEvidence: qualified signatures, fields and construction sites; not how values move at run time.\nExample: cargo atlas types --query examples/queries/atlas.json --panel all --backend pele"
    )]
    Types(types::Options),
    /// Show the steps and branches inside one function.
    #[command(
        after_help = "Answers: what steps, branches, loops and returns does this function's code contain?\n--depth N also opens the functions it calls, up to N levels (max 6). Macros, closures and unsupported syntax are shown as opaque steps.\nThis shows what the code's structure allows; it does not prove a path can run.\nExample: cargo atlas flow --path src/main.rs --function main --depth 1"
    )]
    Flow(flow::Options),
    /// Compare selected types and their functions between two commits.
    #[command(
        after_help = "Answers: how did the selected types' signatures, fields and construction sites change between two versions?\nCompares --before with your current files, or with --after. Select types with a saved question (--query).\nItems are matched by file and name; a rename shows as a removal and an addition.\nExample: cargo atlas compare --before HEAD~1 --query pipeline.json"
    )]
    Compare(comparison::Options),
    /// Draw the stages a project declares in its code, using an adapter.
    #[command(
        after_help = "Answers: how do the stages a project declares (for example in a const PIPELINE table) hand types to each other?\n--adapter names a program that reads the project's declarations; see docs/extensions.md. Atlas checks its answer against the source.\nEvidence: qualified signatures, organized by the declared stages; not execution order.\nExample: cargo atlas overview --adapter ./my-adapter --backend pele --out target/atlas/overview.svg"
    )]
    Overview(overview::Options),
    /// Follow entry functions across the groups in an architecture profile (older view).
    #[command(
        after_help = "Answers: how do selected entry functions cross the groups declared in an architecture profile?\nEvidence: written calls, grouped by the profile; not proof of what runs.\nExample: cargo atlas program --architecture architecture.json"
    )]
    Program(program::Options),
    /// Draw declared groups, or approximate type mentions between files (older view).
    #[command(
        after_help = "Answers: how do the groups in an architecture profile connect, or which files mention each other's types?\nEvidence: declared permissions, or name matching (lexical). Neither shows what happens at run time. For exact file relationships, use dependencies.\nExample: cargo atlas map --scope src"
    )]
    Map(map::Options),
    /// Check whether a change breaks the architecture policy.
    #[command(
        after_help = "Answers: does the candidate commit break a rule in the policy, or make a measured number worse than the baseline REFERENCE?\nVerdicts: PASS, REFUSE (a rule is broken) or UNAVAILABLE (evidence is missing). REFUSE and UNAVAILABLE exit with an error; with --json, REFUSE is data and exits successfully.\nThe policy must be committed. See docs/checks.md.\nExample: cargo atlas delta HEAD~1 --policy .config/architecture.toml --citation-markers docs/"
    )]
    Delta(delta::Options),
    /// Report code that uses stages out of the declared order.
    #[command(
        after_help = "Answers: which paths in each stage use a later stage, or an earlier stage the policy does not allow?\nA report only: it always exits successfully. Use delta to gate changes.\nEvidence: written paths and use statements, not run-time calls.\nExample: cargo atlas stages --policy .config/architecture.toml --scope my-package"
    )]
    Stages(stages::Options),
    /// Check that an architecture policy is valid and matches the code.
    #[command(
        after_help = "Answers: is the policy file valid, and do its declared homes and limits match the code?\nVerdicts: PASS, REFUSE or UNAVAILABLE. Rules that need project-specific information report UNAVAILABLE here; see docs/checks.md.\nThis does not compare commits; use delta for that.\nExample: cargo atlas policy --policy .config/architecture.toml"
    )]
    Policy(policy::Options),
    /// Report items and tests that cross stage boundaries (needs a project wrapper).
    #[command(
        after_help = "Answers: which items cross stage boundaries, who builds shared types, and which tests reach into other stages?\nStage boundaries are project-specific, so without a project wrapper this reports UNAVAILABLE. See docs/checks.md.\nExample: cargo atlas seams --policy .config/architecture.toml --items --json"
    )]
    Seams(stages::SeamOptions),
}
impl Command {
    pub(super) fn root(&self) -> Option<&Path> {
        match self {
            Self::Types(a) => a.root.as_deref(),
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
    use cargo_atlas::diagram::{Level, View};
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
