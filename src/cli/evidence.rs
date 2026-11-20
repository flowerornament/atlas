//! `--evidence`, reserved in 0.19 (spec §3.9): it names the reading a
//! command answers from. `auto` is the command's current reading and
//! `qualified` names it; `lexical` and `semantic` are refused with what to
//! run instead, until a command offers them.
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
    /// A rust-analyzer (SCIP) reading; arrives in 0.20.
    Semantic,
}
#[derive(Args)]
pub(crate) struct Evidence {
    /// The reading to answer from: auto (the default) and qualified are this
    /// command's reading; lexical and semantic are refused, with what to run.
    #[arg(long, value_enum, default_value_t)]
    evidence: Reading,
}
impl Evidence {
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
            Reading::Semantic => {
                Err(format!("semantic readings in {command} arrive in 0.20; {semantic}").into())
            }
        }
    }
}
