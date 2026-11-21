//! `--evidence` (spec §3.9): it names the reading a command answers from.
//! `auto` is the command's current reading and `qualified` names it;
//! `callers` and `callees` offer `semantic` beside it. A reading a command
//! does not offer is refused with what to run instead.
use cargo_atlas::Result;
use clap::{Args, ValueEnum};

#[derive(Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
enum Reading {
    /// The command's own reading.
    #[default]
    Auto,
    /// Names written in the source, matched by spelling.
    Lexical,
    /// Names followed through use statements and modules.
    Qualified,
    /// A rust-analyzer (SCIP) reading, beside the qualified one; needs
    /// --scip FILE or --semantic where a command offers it.
    Semantic,
}
#[derive(Args)]
pub(crate) struct Evidence {
    /// The reading to answer from: auto (the default) and qualified are this
    /// command's reading; semantic adds an index's reading where the command
    /// offers it (callers, callees); otherwise it is refused, with what to run.
    #[arg(long, value_enum, default_value_t)]
    evidence: Reading,
}
impl Evidence {
    /// Whether `command`, which offers the semantic reading beside its
    /// qualified one, answers from it: `indexed` (an index was given with
    /// `--scip` or `--semantic`) selects it. `--evidence semantic` without
    /// an index, and `--evidence qualified` with one, are refused; `lexical`
    /// as in [`Evidence::check`].
    pub(crate) fn semantic(
        &self,
        command: &str,
        needs: &str,
        lexical: &str,
        indexed: bool,
    ) -> Result<bool> {
        match (self.evidence, indexed) {
            (Reading::Lexical, _) => Err(format!("{command} needs {needs}; {lexical}").into()),
            (Reading::Auto, indexed) => Ok(indexed),
            (Reading::Qualified, false) => Ok(false),
            (Reading::Semantic, true) => Ok(true),
            (Reading::Qualified, true) => Err(
                "--evidence qualified conflicts with --scip and --semantic, which select the semantic reading; give one or the other".into(),
            ),
            (Reading::Semantic, false) => Err(format!(
                "{command} --evidence semantic needs an index: give --scip FILE, or --semantic, which runs rust-analyzer and with it the project's build scripts"
            )
            .into()),
        }
    }
    /// Refuse a reading `command` does not offer: it `needs` its own
    /// reading; `lexical` and `semantic` say what to run instead.
    pub(crate) fn check(
        &self,
        command: &str,
        needs: &str,
        lexical: &str,
        semantic: &str,
    ) -> Result<()> {
        match self.evidence {
            Reading::Auto | Reading::Qualified => Ok(()),
            Reading::Lexical => Err(format!("{command} needs {needs}; {lexical}").into()),
            Reading::Semantic => Err(format!(
                "{command} does not answer from the semantic reading; {semantic}"
            )
            .into()),
        }
    }
}
