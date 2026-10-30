//! Developer navigation, source workflows, and explicitly declared architecture maps.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the CLI writes results and diagnostics at the process boundary"
)]
mod artifact_output;
mod comparison_command;
mod delta_command;
mod dependency_command;
mod documentation_command;
mod navigation_command;
mod output;
mod output_files;
mod overview_command;
mod policy_command;
mod representation_command;
mod review_command;
mod stage_command;
use atlas::{
    Result,
    architecture::{Architecture, WorkflowAnchor},
    diagram::{self, Diagram, Level, Query, View},
    discovery, program, queries,
    render::{Artifact, Ascii, Document, Pele, Renderer},
    workflow,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fmt::Write as _;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("atlas: {error}");
        std::process::exit(1);
    }
}
fn run(cli: Cli) -> Result<()> {
    if matches!(&cli.command, Command::Related(_)) {
        eprintln!(
            "atlas: related is a compatibility alias; use name --relations (lexical co-mentions)."
        );
    }
    let selected_root = cli.selected_root()?;
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(selected_root)
        .no_deps()
        .exec()?;
    let root = metadata.workspace_root.as_std_path().canonicalize()?;
    match cli.command {
        Command::Delta(options) => delta_command::run(&root, &metadata, &options),
        Command::Policy(options) => policy_command::run(&root, &metadata, &options),
        Command::Stages(options) => {
            stage_command::run(&root, &metadata, &options, stage_command::Question::Stages)
        }
        Command::Seams(options) => stage_command::seams(&root, &metadata, &options),
        Command::Comments(options) => documentation_command::run(&metadata, &options),
        Command::Diff(options) => review_command::diff(&root, &metadata, &options),
        Command::Visibility(options) => review_command::run(
            &root,
            &metadata,
            &options,
            review_command::Question::Visibility,
        ),
        Command::Moved(options) => {
            review_command::run(&root, &metadata, &options, review_command::Question::Moved)
        }
        Command::Compare(options) => comparison_command::run(&root, &metadata, &options),
        Command::Overview(options) => overview_command::run(&root, &metadata, options),
        Command::Dependencies(options) => dependency_command::run(&root, &metadata, &options),
        Command::Representations(options) => {
            representation_command::run(&root, &metadata, &options)
        }
        Command::Flow(options) => flow(&root, &metadata, &options),
        Command::Spans(options) => source_review(&metadata, &options, SourceQuestion::Spans),
        Command::NontestLoc(options) => source_review(&metadata, &options, SourceQuestion::Loc),
        Command::Concentration(options) => {
            source_review(&metadata, &options, SourceQuestion::Concentration)
        }
        Command::Program(options) => program(&root, &metadata, &options),
        Command::Map(options) => map(&root, &metadata, &options),
        Command::Dump(args) if args.population.selected().is_some() => navigation_command::run(
            &metadata,
            args.population.selected().ok_or("population absent")?,
            navigation_command::Question::Dump,
            true,
        ),
        Command::Census(args) if args.population.selected().is_some() => navigation_command::run(
            &metadata,
            args.population.selected().ok_or("population absent")?,
            navigation_command::Question::Census(
                args.named_scope.as_deref().or(args.scope.as_deref()),
            ),
            args.json,
        ),
        Command::Name(args) if args.population.selected().is_some() => navigation_command::run(
            &metadata,
            args.population.selected().ok_or("population absent")?,
            navigation_command::Question::Name(&args.name),
            args.json,
        ),
        Command::Module(args)
            if args.population.selected().is_some() || args.items || args.graph =>
        {
            navigation_command::run(
                &metadata,
                args.population
                    .selected()
                    .unwrap_or(atlas_core::population::VisibilitySelection::RestrictedOrPublic),
                navigation_command::Question::Module {
                    path: &args.name,
                    items: args.items,
                    graph: args.graph,
                },
                args.json,
            )
        }
        Command::Related(args) if args.population.selected().is_some() => navigation_command::run(
            &metadata,
            args.population.selected().ok_or("population absent")?,
            navigation_command::Question::Name(&args.name),
            args.json,
        ),
        command => {
            let graph = discovery::scan_metadata(&metadata)?;
            let text = match command {
                Command::Census(args) => {
                    let result = queries::census(
                        &graph,
                        args.named_scope.as_deref().or(args.scope.as_deref()),
                    )?;
                    if args.json {
                        json(&result)?
                    } else {
                        result.text()
                    }
                }
                Command::Name(args) => {
                    let result = queries::name(&graph, &args.name)?;
                    if args.json {
                        json(&result)?
                    } else {
                        result.text(args.relations)
                    }
                }
                Command::Related(args) => {
                    let result = queries::name(&graph, &args.name)?;
                    if args.json {
                        json(&result)?
                    } else {
                        result.text(true)
                    }
                }
                Command::Module(args) => {
                    let result = queries::module(&graph, &args.name)?;
                    if args.json {
                        json(&result)?
                    } else {
                        result.text()
                    }
                }
                Command::Dump(_) => json(&graph)?,
                Command::Delta(_)
                | Command::Policy(_)
                | Command::Stages(_)
                | Command::Seams(_)
                | Command::Comments(_)
                | Command::Diff(_)
                | Command::Visibility(_)
                | Command::Moved(_)
                | Command::Concentration(_)
                | Command::Spans(_)
                | Command::NontestLoc(_)
                | Command::Map(_)
                | Command::Flow(_)
                | Command::Program(_)
                | Command::Representations(_)
                | Command::Dependencies(_)
                | Command::Overview(_)
                | Command::Compare(_) => unreachable!(),
            };
            print!("{text}");
            Ok(())
        }
    }
}
fn program(root: &Path, metadata: &cargo_metadata::Metadata, options: &ProgramArgs) -> Result<()> {
    let (text, architecture) = architecture(root, Some(&options.architecture))?;
    let architecture = architecture.ok_or("program requires architecture boundaries")?;
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let analysis = atlas_core::analysis::Analysis::new(&snapshot)?;
    let entry = options.path.as_deref().zip(options.function.as_deref());
    let program = program::from_analysis(&analysis, &architecture, entry)?;
    let overview = program.overview();
    if options.json {
        print!(
            "{}",
            json(&serde_json::json!({"program": &program, "overview": &overview}))?
        );
        return Ok(());
    }
    let mut diagram = program.diagram(&overview)?;
    let query = Query {
        focus: Some("program".into()),
        level: Level::Stages,
        view: View::Program,
    };
    let destination = output::Destination::new(
        root,
        &options.render,
        &query,
        protected_inputs(metadata, graph, [root.join(&options.architecture)]),
    );
    let pending = destination.prepare(
        Some(graph),
        &mut diagram,
        text.as_deref(),
        Some((
            "program",
            &json(&serde_json::json!({"program": &program, "overview": &overview}))?,
        )),
    )?;
    let artifact = if options.render.backend == Backend::Ascii {
        let mut text = program.ascii(&overview);
        for (label, url) in &diagram.evidence_links {
            write!(text, "\n{label}: {url}").expect("formatting into a String cannot fail");
        }
        Ok(Artifact {
            primary: Document {
                suffix: "txt",
                bytes: text.into_bytes(),
            },
            companions: vec![],
        })
    } else {
        render(&options.render, &diagram)
    };
    destination.write(artifact, pending)
}
fn map(root: &Path, metadata: &cargo_metadata::Metadata, options: &MapArgs) -> Result<()> {
    let graph = discovery::scan_metadata(metadata)?;
    let (profile_text, profile) = architecture(root, options.architecture.as_deref())?;
    let query = options.query();
    let mut diagram = diagram::project(&graph, profile.as_ref(), &query)?;
    let destination = output::Destination::new(
        root,
        &options.render,
        &query,
        protected_inputs(
            metadata,
            &graph,
            options.architecture.iter().map(|path| root.join(path)),
        ),
    );
    let pending = destination.prepare(Some(&graph), &mut diagram, profile_text.as_deref(), None)?;
    destination.write(render(&options.render, &diagram), pending)?;
    if destination.path.is_some() {
        eprintln!("{}", diagram.summary());
    }
    Ok(())
}
fn flow(root: &Path, metadata: &cargo_metadata::Metadata, options: &FlowArgs) -> Result<()> {
    let (profile_text, profile) = architecture(root, options.architecture.as_deref())?;
    let selected_anchor = options.named_anchor.as_ref().or(options.anchor.as_ref());
    let anchors = if let (Some(path), Some(function)) = (&options.path, &options.function) {
        if selected_anchor.is_some() {
            return Err("choose a named workflow or --path/--function, not both".into());
        }
        vec![WorkflowAnchor {
            name: function.clone(),
            path: path.clone(),
            function: function.clone(),
        }]
    } else {
        let profile = profile
            .as_ref()
            .ok_or("flow needs --architecture or both --path and --function")?;
        let anchors: Vec<_> = profile
            .workflows
            .iter()
            .filter(|a| selected_anchor.is_none_or(|name| name == &a.name))
            .cloned()
            .collect();
        if anchors.is_empty() {
            return Err("no matching workflow anchors; inspect the architecture profile".into());
        }
        anchors
    };
    let snapshot = discovery::snapshot_metadata(metadata)?;
    let graph = snapshot.graph();
    let (flows, mut diagram) = if options.depth > 0 {
        let path = options
            .path
            .as_deref()
            .ok_or("--depth needs --path and --function")?;
        let function = options
            .function
            .as_deref()
            .ok_or("--depth needs --function")?;
        let analysis = atlas_core::analysis::Analysis::new(&snapshot)?;
        let (flow, diagram) = analysis.expanded(path, function, options.depth)?;
        (vec![(function.to_owned(), flow)], diagram)
    } else {
        let selections: Vec<_> = anchors
            .iter()
            .map(|a| (a.path.as_str(), a.function.as_str()))
            .collect();
        let flows: Vec<_> = anchors
            .iter()
            .zip(workflow::read_many(root, &selections)?)
            .map(|(a, f)| (a.name.clone(), f))
            .collect();
        let diagram = workflow_diagram(&flows)?;
        (flows, diagram)
    };
    let query = Query {
        focus: selected_anchor
            .cloned()
            .or(options.function.clone())
            .or(Some("workflows".into())),
        level: Level::Declarations,
        view: View::Workflow,
    };
    let destination = output::Destination::new(
        root,
        &options.render,
        &query,
        protected_inputs(
            metadata,
            graph,
            options
                .architecture
                .iter()
                .map(|path| root.join(path))
                .chain(anchors.iter().map(|anchor| root.join(&anchor.path))),
        ),
    );
    let pending = destination.prepare(
        None,
        &mut diagram,
        profile_text.as_deref(),
        Some(("workflow", &json(&flows)?)),
    )?;
    let artifact = if options.render.backend == Backend::Ascii && options.depth > 0 {
        Ascii.render(&diagram)
    } else if options.render.backend == Backend::Ascii {
        let mut text = flows
            .iter()
            .map(|(name, flow)| format!("{name}\n{}", flow.ascii()))
            .collect::<Vec<_>>()
            .join("\n");
        for (label, url) in &diagram.evidence_links {
            write!(text, "\n{label}: {url}").expect("formatting into a String cannot fail");
        }
        text.push('\n');
        Ok(Artifact {
            primary: Document {
                suffix: "txt",
                bytes: text.into_bytes(),
            },
            companions: Vec::new(),
        })
    } else {
        render(&options.render, &diagram)
    };
    destination.write(artifact, pending)
}
fn workflow_diagram(flows: &[(String, workflow::Workflow)]) -> Result<Diagram> {
    let mut combined = Diagram {
        title: "Source workflows (independent bodies)".into(),
        level: Level::Declarations,
        view: View::Workflow,
        nodes: vec![],
        edges: vec![],
        inventory: vec![],
        coverage: atlas::diagram::Coverage::default(),
        notes: vec![],
        evidence_links: vec![],
    };
    for (index, (name, flow)) in flows.iter().enumerate() {
        let mut diagram = flow.diagram();
        for node in &mut diagram.nodes {
            node.id = format!("w{index}_{}", node.id);
            node.group = format!("{} · {}", node.group, name);
        }
        for edge in &mut diagram.edges {
            edge.from = format!("w{index}_{}", edge.from);
            edge.to = format!("w{index}_{}", edge.to);
        }
        combined.nodes.extend(diagram.nodes);
        combined.edges.extend(diagram.edges);
        combined.inventory.extend(diagram.inventory);
        combined.notes.extend(diagram.notes);
    }
    combined.notes.sort();
    combined.notes.dedup();
    combined.inventory.sort();
    combined.inventory.dedup();
    combined.validate()?;
    Ok(combined)
}
fn render(options: &RenderOptions, diagram: &Diagram) -> Result<Artifact> {
    match options.backend {
        Backend::Ascii => Ascii.render(diagram),
        Backend::Pele => Pele {
            html: options.format == Format::Html,
        }
        .render(diagram),
    }
}
fn architecture(
    root: &Path,
    path: Option<&Path>,
) -> Result<(Option<String>, Option<Architecture>)> {
    let text = path.map(|p| fs::read_to_string(root.join(p))).transpose()?;
    let profile = text
        .as_deref()
        .map(|s| Architecture::parse(s, root))
        .transpose()?;
    Ok((text, profile))
}
fn json(value: &impl serde::Serialize) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

#[derive(Parser)]
#[command(
    version,
    about = "Source-backed Rust navigation, workflows, and architecture maps",
    after_help = "Navigation: census, name (optional --relations), module, dump.\nReview: spans, nontest-loc, concentration, diff, moved, visibility, comments.\nChecks: policy, delta, stages, seams (explicit policy required).\nQualified diagrams: overview, representations, dependencies, compare.\nSource-body syntax: flow. Declared/lexical summaries: program, map.\nAll commands accept --root PATH (default: current directory); positional ROOT remains compatible.\nUse --scope where available to select source separately from the project root.\nLexical co-mentions do not establish qualified uses or runtime order."
)]
struct Cli {
    /// Cargo project directory; defaults to the current directory. Positional ROOT remains supported.
    #[arg(long = "root", global = true, value_name = "PATH")]
    project_root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
impl Cli {
    fn selected_root(&self) -> Result<PathBuf> {
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
enum Command {
    /// Judge architecture debt between pinned baseline and candidate source captures.
    #[command(
        after_help = "Question: what architecture debt changes between two exact revisions? Population: the authored policy selects core packages and rule populations. Evidence: paired source measurements and supplied authority facts; required missing evidence is UNAVAILABLE. Scope: --policy and the pinned revisions. Example: atlas delta HEAD~1 --candidate HEAD --policy .config/architecture.toml --json"
    )]
    Delta(delta_command::Options),
    /// Read production paths against an explicit stage order and declared interfaces.
    #[command(
        after_help = "Question: which written production paths cross the declared stage interfaces? Population: the policy selects packages and ordered stages. Evidence: lexical paths, not runtime calls or complete policy PASS. Scope: --policy. Example: atlas stages --policy .config/architecture.toml --json"
    )]
    Stages(stage_command::Options),
    /// Validate authored policy against captured source and required observations.
    #[command(
        after_help = "Question: are authored policy inputs valid against captured source? Population: native source trees with each rule's production/test selection. Evidence: configuration and source observations; this does not judge measured debt or a landing delta. Required missing authority is UNAVAILABLE. Scope: --policy. Example: atlas policy --policy .config/architecture.toml --json"
    )]
    Policy(policy_command::Options),
    /// Read item seams, writer calls and test coupling; project boundaries required.
    #[command(
        after_help = "Question: where do items, writers and tests cross declared seams? Population: selected source trees, including rule-selected tests and mounted code. Evidence: written paths and project-supplied boundaries; missing authority is UNAVAILABLE. Scope: --policy; --items exposes observations. Example: atlas seams --policy .config/architecture.toml --items --json"
    )]
    Seams(stage_command::SeamOptions),
    /// Audit documentation with explicit population and caller-supplied citation markers.
    #[command(
        after_help = "Question: which selected declarations or files lack the requested line documentation? Population: --population subjects outside test modules; directly gated helpers remain. Evidence: /// and //! lines, not expanded doc attributes or semantic citation resolution. Scope: --scope. No architecture verdict is assigned. Example: atlas comments --audit --scope src --population pub-ish --json"
    )]
    Comments(documentation_command::Options),
    /// Compare production module shape: lines, block depth and item breadth; no verdict.
    #[command(
        after_help = "Question: what captured source shape or declarations changed? Population: non-test-module source; --items defaults to all written visibilities, with --population available. Evidence: lexical spans and known incident co-mentions, not behavioral equivalence. Scope: --scope, including removed paths. Inline bodies on the signature-ending line remain captured. Example: atlas diff HEAD~1 --scope src --items --graph --json"
    )]
    Diff(review_command::DiffOptions),
    /// Compare written visibility by crate/class/name; repeated names remain approximate.
    #[command(
        after_help = "Question: which matched declarations changed their written visibility? Population: production declarations, excluding test-implying gates. Evidence: crate/class/name lexical matching; duplicate keys remain approximate, not proof of API compatibility. Scope: --scope narrows output while matching spans the crate. Example: atlas visibility HEAD~1 --scope src --json"
    )]
    Visibility(review_command::Options),
    /// Classify normalized token bodies: exact, rerooted, changed or unreadable; no behavior proof.
    #[command(
        after_help = "Question: which declarations kept their normalized body across a move? Population: production declarations, excluding test-implying gates. Evidence: exact tokens or tokens after import rerooting; duplicate keys remain approximate and no behavioral equivalence is claimed. Scope: --scope narrows output while matching spans the crate. Example: atlas moved HEAD~1 --scope src --json"
    )]
    Moved(review_command::Options),
    /// Show attached lexical item spans and uncovered code; includes private imports.
    #[command(
        after_help = "Question: where are the attached declarations and uncovered code? Population: all captured item spans, including test items; line and shape metrics separately exclude test modules. Evidence: lexical item and source-line boundaries, without macro expansion. Scope: positional PATH or --scope; no match refuses. Example: atlas spans --scope src --json"
    )]
    Spans(SourceReviewArgs),
    /// Report native top-level block depth and item breadth, without assigning a score.
    #[command(
        after_help = "Question: how much source lies in each top-level block? Population: all visibilities outside test modules, with directly gated helpers retained. Evidence: native declaration-to-next-declaration block depth and item breadth; not cognitive complexity or a quality verdict. Scope: positional PATH or --scope. Example: atlas concentration --scope src --json"
    )]
    Concentration(SourceReviewArgs),
    /// Count lines outside native cfg-test module boundaries.
    #[command(
        after_help = "Question: how many physical and token-bearing lines lie outside test modules? Population: native cfg-test module exclusion, including attached attributes; directly gated helpers remain. Evidence: captured lexical lines, not executed coverage. Scope: positional PATH or --scope. Example: atlas nontest-loc --scope src --json"
    )]
    NontestLoc(SourceReviewArgs),
    /// Compare source contracts, constructors and preparation across revisions.
    #[command(
        after_help = "Question: which representation observations changed between revisions? Population: production declarations selected by the optional saved query. Evidence: qualified signature, field and construction observations, not inferred behavior. Scope: --before, optional --after and --query. Example: atlas compare --before HEAD~1 --backend ascii"
    )]
    Compare(comparison_command::Options),
    /// Derive a default representation picture from MODULES/SURFACES in source.
    #[command(
        after_help = "Question: which representation contracts follow the declared stage surfaces? Population: production declarations reached from MODULES/SURFACES or a supplied declaration adapter. Evidence: signatures and source declarations, not execution order. Scope: selected Cargo root and adapter input. Missing declarations refuse. Example: atlas overview --backend pele --out target/atlas/overview.svg"
    )]
    Overview(overview_command::Options),
    /// Qualified observed file relationships, with calls/signatures/fields kept distinct.
    #[command(
        after_help = "Question: which files are related by observed calls, signatures or retained fields? Population: captured production source. Evidence: selected qualified relationship kinds with unresolved and ambiguous targets retained; not a complete Rust call graph. Scope: --scope and relation selection. Example: atlas dependencies --relation all --json"
    )]
    Dependencies(dependency_command::Options),
    /// Derive representation contracts, direct fields and preparation calls from a saved question.
    #[command(
        after_help = "Question: how are the selected representations produced, retained and prepared? Population: production types and functions selected by --query. Evidence: qualified signatures, direct fields and written construction/call sites; no inferred ownership or freshness. Scope: the saved query. Example: atlas representations --query examples/queries/atlas.json --backend ascii"
    )]
    Representations(representation_command::Options),
    /// Follow selected entry functions and fold declared stage boundaries.
    #[command(
        after_help = "Question: how do selected entry bodies cross declared boundaries? Population: source functions selected by the architecture profile or --path/--function. Evidence: written calls and profile grouping, not feasible execution paths. Scope: --architecture and entry selection. Example: atlas program --architecture architecture.json --backend ascii"
    )]
    Program(ProgramArgs),
    /// Project declared boundaries or approximate lexical type relationships.
    #[command(
        after_help = "Question: how do declared groups or lexical type mentions connect? Population: selected workspace files and profile groups. Evidence: declared permissions or approximate lexical co-mentions; neither establishes runtime flow. Scope: --scope, --level, --view and optional --architecture. Example: atlas map --scope src --backend ascii"
    )]
    Map(MapArgs),
    /// Read coordinating function bodies; source order is not a resolved call graph.
    #[command(
        after_help = "Question: what source-control steps occur in a selected function body? Population: selected written bodies; --depth optionally expands matched source calls. Evidence: syntax order, branches, loops and explicit returns; not feasible paths, macro execution or unwinding. Scope: --path/--function or a declared anchor. Example: atlas flow --path src/main.rs --function main --depth 1 --backend ascii"
    )]
    Flow(FlowArgs),
    /// Count physical files, declarations, and lexical evidence.
    #[command(
        after_help = "Question: what files and declarations exist in this source scope? Population: unflagged 0.5 lexical graph, or explicit --population native-style subjects outside test modules; macro and line units are independent. Evidence: captured declarations and lexical counts, not executed or resolved APIs. Scope: --scope. Example: atlas census --scope src --population pub-ish --json"
    )]
    Census(ScopeArgs),
    /// Find declarations by exact name or source-site ID, keeping ambiguity explicit.
    #[command(
        after_help = "Question: where is this name declared or mentioned? Population: unflagged 0.5 graph, or explicit --population native-style subjects outside test modules. Evidence: written names, lexical candidate bindings and approximate co-mentions, not Rust resolution. Scope: NAME or path::NAME; ambiguity remains explicit. Example: atlas name Graph --population pub-ish --json"
    )]
    Name(RelatedNameArgs),
    /// Inspect a physical module, directory, or Cargo package.
    #[command(
        after_help = "Question: what is in this file, folder or package and how is it mentioned? Population: unflagged 0.5 graph, or --population/--items/--graph native-style source outside test modules. Evidence: declarations, normalized imports and approximate co-mentions, not resolved reachability. Scope: NAME/path; native mode uses a physical path. Example: atlas module src/lib.rs --population pub-ish --items --graph --json"
    )]
    Module(ModuleArgs),
    /// Compatibility spelling of name --relations (approximate lexical evidence).
    #[command(hide = true)]
    #[command(
        after_help = "Compatibility alias for name --relations. Population and evidence follow the lexical name query; no runtime reachability is established. Scope: written NAME. Example: atlas name Graph --relations --json"
    )]
    Related(NameArgs),
    /// Dump the complete source graph as JSON.
    #[command(
        after_help = "Question: what complete source observations can a consumer inspect? Population: unflagged 0.5 graph, or explicit --population native declarations, facade/reexport candidates and co-mentions outside test modules. Evidence: lexical observations with ambiguity and limits; not a compiler symbol table. Scope: selected Cargo root. Example: atlas dump --population all --json"
    )]
    Dump(DumpArgs),
}
impl Command {
    fn root(&self) -> Option<&Path> {
        match self {
            Self::Delta(_)
            | Self::Policy(_)
            | Self::Stages(_)
            | Self::Seams(_)
            | Self::Comments(_)
            | Self::Diff(_)
            | Self::Visibility(_)
            | Self::Moved(_)
            | Self::Spans(_)
            | Self::NontestLoc(_)
            | Self::Concentration(_) => None,
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
        }
    }
}
#[derive(Args)]
struct MapArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    focus: Option<String>,
    /// Select a physical/module scope without positional ambiguity.
    #[arg(long = "scope", conflicts_with = "focus")]
    named_scope: Option<String>,
    #[arg(long)]
    architecture: Option<PathBuf>,
    #[arg(long, value_enum)]
    level: Option<Level>,
    #[arg(long, value_enum)]
    view: Option<View>,
    #[command(flatten)]
    render: RenderOptions,
}
impl MapArgs {
    fn query(&self) -> Query {
        let focus = self.named_scope.as_ref().or(self.focus.as_ref());
        let level = self
            .level
            .unwrap_or(if self.architecture.is_some() && focus.is_none() {
                Level::Stages
            } else {
                Level::Modules
            });
        let view = self.view.unwrap_or(
            if self.architecture.is_some()
                && matches!(level, Level::Stages | Level::Modules)
                && !(focus.is_some() && level == Level::Modules)
            {
                View::Dependencies
            } else {
                View::Types
            },
        );
        Query {
            focus: focus.cloned(),
            level,
            view,
        }
    }
}
#[derive(Args)]
struct ProgramArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    #[arg(long)]
    architecture: PathBuf,
    #[arg(long, requires = "function")]
    path: Option<String>,
    #[arg(long, requires = "path")]
    function: Option<String>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    render: RenderOptions,
}
#[derive(Args)]
struct FlowArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    /// Expand matched source calls; recursion/unknown bodies remain explicit (max 6).
    #[arg(long,default_value_t=0,value_parser=clap::value_parser!(u8).range(0..=i64::from(workflow::MAX_DEPTH)),requires="path")]
    depth: u8,
    /// Profile workflow name. Omit to show independent source bodies together.
    anchor: Option<String>,
    /// Select a declared workflow without a positional ROOT.
    #[arg(long = "anchor", conflicts_with = "anchor")]
    named_anchor: Option<String>,
    #[arg(long)]
    architecture: Option<PathBuf>,
    #[arg(long, requires = "function")]
    path: Option<String>,
    #[arg(long, requires = "path")]
    function: Option<String>,
    #[command(flatten)]
    render: RenderOptions,
}
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Backend {
    Ascii,
    Pele,
}
impl Backend {
    pub(crate) fn render_view(self, view: atlas::render::View<'_>) -> Result<Artifact> {
        match self {
            Self::Ascii => Ascii.render_view(view),
            Self::Pele => Pele { html: false }.render_view(view),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Html,
    Svg,
}
#[derive(Args)]
pub(crate) struct RenderOptions {
    #[arg(long, value_enum, default_value = "ascii")]
    backend: Backend,
    #[arg(long, value_enum, default_value = "html")]
    format: Format,
    /// Relative output paths are resolved against the Cargo workspace root.
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    evidence_dir: Option<PathBuf>,
}
#[derive(Args)]
struct ScopeArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    scope: Option<String>,
    #[arg(long = "scope", conflicts_with = "scope")]
    named_scope: Option<String>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: navigation_command::PopulationArgs,
}
#[derive(Args)]
#[command(allow_missing_positional = true)]
struct RelatedNameArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    name: String,
    /// Include incoming/outgoing approximate lexical co-mentions, not qualified uses.
    #[arg(long)]
    relations: bool,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: navigation_command::PopulationArgs,
}
#[derive(Args)]
#[command(allow_missing_positional = true)]
struct ModuleArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    name: String,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: navigation_command::PopulationArgs,
    /// Show every captured production item instead of the module summary alone.
    #[arg(long)]
    items: bool,
    /// Show declaration co-mentions; lexical, not a resolved call graph.
    #[arg(long)]
    graph: bool,
}
#[derive(Args)]
#[command(allow_missing_positional = true)]
struct NameArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    name: String,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: navigation_command::PopulationArgs,
}
#[derive(Args)]
struct DumpArgs {
    /// Compatibility positional project root; prefer --root.
    root: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    population: navigation_command::PopulationArgs,
}
#[cfg(test)]
mod tests {
    use super::*;
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

#[derive(Args)]
struct SourceReviewArgs {
    /// Compatibility positional source scope; defaults to the whole workspace.
    path: Option<String>,
    /// Physical source file or directory; no matches is an error.
    #[arg(long, conflicts_with = "path")]
    scope: Option<String>,
    #[arg(long)]
    json: bool,
}
#[derive(Clone, Copy)]
enum SourceQuestion {
    Spans,
    Loc,
    Concentration,
}
impl SourceQuestion {
    fn name(&self) -> &'static str {
        match self {
            Self::Spans => "spans",
            Self::Loc => "nontest-loc",
            Self::Concentration => "concentration",
        }
    }
}
fn source_review(
    metadata: &cargo_metadata::Metadata,
    args: &SourceReviewArgs,
    question: SourceQuestion,
) -> Result<()> {
    let snapshot = atlas_core::discovery::snapshot_metadata(metadata)?;
    let report = atlas_core::review::outline(
        &snapshot,
        args.scope
            .as_deref()
            .or(args.path.as_deref())
            .unwrap_or("."),
    )?;
    if args.json {
        println!(
            "{}",
            json(&serde_json::json!({"schema":1,"question":question.name(),"report":report}))?
        );
    } else {
        for file in &report.files {
            if matches!(question, SourceQuestion::Loc) {
                println!("{} {}", file.non_test_lines, file.file);
            } else if matches!(question, SourceQuestion::Concentration) {
                let depth = file.largest_block.as_ref().map_or(0, |b| b.lines);
                println!(
                    "{}: {} non-test lines, {} top-level items, largest block {}",
                    file.file, file.non_test_lines, file.top_level_items, depth
                );
            } else {
                println!("{}", file.file);
                for item in &file.items {
                    println!(
                        "  {}:{}-{} {} {}",
                        item.declaration, item.first, item.last, item.kind, item.name
                    );
                }
                if !file.uncovered_code.is_empty() {
                    println!("  uncovered code: {:?}", file.uncovered_code);
                }
            }
        }
    }
    Ok(())
}

fn protected_inputs(
    metadata: &cargo_metadata::Metadata,
    graph: &atlas::model::SourceGraph,
    extra: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    metadata
        .workspace_packages()
        .into_iter()
        .map(|package| package.manifest_path.as_std_path().to_owned())
        .chain([
            metadata
                .workspace_root
                .join(".cargo/config.toml")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join(".cargo/config")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join("Cargo.toml")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join("Cargo.lock")
                .into_std_path_buf(),
            metadata
                .workspace_root
                .join("rust-toolchain.toml")
                .into_std_path_buf(),
        ])
        .chain(
            graph
                .files
                .iter()
                .map(|file| metadata.workspace_root.join(&file.path).into_std_path_buf()),
        )
        .chain(extra)
        .collect()
}
