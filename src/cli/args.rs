//! Command grammar and compatibility positional roots; operational work belongs to each command.
use super::output::{
    form::{As, formats},
    notice::Notices,
};
use super::subject;
use super::{
    callers, comparison, delta, dependencies, documentation, facts, flow, help, map, navigation,
    orient, overview, policy, program, query, review, stages, types,
};
use cargo_atlas::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    version,
    bin_name = "cargo atlas",
    disable_help_subcommand = true,
    about = "Answer questions about how a Rust codebase is put together",
    before_help = "New here, or an agent? Run `cargo atlas help agent` first: one page on what answers mean, which command to use, and query recipes. Then `cargo atlas orient` in your project.",
    after_help = "Start here: orient (entry points, where the code is, and what Atlas could not resolve)\nAsk by question: what is X, who names X: name · what is in a directory: module · who calls X, what X calls: callers, callees · steps in a function: flow · which files call which: dependencies · what takes or returns a type: types · what changed: diff, moved, visibility · rules: policy, delta · every fact as tables: facts · your own question over the facts: query\n\nFind your way around: orient, census, name, module, callers, callees, dump, facts, query\nReview a change: diff, moved, visibility, compare\nMeasure: spans, concentration, nontest-loc, comments\nDraw diagrams: dependencies, types, flow, overview, program, map\nCheck architecture: delta, stages, policy, seams\n\nCommands read the Cargo project in the current directory, or the one given with --root.\nAdd --as json (or --json) for machine-readable output, or --as svg for an image; flow and map write their JSON evidence beside the drawing instead. Atlas never changes the code it reads.\nGuides: cargo atlas help topics lists them; cargo atlas help TOPIC prints one (also at https://github.com/flowerornament/atlas#documentation)"
)]
pub(super) struct Cli {
    /// Cargo project directory; defaults to the current directory. A project directory as the first bare word still works until 1.0.
    #[arg(long = "root", global = true, value_name = "PATH")]
    project_root: Option<PathBuf>,
    /// Read every file afresh, without the syntax-facts cache (same as ATLAS_CACHE=off).
    #[arg(long, global = true)]
    pub(super) no_cache: bool,
    #[command(subcommand)]
    pub(super) command: Command,
}
impl Cli {
    /// The project directory: `--root`, or a positional ROOT the bare words
    /// hold (spec §3.2.2). Reads every command's bare words into its subject.
    pub(super) fn selected_root(&mut self, notices: &mut Notices) -> Result<PathBuf> {
        let positional = self.command.read_bare(&mut subject::Reader {
            command: self.command.name(),
            named_root: self.project_root.as_deref(),
            notices,
        })?;
        if let (Some(named), Some(positional)) =
            (self.project_root.as_deref(), positional.as_deref())
            && named.canonicalize()? != positional.canonicalize()?
        {
            return Err(
                "positional ROOT conflicts with --root; choose one project directory".into(),
            );
        }
        Ok(self
            .project_root
            .clone()
            .or(positional)
            .unwrap_or_else(|| PathBuf::from(".")))
    }
}
/// The word that named the command, as written (an alias stays an alias),
/// skipping the global options that may come before it.
pub(super) fn command_word(args: &[std::ffi::OsString]) -> Option<&std::ffi::OsStr> {
    let mut words = args.iter().skip(1);
    while let Some(word) = words.next() {
        if word == "--root" {
            words.next();
        } else if !word.to_string_lossy().starts_with('-') {
            return Some(word);
        }
    }
    None
}
/// Whether the first argument equal to `word` is the value of a flag.
pub(super) fn follows_flag(word: &str) -> bool {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == word)
        .and_then(|at| at.checked_sub(1))
        .is_some_and(|before| args[before].starts_with('-'))
}
/// This run's command line, as `cargo atlas ...`, with the first argument
/// equal to `from` replaced by `to`; `None` when no argument is `from`.
pub(super) fn this_command_with(from: &str, to: &str) -> Option<String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "atlas") {
        args.remove(0);
    }
    let at = args.iter().position(|arg| arg == from)?;
    to.clone_into(&mut args[at]);
    Some(format!("cargo atlas {}", args.join(" ")))
}
#[derive(Subcommand)]
pub(super) enum Command {
    /// Start here: entry points, where the code is, and what Atlas could not resolve.
    #[command(
        after_help = "Answers: what is this project, where is its code, and how far can Atlas resolve it?\nOne page of measured lists. Each states its unit, its order and its total, and names the command that shows the rest; tied rows are reported, never picked. It ends with commands to run next, built from the names it found.\nEvidence: Cargo targets, non-test lines, resolved calls (a lower bound) and lexical type mentions. Nothing is ranked by importance.\nExample: cargo atlas orient src --rows 8"
    )]
    Orient(orient::Options),
    /// Count the files and declarations in a project.
    #[command(
        after_help = "Answers: what does this project contain?\nCounts: files and declarations, by kind. Without --population, every declaration Atlas found; with it, only items of that visibility outside test modules.\nEvidence: names written in the source (lexical). Counts are not proof that code is used.\nExample: cargo atlas census src --population pub-ish"
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
    /// List the functions that call a function, from resolved calls.
    #[command(
        after_help = "Answers: which functions call this one?\nResolved: each call Atlas followed through use statements and modules, with its site. Possibly: calls Atlas could not resolve that are written with the same name, counted by why they stayed unresolved; they are never counted as callers.\nAccepts: FILE:LINE, a declaration id, Type::method, module::name or a name (each match is answered on its own). --in or -p keeps callers in part of the project.\nNo resolved caller is not proof that nothing calls it: calls inside macros, or through a function passed elsewhere as a value, are not recorded.\nExample: cargo atlas callers parse"
    )]
    Callers(callers::Options),
    /// List the functions a function calls, from resolved calls.
    #[command(
        after_help = "Answers: what does this function call?\nResolved: each call Atlas followed to a workspace function. The rest are external or unknown, not absent: they are counted by why they stayed unresolved.\nAccepts the same functions as callers. --in or -p keeps called functions in part of the project.\nExample: cargo atlas callees main"
    )]
    Callees(callers::Options),
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
    #[command(mut_arg("as", |arg| arg.value_parser(formats(&[As::Json]))))]
    Dump(navigation::DumpArgs),
    /// Export what Atlas found as flat tables (relations), for your own queries.
    #[command(
        after_help = "Answers: what did Atlas observe, as tables a tool can join?\nRelations: lexical (files, declarations, mentions and their candidates) and qualified (functions, types, calls, references), each with documented columns. An ambiguous mention has one row per candidate with a basis column; an unresolved call or type is a row in its unknowns relation, never left out.\n--schema prints every relation and column without reading the source. See cargo atlas help --topic facts.\nExample: cargo atlas facts --as jsonl --out target/atlas/facts.jsonl"
    )]
    Facts(facts::Options),
    /// Ask your own question over the facts, in Datalog.
    #[command(
        after_help = query::after_help()
    )]
    #[command(mut_arg("as", |arg| arg.value_parser(formats(&[As::Text, As::Json, As::Jsonl]))))]
    Query(query::Options),
    /// Compare file sizes and shapes, or declarations, with an earlier commit.
    #[command(
        after_help = "Answers: how did each file's size and shape change since REVISION?\nWith --items: which declarations were added, removed or changed. Add --graph for their mentions.\nCounts: code outside test modules. Removed files are included.\nEvidence: source lines and declarations; not proof that behavior is the same.\nExample: cargo atlas diff HEAD~1 --in src --items"
    )]
    Diff(review::DiffOptions),
    /// List item bodies that are unchanged, moved or changed since an earlier commit.
    #[command(
        after_help = "Answers: which item bodies stayed the same, moved, or changed since REVISION?\nExact: same tokens, ignoring docs, visibility and layout. Rerooted: the same once module paths such as crate:: and super:: are ignored.\nItems are matched by package, kind and name; repeated names are matched approximately. This does not prove behavior is the same.\nExample: cargo atlas moved HEAD~1 --in src"
    )]
    Moved(review::Options),
    /// List items that became more or less public since an earlier commit.
    #[command(
        after_help = "Answers: which items changed their written visibility (pub, pub(crate), private) since REVISION?\nItems are matched by package, kind and name; repeated names are matched approximately.\nThis compares what is written, not Rust's effective visibility or API compatibility.\nExample: cargo atlas visibility HEAD~1 --in src"
    )]
    Visibility(review::Options),
    /// Show where each item starts and ends, and any code outside items.
    #[command(
        after_help = "Answers: which lines does each item cover, and which code lies outside any item?\nIncludes private imports and test items. Macros are not expanded.\nExample: cargo atlas spans src"
    )]
    Spans(review::SourceReviewArgs),
    /// Measure each file's largest block and number of items.
    #[command(
        after_help = "Answers: how big is each file's largest top-level block, and how many top-level items does it have?\nA block runs from one top-level declaration to the next. Code in test modules is left out.\nThis is a measurement, not a quality score.\nExample: cargo atlas concentration src"
    )]
    Concentration(review::SourceReviewArgs),
    /// Count lines of code outside test modules.
    #[command(
        after_help = "Answers: how many lines, and how many lines with code, are outside test modules?\nA #[cfg(test)] module is left out with its attributes. A single #[cfg(test)] function outside a test module is still counted.\nExample: cargo atlas nontest-loc src"
    )]
    NontestLoc(review::SourceReviewArgs),
    /// Report missing documentation comments.
    #[command(
        after_help = "Answers: which files lack a //! header, which headers cite nothing, and which items lack a /// comment?\n--citation-markers lists the strings that count as a citation (such as docs/ or RFC-); without it, citations are not checked.\nReads //! and /// comments only, not #[doc] attributes or block comments. No pass or fail.\nExample: cargo atlas comments src --population pub-ish"
    )]
    Comments(documentation::Options),
    /// Draw which files call each other or use each other's types.
    #[command(
        after_help = "Answers: which files are linked by function calls, types in signatures, or types in fields?\nChoose with --relation calls|signatures|fields|all.\nEvidence: qualified (names followed through use statements and modules). Calls Atlas cannot resolve are counted and kept in the JSON, never treated as absent.\nExample: cargo atlas dependencies --relation calls --as svg"
    )]
    Dependencies(dependencies::Options),
    /// Draw which functions take, return and build selected types.
    #[command(
        alias = "representations",
        after_help = "Answers: for the selected types, which functions take them, return them and build them, and which types hold them in fields?\nName the types (Tree,Checked); a name several types share takes module::Name, FILE:Name or --in PATH, and is refused with each one's spelling. Or give a saved question (--query). Choose a view with --panel.\nEvidence: qualified signatures, fields and construction sites; not how values move at run time.\nExample: cargo atlas types --query pipeline.json --panel all --as svg"
    )]
    Types(types::Options),
    /// Show the steps and branches inside one function.
    #[command(
        after_help = "Answers: what steps, branches, loops and returns does this function's code contain?\n--depth N also opens the functions it calls, up to N levels (max 6). Macros, closures and unsupported syntax are shown as opaque steps.\nThis shows what the code's structure allows; it does not prove a path can run.\nExample: cargo atlas flow --path src/main.rs --function main --depth 1"
    )]
    Flow(flow::Options),
    /// Compare selected types and their functions between two commits.
    #[command(
        after_help = "Answers: how did the selected types' signatures, fields and construction sites change between two versions?\nCompares --vs REV with your current files, or with --at REV. Select types with a saved question (--query).\nItems are matched by file and name; a rename shows as a removal and an addition.\nExample: cargo atlas compare --vs HEAD~1 --query pipeline.json"
    )]
    Compare(comparison::Options),
    /// Draw the stages a project declares in its code, using an adapter.
    #[command(
        after_help = "Answers: how do the stages a project declares (for example in a const PIPELINE table) hand types to each other?\n--adapter names a program that reads the project's declarations; see cargo atlas help extensions. Atlas checks its answer against the source.\nEvidence: qualified signatures, organized by the declared stages; not execution order.\nExample: cargo atlas overview --adapter ./my-adapter --as svg --out target/atlas/overview.svg"
    )]
    Overview(overview::Options),
    /// Follow entry functions across the groups in an architecture profile (older view).
    #[command(
        after_help = "Answers: how do selected entry functions cross the groups declared in an architecture profile?\nEvidence: written calls, grouped by the profile; not proof of what runs.\nExample: cargo atlas program --architecture architecture.json"
    )]
    #[command(mut_arg("as", |arg| {
        arg.value_parser(formats(&[As::Text, As::Json, As::Svg, As::Html]))
            .help("`text` draws in the terminal; `json` prints the program and overview; `svg` draws an image and `html` a page with the image and its evidence (both need Node.js 20 or newer)")
    }))]
    Program(program::Options),
    /// Draw declared groups, or approximate type mentions between files (older view).
    #[command(
        after_help = "Answers: how do the groups in an architecture profile connect, or which files mention each other's types?\nEvidence: declared permissions, or name matching (lexical). Neither shows what happens at run time. For exact file relationships, use dependencies.\nExample: cargo atlas map --in src"
    )]
    Map(map::Options),
    /// Check whether a change breaks the architecture policy.
    #[command(
        after_help = "Answers: does the candidate commit break a rule in the policy, or make a measured number worse than the baseline REFERENCE?\nVerdicts: PASS, REFUSE (a rule is broken) or UNAVAILABLE (evidence is missing). REFUSE and UNAVAILABLE exit with an error; with --json, REFUSE is data and exits successfully.\nThe policy must be committed. See cargo atlas help checks.\nExample: cargo atlas delta HEAD~1 --policy .config/architecture.toml --citation-markers docs/"
    )]
    #[command(mut_arg("json", |arg| arg.help("Print the report as JSON. A REFUSE verdict is then data and exits successfully; UNAVAILABLE still fails.")))]
    #[command(mut_arg("bare", |arg| {
        arg.value_name("REFERENCE")
            .help("The published revision whose policy and source form the baseline. --vs REV is the same.")
    }))]
    Delta(delta::Options),
    /// Report code that uses stages out of the declared order.
    #[command(
        after_help = "Answers: which paths in each stage use a later stage, or an earlier stage the policy does not allow?\nA report only: it always exits successfully. Use delta to gate changes.\nEvidence: written paths and use statements, not run-time calls.\nExample: cargo atlas stages --policy .config/architecture.toml --scope tutorial"
    )]
    Stages(stages::Options),
    /// Check that an architecture policy is valid and matches the code.
    #[command(
        after_help = "Answers: is the policy file valid, and do its declared homes and limits match the code?\nVerdicts: PASS, REFUSE or UNAVAILABLE. Rules that need project-specific information report UNAVAILABLE here; see cargo atlas help checks.\nThis does not compare commits; use delta for that.\nExample: cargo atlas policy --policy .config/architecture.toml"
    )]
    Policy(policy::Options),
    /// Report items and tests that cross stage boundaries (needs a project wrapper).
    #[command(
        after_help = "Answers: which items cross stage boundaries, who builds shared types, and which tests reach into other stages?\nStage boundaries are project-specific, so without a project wrapper this reports UNAVAILABLE. See cargo atlas help checks.\nExample: cargo atlas seams --policy .config/architecture.toml --items --json"
    )]
    Seams(stages::SeamOptions),
    /// Show a command's help, or a guide: cargo atlas help topics lists them.
    Help(help::Options),
}
impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Orient(_) => "orient",
            Self::Census(_) => "census",
            Self::Name(_) => "name",
            Self::Module(_) => "module",
            Self::Callers(_) => "callers",
            Self::Callees(_) => "callees",
            Self::Related(_) => "related",
            Self::Dump(_) => "dump",
            Self::Facts(_) => "facts",
            Self::Query(_) => "query",
            Self::Diff(_) => "diff",
            Self::Moved(_) => "moved",
            Self::Visibility(_) => "visibility",
            Self::Spans(_) => "spans",
            Self::Concentration(_) => "concentration",
            Self::NontestLoc(_) => "nontest-loc",
            Self::Comments(_) => "comments",
            Self::Dependencies(_) => "dependencies",
            Self::Types(_) => "types",
            Self::Flow(_) => "flow",
            Self::Compare(_) => "compare",
            Self::Overview(_) => "overview",
            Self::Program(_) => "program",
            Self::Map(_) => "map",
            Self::Delta(_) => "delta",
            Self::Stages(_) => "stages",
            Self::Policy(_) => "policy",
            Self::Seams(_) => "seams",
            Self::Help(_) => "help",
        }
    }
    /// Read the bare words into the command's subject, returning the
    /// positional ROOT among them, if any.
    fn read_bare(&mut self, reader: &mut subject::Reader<'_>) -> Result<Option<PathBuf>> {
        match self {
            Self::Orient(a) => a.scope.read(reader),
            Self::Census(a) => a.scope.read(reader),
            Self::Spans(a) | Self::Concentration(a) | Self::NontestLoc(a) => a.scope.read(reader),
            Self::Comments(a) => a.scope.read(reader),
            Self::Dependencies(a) => a.scope.read(reader),
            Self::Name(a) => a.name.read(reader),
            Self::Module(a) => a.name.read(reader),
            Self::Related(a) => a.name.read(reader),
            Self::Types(a) => {
                let flagged = !a.types.is_empty() || a.query.is_some();
                a.words.read(reader, flagged)
            }
            Self::Dump(a) => a.root.read(reader),
            Self::Compare(a) => a.words.read(reader),
            Self::Overview(a) => a.words.read(reader),
            Self::Program(a) => a.words.read(reader),
            Self::Flow(a) => {
                let root = a.words.read_anchor(reader, a.architecture.is_some())?;
                if a.words.subject().is_some() && a.named_anchor.is_some() {
                    return Err("give the anchor once: a bare ANCHOR or --anchor".into());
                }
                Ok(root)
            }
            Self::Map(a) => {
                let root = a.words.read_focus(reader)?;
                if a.words.subject().is_some() && (a.in_path.is_some() || a.scope.is_some()) {
                    return Err("give the scope once: a bare argument or --in".into());
                }
                Ok(root)
            }
            Self::Diff(a) => a.comparison.revision.read().map(|()| None),
            Self::Moved(a) | Self::Visibility(a) => a.revision.read().map(|()| None),
            Self::Delta(a) => a.revision.read().map(|()| None),
            Self::Facts(_)
            | Self::Callers(_)
            | Self::Callees(_)
            | Self::Query(_)
            | Self::Stages(_)
            | Self::Policy(_)
            | Self::Seams(_)
            | Self::Help(_) => Ok(None),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cargo_atlas::diagram::{Level, View};
    use std::path::Path;
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
    /// Parse `args` and read their bare words, as `run` does.
    fn read(args: &[&str]) -> Result<(Command, PathBuf)> {
        let mut cli = Cli::try_parse_from(args)?;
        let root = cli.selected_root(&mut Notices::default())?;
        Ok((cli.command, root))
    }
    #[test]
    fn named_root_does_not_consume_a_required_name() {
        let (command, root) = read(&["atlas", "name", "--root", ".", "Graph"]).expect("name");
        assert_eq!(root, Path::new("."));
        let Command::Name(args) = command else {
            panic!("name command")
        };
        assert_eq!(args.name.subject(), "Graph");
        let (command, _) = read(&["atlas", "--root", ".", "module", "src"]).expect("module");
        let Command::Module(args) = command else {
            panic!("module command")
        };
        assert_eq!(args.name.subject(), "src");
    }
    #[test]
    fn named_selectors_keep_the_same_question_without_positional_ambiguity() {
        let (command, _) = read(&["atlas", "map", "--root", ".", "--scope", "admit"]).expect("map");
        let Command::Map(args) = command else {
            panic!("map command")
        };
        assert_eq!(args.query().focus.as_deref(), Some("admit"));
        let (_, root) =
            read(&["atlas", "census", "--root", ".", "--scope", "src"]).expect("census");
        assert_eq!(root, Path::new("."));
        assert!(read(&["atlas", "census", ".", "src", "--scope", "src"]).is_err());
        assert!(read(&["atlas", "flow", ".", "one", "--anchor", "two"]).is_err());
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
            let Command::Map(map) = read(&args).unwrap().0 else {
                panic!()
            };
            assert_eq!((map.query().level, map.query().view), expected);
        }
        assert!(Cli::try_parse_from(["atlas", "map", ".", "--detail", "files"]).is_err());
        assert!(Cli::try_parse_from(["atlas", "flow", ".", "--path", "src/lib.rs"]).is_err());
    }
}
