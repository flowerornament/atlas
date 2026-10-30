//! Drain all pipes concurrently so large requests/replies cannot deadlock on pipe capacity.
use super::MAX_REPLY_BYTES;
use crate::Result;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
};
fn bounded(mut input: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut overflow = false;
    let mut buffer = [0; 8192];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        if out.len() + n <= limit && !overflow {
            out.extend_from_slice(&buffer[..n]);
        } else {
            overflow = true;
        }
    }
    if overflow {
        return Err(format!("adapter stream exceeds {limit} bytes").into());
    }
    Ok(out)
}
pub(super) fn exchange(program: &Path, request: &[u8]) -> Result<Output> {
    let program = program.canonicalize().map_err(|e| {
        format!(
            "adapter {} unavailable: {e}; install the adapter or pass --adapter PATH",
            program.display()
        )
    })?;
    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("adapter stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("adapter stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("adapter stderr unavailable")?;
    thread::scope(|s| {
        let writer = s.spawn(move || stdin.write_all(request));
        let out = s.spawn(move || bounded(stdout, MAX_REPLY_BYTES));
        let err = s.spawn(move || bounded(stderr, 64 * 1024));
        let status = child.wait();
        let written = writer.join().map_err(|_| "adapter input writer failed")?;
        let stdout = out.join().map_err(|_| "adapter output reader failed")??;
        let stderr = err
            .join()
            .map_err(|_| "adapter diagnostic reader failed")??;
        let status = status?;
        if status.success() {
            written?;
        }
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    })
}
