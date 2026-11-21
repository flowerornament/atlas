//! `--with-tests`: the test population beside production code. A command
//! that offers it reads test targets' files (and the modules they declare),
//! test modules and test-gated items too, and marks each row it reads from
//! them `test_only`; without it the population stays production code.
use atlas_lib::population::TestSelection;
use clap::Args;

#[derive(Args)]
pub(crate) struct Tests {
    /// Read test code too: test targets' files and the modules they declare,
    /// `#[cfg(test)]` modules and test-gated items. Each row from them is
    /// marked test_only.
    #[arg(long)]
    with_tests: bool,
}
impl Tests {
    pub(crate) fn asked(&self) -> bool {
        self.with_tests
    }
    pub(crate) fn selection(&self) -> TestSelection {
        if self.with_tests {
            TestSelection::All
        } else {
            TestSelection::Production
        }
    }
}
