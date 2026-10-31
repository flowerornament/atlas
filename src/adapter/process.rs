//! Explicit adapter executable identity and declaration-protocol stream limits.
use super::MAX_REPLY_BYTES;
use crate::{
    Result,
    process::{self, Limits},
};
use std::{path::Path, process::Command};

pub(super) fn exchange(program: &Path, request: &[u8]) -> Result<process::Exchange> {
    let program = program.canonicalize().map_err(|e| {
        format!(
            "adapter {} unavailable: {e}; install the adapter or pass --adapter PATH",
            program.display()
        )
    })?;
    process::exchange(
        Command::new(program),
        request,
        Limits {
            stdout: MAX_REPLY_BYTES,
            stderr: 64 * 1024,
        },
    )
    .map_err(|e| format!("adapter transport: {e}").into())
}
