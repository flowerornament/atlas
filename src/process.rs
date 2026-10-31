//! One owned child exchange: concurrent pipes, bounded reads, cleanup on failure.
//! Limits belong to each protocol. This bounds I/O, not total execution time;
//! explicitly invoked programs must close their pipes and not delegate them.
use crate::Result;
use std::{
    io::{self, Read, Write},
    process::{Child, Command, Output, Stdio},
    sync::mpsc::{self, Sender},
    thread,
};

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub stdout: usize,
    pub stderr: usize,
}

/// Child failure and failure to deliver its request are independent observations.
pub(crate) struct Exchange {
    pub output: Output,
    pub input_error: Option<io::Error>,
}
#[derive(PartialEq)]
enum Completion {
    Done,
    Failed,
}

/// Drop also covers setup, thread creation, read, write and join failures.
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn bounded(input: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let count = u64::try_from(limit)
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| io::Error::other("invalid stream limit"))?;
    let mut bytes = Vec::new();
    input.take(count).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::other(format!("stream exceeds {limit} bytes")));
    }
    Ok(bytes)
}

fn completed<T>(sender: &Sender<Completion>, result: &io::Result<T>) {
    // A channel disconnect means the owner is already cleaning up.
    let _ = sender.send(if result.is_ok() {
        Completion::Done
    } else {
        Completion::Failed
    });
}

pub(crate) fn exchange(mut command: Command, request: &[u8], limits: Limits) -> Result<Exchange> {
    let running = Running(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    thread::scope(move |scope| {
        // This guard drops before scope joins workers, unblocking them on error.
        let mut running = running;
        let mut stdin = running.0.stdin.take().ok_or("child stdin unavailable")?;
        let stdout = running.0.stdout.take().ok_or("child stdout unavailable")?;
        let stderr = running.0.stderr.take().ok_or("child stderr unavailable")?;
        let (sender, receiver) = mpsc::channel();
        let sent = sender.clone();
        let writer = thread::Builder::new().spawn_scoped(scope, move || {
            let result = stdin.write_all(request);
            drop(stdin);
            completed(&sent, &result);
            result
        })?;
        let sent = sender.clone();
        let out = thread::Builder::new().spawn_scoped(scope, move || {
            let result = bounded(stdout, limits.stdout);
            completed(&sent, &result);
            result
        })?;
        let err = thread::Builder::new().spawn_scoped(scope, move || {
            let result = bounded(stderr, limits.stderr);
            completed(&sender, &result);
            result
        })?;
        // On the first failure, stop the child before joining its pipes.
        // Completion events avoid polling or waiting on a child with full pipes.
        for completion in receiver {
            if completion == Completion::Failed {
                // Retain any already-observed exit; otherwise stop the child.
                // Still collect bounded stderr so an early refusal is not lost.
                if running.0.try_wait()?.is_none() {
                    let _ = running.0.kill();
                }
                break;
            }
        }
        let input_error = writer
            .join()
            .map_err(|_| "child input writer failed")?
            .err();
        let stdout = out
            .join()
            .map_err(|_| "child output reader failed")?
            .map_err(|e| format!("stdout: {e}"))?;
        let stderr = err
            .join()
            .map_err(|_| "child diagnostic reader failed")?
            .map_err(|e| format!("diagnostics: {e}"))?;
        let status = running.0.wait()?;
        Ok(Exchange {
            output: Output {
                status,
                stdout,
                stderr,
            },
            input_error,
        })
    })
}
