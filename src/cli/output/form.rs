//! The `--as` output flag and its agreement with the 0.18 spellings it supersedes.
//! Each command lists the formats it can produce. The old flags stay the
//! command's own fields: `--as` selects among the same outputs and never adds
//! one, so an old spelling and its `--as` form print the same bytes.
use super::notice::Notices;
use cargo_atlas::Result;
use clap::{
    ArgMatches, Args, ValueEnum,
    builder::{PossibleValuesParser, TypedValueParser},
    parser::ValueSource,
};

/// An output format a command can be asked for with `--as`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub(crate) enum As {
    Text,
    Json,
    Jsonl,
    Svg,
    Html,
}
/// A `--as` parser that accepts, and lists in `--help`, only `allowed`.
pub(crate) fn formats(allowed: &'static [As]) -> impl TypedValueParser<Value = As> {
    PossibleValuesParser::new(allowed.iter().filter_map(ValueEnum::to_possible_value))
        .map(|name| As::from_str(&name, false).expect("a listed format parses"))
}

/// The output choice of a command that prints text or JSON.
#[derive(Args)]
pub(crate) struct Report {
    /// Output format; --json is the same as --as json.
    #[arg(
        long = "as",
        id = "as",
        value_name = "FORMAT",
        value_parser = formats(&[As::Text, As::Json])
    )]
    form: Option<As>,
    /// Print the same answer as JSON, for scripts and agents.
    #[arg(long)]
    json: bool,
}
impl Report {
    pub(crate) fn json(&self) -> bool {
        self.json || self.form == Some(As::Json)
    }
    /// The format `--as` chose, for a command that offers more than text and
    /// JSON.
    pub(crate) fn chosen(&self) -> Option<As> {
        self.form
    }
}

/// Note each superseded output spelling this invocation used. `--json` is a
/// permanent alias for `--as json`, so it is never noted.
pub(crate) fn note_superseded(matches: &ArgMatches, notices: &mut Notices) {
    for (id, old, new) in [
        ("backend", "--backend", "--as"),
        ("format", "--format", "--as"),
        ("scope", "--scope", "--in"),
        ("types", "--type", "the bare TYPES argument"),
        ("before", "--before", "--vs"),
        ("after", "--after", "--at"),
        ("show", "--show", "--rows"),
    ] {
        if given(matches, id).is_some() {
            notices.superseded(old, new);
        }
    }
}
/// Refuse `--as` beside an old spelling that asks for a different output.
/// Agreeing spellings are accepted: `--as json --json`, `--as svg --backend pele`.
pub(crate) fn agree(matches: &ArgMatches) -> Result<()> {
    let Some(chosen) = given(matches, "as") else {
        return Ok(());
    };
    let conflict = if given(matches, "json").is_some() && chosen != "json" {
        Some("--json".to_owned())
    } else if let Some(backend) = given(matches, "backend")
        && !matches!(
            (backend.as_str(), chosen.as_str()),
            ("ascii", "text") | ("pele", "svg" | "html")
        )
    {
        Some(format!("--backend {backend}"))
    } else {
        given(matches, "format")
            .filter(|format| *format != chosen)
            .map(|format| format!("--format {format}"))
    };
    match conflict {
        Some(old) => Err(format!("--as {chosen} conflicts with {old}").into()),
        None => Ok(()),
    }
}
/// The value of option `id` when the command line gave it, not a default.
fn given(matches: &ArgMatches, id: &str) -> Option<String> {
    if !matches.ids().any(|known| known == id)
        || matches.value_source(id) != Some(ValueSource::CommandLine)
    {
        return None;
    }
    let value = matches.get_raw(id)?.next()?;
    Some(value.to_string_lossy().into_owned())
}
