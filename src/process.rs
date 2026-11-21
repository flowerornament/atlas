//! One owned child exchange: concurrent pipes, bounded reads, a wall-clock
//! deadline, and cleanup on failure. Limits belong to each protocol. Past the
//! deadline the child is stopped and its pipes get a short grace to close;
//! a process the child handed a pipe to is not waited for, so it cannot hold
//! Atlas, though it may outlive the exchange.
use crate::Result;
use std::{
    io::{self, Read, Write},
    process::{Child, Command, Output, Stdio},
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub stdout: usize,
    pub stderr: usize,
    /// The whole exchange: the request written, both pipes read and the
    /// child exited.
    pub time: Duration,
}

/// How long a stopped child's pipes are given to close.
const GRACE: Duration = Duration::from_secs(2);

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

/// A worker that reports its completion before it returns.
fn worker<T: Send + 'static>(
    sender: Sender<Completion>,
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<JoinHandle<io::Result<T>>> {
    thread::Builder::new().spawn(move || {
        let result = work();
        completed(&sender, &result);
        result
    })
}

/// Stop a child past its deadline and give its pipes a grace to close;
/// the refusal says whether a process it started still holds one.
fn stopped_at_deadline(
    running: &mut Running,
    receiver: &mpsc::Receiver<Completion>,
    mut done: usize,
    limits: Limits,
) -> String {
    let _ = running.0.kill();
    let grace = Instant::now() + GRACE;
    while done < 3 {
        match receiver.recv_timeout(grace.saturating_duration_since(Instant::now())) {
            Ok(_) => done += 1,
            Err(_) => break,
        }
    }
    let held = if done < 3 {
        "; a process it started still holds its output, and is not waited for"
    } else {
        ""
    };
    format!(
        "the program did not finish within {} s and was stopped{held}",
        limits.time.as_secs()
    )
}

pub(crate) fn exchange(mut command: Command, request: &[u8], limits: Limits) -> Result<Exchange> {
    let mut running = Running(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let deadline = Instant::now() + limits.time;
    let mut stdin = running.0.stdin.take().ok_or("child stdin unavailable")?;
    let stdout = running.0.stdout.take().ok_or("child stdout unavailable")?;
    let stderr = running.0.stderr.take().ok_or("child stderr unavailable")?;
    let (sender, receiver) = mpsc::channel();
    let request = request.to_vec();
    let writer = worker(sender.clone(), move || {
        let result = stdin.write_all(&request);
        drop(stdin);
        result
    })?;
    let out = worker(sender.clone(), move || bounded(stdout, limits.stdout))?;
    let err = worker(sender, move || bounded(stderr, limits.stderr))?;
    // On the first failure, stop the child before joining its pipes; past
    // the deadline, stop it and give its pipes a grace to close.
    let mut done = 0;
    while done < 3 {
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Completion::Done) => done += 1,
            Ok(Completion::Failed) => {
                // Retain any already-observed exit; otherwise stop the child.
                // Still collect bounded stderr so an early refusal is not lost.
                if running.0.try_wait()?.is_none() {
                    let _ = running.0.kill();
                }
                break;
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                return Err(stopped_at_deadline(&mut running, &receiver, done, limits).into());
            }
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
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn limits(seconds: u64) -> Limits {
        Limits {
            stdout: 1024,
            stderr: 1024,
            time: Duration::from_secs(seconds),
        }
    }
    fn shell(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn a_stalled_child_is_stopped_at_the_deadline() {
        // The child is the sleeper itself, with no shell: a shell's own
        // child could still be running at the deadline on a loaded host and
        // rightly be reported holding the pipe.
        let mut sleeper = Command::new("sleep");
        sleeper.arg("30");
        let started = Instant::now();
        let error = exchange(sleeper, b"request", limits(1))
            .err()
            .expect("a stalled child is refused")
            .to_string();
        assert!(
            error.contains("did not finish within 1 s and was stopped"),
            "{error}"
        );
        assert!(!error.contains("still holds"), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_process_holding_the_pipe_does_not_hold_the_exchange() {
        let started = Instant::now();
        let error = exchange(
            shell("sleep 6 & cat >/dev/null; exec sleep 30"),
            b"request",
            limits(1),
        )
        .err()
        .expect("a stalled child is refused")
        .to_string();
        assert!(
            error.contains("still holds its output, and is not waited for"),
            "{error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_child_that_answers_in_time_is_read_whole() {
        let exchange = exchange(shell("cat"), b"echoed", limits(10)).expect("answered");
        assert!(exchange.output.status.success());
        assert_eq!(exchange.output.stdout, b"echoed");
    }
}
