//! Declared boundaries and lexical mentions; protect inputs before publishing the view.
use super::{
    output::{self, RenderOptions, protected_inputs, render},
    profile::architecture,
};
use atlas::{
    Result,
    diagram::{self, Level, Query, View},
    discovery,
};
use clap::Args;
use std::path::{Path, PathBuf};
#[derive(Args)]
pub(super) struct Options {
    /// Compatibility positional project root; prefer --root.
    pub(super) root: Option<PathBuf>,
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
impl Options {
    pub(super) fn query(&self) -> Query {
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

pub(super) fn run(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
    options: &Options,
) -> Result<()> {
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
