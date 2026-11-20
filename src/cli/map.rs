//! Declared boundaries and lexical mentions; protect inputs before publishing the view.
use super::{
    output::{self, RenderOptions, protected_inputs, render},
    profile::architecture,
};
use cargo_atlas::{
    Result,
    diagram::{self, Level, Query, View},
    discovery,
};
use clap::Args;
use std::path::{Path, PathBuf};
#[derive(Args)]
pub(super) struct Options {
    #[command(flatten)]
    pub(super) words: super::subject::Root,
    /// A physical or module scope to draw.
    #[arg(long = "in", value_name = "PATH", conflicts_with = "scope")]
    pub(super) in_path: Option<String>,
    /// Superseded by --in.
    #[arg(long, value_name = "PATH")]
    pub(super) scope: Option<String>,
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
        let focus = self
            .in_path
            .as_deref()
            .or(self.scope.as_deref())
            .or(self.words.subject())
            .map(str::to_owned);
        let focus = focus.as_ref();
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
