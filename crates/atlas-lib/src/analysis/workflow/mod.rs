//! Ordered source syntax for a selected coordinating function.
//!
//! These diagrams describe Rust syntax, not a resolved call graph. Calls are not
//! expanded; macros, closures and unsupported constructs remain visible and opaque.
mod compact;
pub(crate) mod facts;
mod functions;
mod parse;
mod projection;
use crate::{Result, diagram::Diagram};
use serde::{Deserialize, Serialize};
use std::{
    fmt::Write,
    path::{Component, Path},
};

/// A relative source location; line numbers are one-based, columns zero-based.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Site {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}
impl Site {
    pub(crate) fn at(path: &str, span: proc_macro2::Span) -> Self {
        let start = span.start();
        let end = span.end();
        Self {
            path: path.into(),
            line: start.line,
            column: start.column,
            end_line: end.line,
            end_column: end.column,
        }
    }

    pub fn contains(&self, other: &Self) -> bool {
        self.path == other.path
            && (self.line, self.column) <= (other.line, other.column)
            && (other.end_line, other.end_column) <= (self.end_line, self.end_column)
    }
    pub fn link(&self) -> String {
        format!("{}:{}", self.path, self.line)
    }
}

/// Display detail changes projection only; raw workflow facts are retained.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[non_exhaustive]
pub enum Detail {
    #[default]
    Expressions,
    Control,
}

/// One syntactic step. Branch arms and loop bodies keep their own ordered steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Step {
    /// Explicit borrowing, including its independently ordered operand steps.
    Borrow { label: String, site: Site },
    Action {
        label: String,
        /// Local pattern bound to this final initializer step, when written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        binding: Option<String>,
        fallible: bool,
        site: Site,
    },
    Branch {
        label: String,
        arms: Vec<Arm>,
        site: Site,
    },
    Match {
        label: String,
        arms: Vec<MatchArm>,
        site: Site,
    },
    Loop {
        label: String,
        form: LoopKind,
        head: Vec<Self>,
        body: Vec<Self>,
        target: Option<String>,
        site: Site,
    },
    /// A labelled block: its steps run once, and `break 'label` leaves it.
    Block {
        label: String,
        body: Vec<Self>,
        target: String,
        site: Site,
    },
    Exit {
        label: String,
        control: ExitKind,
        site: Site,
    },
    Opaque {
        label: String,
        fallible: bool,
        site: Site,
    },
}
/// The loop forms have different entry/exhaustion behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum LoopKind {
    Infinite,
    While,
    For,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MatchArm {
    pub pattern: String,
    pub guard: Vec<Step>,
    pub body: Vec<Step>,
}
/// Syntactic control transfer, preserving labelled loop targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transfer", content = "target", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ExitKind {
    Return,
    Break(Option<String>),
    Continue(Option<String>),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Arm {
    pub label: String,
    pub steps: Vec<Step>,
}
impl Step {
    pub fn site(&self) -> &Site {
        match self {
            Self::Borrow { site, .. }
            | Self::Action { site, .. }
            | Self::Branch { site, .. }
            | Self::Match { site, .. }
            | Self::Loop { site, .. }
            | Self::Block { site, .. }
            | Self::Exit { site, .. }
            | Self::Opaque { site, .. } => site,
        }
    }
    /// The step's own label, without the fallibility or opaque markers.
    pub fn label(&self) -> &str {
        match self {
            Self::Borrow { label, .. }
            | Self::Action { label, .. }
            | Self::Branch { label, .. }
            | Self::Match { label, .. }
            | Self::Loop { label, .. }
            | Self::Block { label, .. }
            | Self::Exit { label, .. }
            | Self::Opaque { label, .. } => label,
        }
    }
    pub(super) fn label_mut(&mut self) -> &mut String {
        match self {
            Self::Borrow { label, .. }
            | Self::Action { label, .. }
            | Self::Branch { label, .. }
            | Self::Match { label, .. }
            | Self::Loop { label, .. }
            | Self::Block { label, .. }
            | Self::Exit { label, .. }
            | Self::Opaque { label, .. } => label,
        }
    }
}
/// A source call observation. Receiver dispatch is unresolved until supported by source types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Call {
    pub spelling: String,
    pub receiver: Option<String>,
    pub arguments: Vec<String>,
    pub site: Site,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Binding {
    pub name: String,
    pub ty: Option<String>,
    pub initializer: Option<String>,
    pub site: Site,
}
/// An item or `use` written inside a body. It declares a name and runs
/// nothing, so it is not a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Declared {
    pub label: String,
    pub site: Site,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Workflow {
    pub schema: u32,
    pub function: String,
    pub module: Vec<String>,
    pub owner: Option<String>,
    /// A trait impl's method, whose owner may be a foreign type. Read from
    /// source only; workflow JSON does not carry it.
    #[serde(skip)]
    pub trait_impl: bool,
    #[serde(default)]
    pub type_parameters: Vec<String>,
    pub site: Site,
    pub steps: Vec<Step>,
    pub calls: Vec<Call>,
    pub bindings: Vec<Binding>,
    /// Names bound by items and `use` declarations written inside the body,
    /// sorted; `*` marks a glob import. They shadow the module's names, so a
    /// call through one stays unresolved.
    #[serde(default)]
    pub local_names: Vec<String>,
    /// Body-local items and imports in source order, outside closures.
    #[serde(default)]
    pub declarations: Vec<Declared>,
    pub imports: std::collections::BTreeMap<String, Vec<Vec<String>>>,
    pub output: String,
    pub limits: Vec<String>,
}

/// Read exactly one function in a relative physical Rust file.
/// Qualified names distinguish inline modules and impl methods; ambiguous names refuse.
pub fn read(root: &Path, path: &str, function: &str) -> Result<Workflow> {
    read_many(root, &[(path, function)])?
        .pop()
        .ok_or_else(|| "workflow result missing".into())
}
/// Read ordered anchors, parsing each physical source file only once per batch.
pub fn read_many(root: &Path, anchors: &[(&str, &str)]) -> Result<Vec<Workflow>> {
    let root = root.canonicalize()?;
    let mut grouped = std::collections::BTreeMap::<&str, Vec<(usize, &str)>>::new();
    for (index, &(path, function)) in anchors.iter().enumerate() {
        grouped.entry(path).or_default().push((index, function));
    }
    let mut ordered = std::collections::BTreeMap::new();
    for (path, entries) in grouped {
        let source = source(&root, path)?;
        let selectors: Vec<_> = entries.iter().map(|&(_, function)| function).collect();
        for ((index, _), flow) in entries
            .into_iter()
            .zip(parse::parse_many(&source, path, &selectors)?)
        {
            ordered.insert(index, flow);
        }
    }
    Ok(ordered.into_values().collect())
}
/// Parsed module facts shared by body views and the program index.
#[non_exhaustive]
pub struct ParsedFile {
    pub functions: Vec<Workflow>,
    pub exports: std::collections::BTreeMap<String, Vec<Vec<String>>>,
    /// Declared type and trait paths, relative to the file's module.
    pub types: Vec<Vec<String>>,
}
/// Inspect all function bodies in a file with one AST parse.
pub fn read_file(root: &Path, path: &str) -> Result<ParsedFile> {
    let root = root.canonicalize()?;
    parse::parse_all(&source(&root, path)?, path)
}
fn source(root: &Path, path: &str) -> Result<String> {
    if path.is_empty()
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("workflow path must be a normalized relative file path".into());
    }
    let file = root.join(path).canonicalize()?;
    if !file.starts_with(root) {
        return Err("workflow source escapes the project root".into());
    }
    Ok(std::fs::read_to_string(file)?)
}

impl Workflow {
    /// Project syntax into the common renderer model, with source-order evidence.
    pub fn diagram(&self) -> Diagram {
        projection::diagram(self)
    }
    pub fn diagram_with_detail(&self, detail: Detail) -> Result<Diagram> {
        projection::with_detail(self, detail)
    }
    /// Structured terminal view: indentation is control nesting, arrows are source order.
    pub fn ascii(&self) -> String {
        let mut out = format!("{} ({})\n\n", self.function, self.site.link());
        for declared in &self.declarations {
            let _ = writeln!(
                out,
                "declares (not a step): {} [{}]",
                declared.label,
                declared.site.link()
            );
        }
        if !self.declarations.is_empty() {
            out.push('\n');
        }
        write_steps(&mut out, &self.steps, 0);
        let _ = writeln!(
            out,
            "\nreturn from {} → {} (if the body completes)",
            self.function, self.output
        );
        out.push_str("\nSource syntax, not a resolved call graph. ? may return early.\n");
        for limit in &self.limits {
            let _ = writeln!(out, "- {limit}");
        }
        out
    }
}
fn write_steps(out: &mut String, steps: &[Step], depth: usize) {
    for step in steps {
        let pad = "  ".repeat(depth);
        let marker = match step {
            Step::Action { fallible: true, .. } => " ?",
            Step::Opaque { fallible: true, .. } => " ? [opaque]",
            Step::Opaque { .. } => " [opaque]",
            _ => "",
        };
        let _ = writeln!(
            out,
            "{pad}-> {}{marker} [{}]",
            step.label(),
            step.site().link()
        );
        match step {
            Step::Branch { arms, .. } => {
                for arm in arms {
                    let _ = writeln!(out, "{pad}   | {}", arm.label);
                    write_steps(out, &arm.steps, depth + 2);
                }
            }
            Step::Match { arms, .. } => {
                for arm in arms {
                    let _ = writeln!(out, "{pad}   | pattern {}", arm.pattern);
                    write_steps(out, &arm.guard, depth + 2);
                    write_steps(out, &arm.body, depth + 2);
                }
            }
            Step::Loop { head, body, .. } => {
                write_steps(out, head, depth + 1);
                write_steps(out, body, depth + 1);
                let _ = writeln!(out, "{pad}   repeat / exit loop");
            }
            Step::Block { body, .. } => {
                write_steps(out, body, depth + 1);
                let _ = writeln!(out, "{pad}   end of block");
            }
            _ => {}
        }
    }
}

/// Shared AST input for queries needing both function bodies and type structure.
pub(crate) fn read_ast(
    source: &crate::source::Source,
    path: &str,
    ast: &syn::File,
    tests: crate::population::TestSelection,
) -> ParsedFile {
    parse::parse_ast(
        source.text(),
        path,
        ast,
        crate::population::SyntaxSelection::Selected { source, tests },
    )
}

/// Expand source-resolved calls to a bounded depth, preserving occurrence-specific returns.
/// Recursion and unavailable bodies are explicit boundaries, not invented paths.
pub fn expanded(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    graph: &crate::model::SourceGraph,
    path: &str,
    function: &str,
    depth: u8,
) -> Result<(Workflow, Diagram)> {
    check_depth(depth)?;
    let architecture = crate::architecture::Architecture {
        schema: 1,
        provenance: vec![],
        modules: vec![],
        workflows: vec![],
    };
    let index = crate::analysis::index::Index::new(root, metadata, graph, &architecture)?;
    expand(&index, path, function, depth)
}
/// Expand a captured name index without any source or Cargo read.
pub fn expand(
    index: &crate::analysis::index::Index,
    path: &str,
    function: &str,
    depth: u8,
) -> Result<(Workflow, Diagram)> {
    expand_with_detail(index, path, function, depth, Detail::Expressions)
}
/// Use the same source-control builder with explicit display detail.
pub fn expand_with_detail(
    index: &crate::analysis::index::Index,
    path: &str,
    function: &str,
    depth: u8,
    detail: Detail,
) -> Result<(Workflow, Diagram)> {
    check_depth(depth)?;
    let entry = index.select(path, function)?;
    let diagram = projection::expanded(index, entry, depth, detail)?;
    Ok((index.functions()[entry].flow.clone(), diagram))
}

/// Maximum supported source-call expansion depth; larger requests must narrow their question.
pub const MAX_DEPTH: u8 = 6;
pub(crate) fn check_depth(depth: u8) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(format!("flow depth must be at most {MAX_DEPTH}").into());
    }
    Ok(())
}
