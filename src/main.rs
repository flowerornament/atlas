//! Atlas process boundary: run the CLI and turn its error into a diagnostic and exit.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the CLI writes results and diagnostics at the process boundary"
)]
mod cli;
fn main() {
    if let Err(error) = cli::run() {
        eprintln!("atlas: {error}");
        std::process::exit(1);
    }
}
