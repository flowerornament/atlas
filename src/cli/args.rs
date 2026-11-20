//! Command grammar and compatibility positional roots; operational work belongs to each command.
use super::{
    comparison, delta, dependencies, documentation, facts, flow, map, navigation, orient, overview,
    policy, program, review, stages, types,
};
use cargo_atlas::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
#[derive(Parser)]
#[command(
    version,
    bin_name = "cargo atlas",
    about = "Answer questions about how a Rust codebase is put together",
    after_help = "Start here: orient (entry points, where the code is, and what Atlas could not resolve)\nAsk by question: what is X, who names X: name · what is in a directory: module · steps in a function: flow · who calls whom: dependencies · what takes or returns a type: types · what changed: diff, moved, visibility · rules: policy, delta · every fact as tables: facts\n\nFind your way around: orient, census, name, module, dump, facts\nReview a change: diff, moved, visibility, compare\nMeasure: spans, concentration, nontest-loc, comments\nDraw diagrams: dependencies, types, flow, overview, program, map\nCheck architecture: delta, stages, policy, seams\n\nCommands read the Cargo project in the current directory, or the one given with --root.\nAdd --json for machine-readable output; flow and map write their JSON evidence beside the drawing instead. Atlas never changes the code it reads.\nGuides: https://github.com/flowerornament/atlas#documentation"
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
        let given = self.command.root();
        let positional = given.map(|(_, root)| root);
        if let (Some(named), Some(positional)) = (self.project_root.as_deref(), positional)
            && named.canonicalize()? != positional.canonicalize()?
        {
            return Err(
                "positional ROOT conflicts with --root; choose one project directory".into(),
            );
        }
        if let (None, Some((command, root))) = (self.project_root.as_deref(), given) {
            let shown = root.display();
            // A directory without a manifest is part of the enclosing
            // project, which Cargo would read instead: that is a scope.
            if root.is_dir() && !root.join("Cargo.toml").is_file() {
                return Err(format!(
                    "{shown} is not a Cargo project (it has no Cargo.toml), so it cannot be the project root; to read part of this project, run: {}",
                    scoped_command(command, root)
                )
                .into());
            }
            eprintln!(
                "atlas: reading {shown} as the project root; a positional ROOT is deprecated, so use --root {shown} (or --scope for part of a project)"
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
/// This run's command line with the positional `root` as `--scope`.
fn scoped_command(command: &str, root: &Path) -> String {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "atlas") {
        args.remove(0);
    }
    let root = root.to_string_lossy();
    match args.iter().position(|arg| *arg == root) {
        Some(at) => {
            args[at] = format!("--scope {root}");
            format!("cargo atlas {}", args.join(" "))
        }
        None => format!("cargo atlas {command} --scope {root}"),
    }
}
#[derive(Subcommand)]
pub(super) enum Command {
    /// Start here: entry points, where the code is, and what Atlas could not resolve.
    #[command(
        after_help = "Answers: what is this project, where is its code, and how far can Atlas resolve it?\nOne page of measured lists. Each states its unit, its order and its total, and names the command that shows the rest; tied rows are reported, never picked. It ends with commands to run next, built from the names it found.\nEvidence: Cargo targets, non-test lines, resolved calls (a lower bound) and lexical type mentions. Nothing is ranked by importance.\nExample: cargo atlas orient --scope src --rows 8"
    )]
    Orient(orient::Options),
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
    /// Export what Atlas found as flat tables (relations), for your own queries.
    #[command(
        after_help = "Answers: what did Atlas observe, as tables a tool can join?\nRelations: lexical (files, declarations, mentions and their candidates) and qualified (functions, types, calls, references), each with documented columns. An ambiguous mention has one row per candidate with a basis column; an unresolved call or type is a row in its unknowns relation, never left out.\n--schema prints every relation and column without reading the source. See docs/facts.md.\nExample: cargo atlas facts --format jsonl --out target/atlas/facts.jsonl"
    )]
    Facts(facts::Options),
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
        after_help = "Answers: which files lack a //! header, which headers cite nothing, and which items lack a /// comment?\n--citation-markers lists the strings that count as a citation (such as docs/ or RFC-); without it, citations are not checked.\nReads //! and /// comments only, not #[doc] attributes or block comments. No pass or fail.\nExample: cargo atlas comments --scope src --population pub-ish"
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
    /// The positional ROOT, with the command's name for messages about it.
    pub(super) fn root(&self) -> Option<(&'static str, &Path)> {
        fn named<'a>(
            name: &'static str,
            root: Option<&'a PathBuf>,
        ) -> Option<(&'static str, &'a Path)> {
            root.map(|root| (name, root.as_path()))
        }
        match self {
            Self::Types(a) => named("types", a.root.as_ref()),
            Self::Dependencies(a) => named("dependencies", a.root.as_ref()),
            Self::Overview(a) => named("overview", a.root.as_ref()),
            Self::Compare(a) => named("compare", a.root.as_ref()),
            Self::Program(a) => named("program", a.root.as_ref()),
            Self::Map(a) => named("map", a.root.as_ref()),
            Self::Flow(a) => named("flow", a.root.as_ref()),
            Self::Census(a) => named("census", a.root.as_ref()),
            Self::Name(a) => named("name", a.root.as_ref()),
            Self::Module(a) => named("module", a.root.as_ref()),
            Self::Related(a) => named("related", a.root.as_ref()),
            Self::Dump(a) => named("dump", a.root.as_ref()),
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
