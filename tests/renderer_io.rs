//! Hostile pipe behavior at the actual CLI boundary, with a bounded harness.
#![cfg(unix)]
mod support;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use support::Fixture;

fn project() -> Fixture {
    Fixture::new(&[
        ("src/lib.rs", "pub struct Token;\n"),
        (
            "question.json",
            r#"{"title":"fixture","types":[{"name":"Token"}]}"#,
        ),
    ])
}
fn run(fixture: &Fixture, body: &str) -> Output {
    let real = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("query real Node executable");
    assert!(real.status.success());
    let real = String::from_utf8(real.stdout).expect("Node executable path is UTF-8");
    fixture.write(
        "fake/node",
        "#!/bin/sh\nexec \"$ATLAS_REAL_NODE\" \"$ATLAS_NODE_SCRIPT\"\n",
    );
    fs::set_permissions(
        fixture.path().join("fake/node"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("make fake Node executable");
    fixture.write("fake/body.mjs", &format!(
        "import fs from 'node:fs';\nfs.writeFileSync(process.env.ATLAS_NODE_PID, String(process.pid));\n{body}\n"
    ));
    let mut paths = vec![fixture.path().join("fake")];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH for Node fixture"),
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "representations",
            "--query",
            "question.json",
            "--panel",
            "contracts",
            "--backend",
            "pele",
            "--out",
            "result.svg",
            "--root",
        ])
        .arg(fixture.path())
        .env(
            "PATH",
            std::env::join_paths(paths).expect("compose fake Node search path"),
        )
        .env("ATLAS_REAL_NODE", real.trim())
        .env("ATLAS_NODE_SCRIPT", fixture.path().join("fake/body.mjs"))
        .env("ATLAS_NODE_PID", fixture.path().join("fake/pid"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start Atlas CLI fixture");
    let deadline = Instant::now() + Duration::from_secs(20);
    while child
        .try_wait()
        .expect("observe Atlas CLI fixture")
        .is_none()
    {
        if Instant::now() >= deadline {
            child.kill().expect("terminate timed-out Atlas fixture");
            child.wait().expect("reap timed-out Atlas fixture");
            if let Ok(pid) = fs::read_to_string(fixture.path().join("fake/pid")) {
                let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
            }
            panic!("renderer exchange exceeded test harness deadline");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child
        .wait_with_output()
        .expect("collect Atlas fixture result");
    let pid = fs::read_to_string(fixture.path().join("fake/pid")).expect("read renderer PID");
    let alive = Command::new("kill")
        .args(["-0", pid.trim()])
        .output()
        .expect("check renderer process disappearance");
    assert!(
        !alive.status.success(),
        "owned renderer must be reaped: {pid}"
    );
    output
}
fn refuses(body: &str, message: &str) {
    let fixture = project();
    let source = fs::read(fixture.path().join("src/lib.rs")).expect("read fixture source");
    let query = fs::read(fixture.path().join("question.json")).expect("read fixture query");
    let output = run(&fixture, body);
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).expect("CLI diagnostic text");
    assert!(error.contains(message), "{error}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read(fixture.path().join("src/lib.rs")).expect("read fixture source"),
        source
    );
    assert_eq!(
        fs::read(fixture.path().join("question.json")).expect("read fixture query"),
        query
    );
    for path in ["result.svg", "result.json", "result.mmd", "result.html"] {
        assert!(
            !fixture.path().join(path).exists(),
            "failed render published {path}"
        );
    }
    assert!(!fixture.path().join("target/atlas").exists());
}

#[test]
fn oversize_svg_and_diagnostics_refuse_and_reap_without_waiting_for_eof() {
    refuses(
        "fs.writeSync(1, Buffer.alloc(32*1024*1024+1, 120)); setInterval(()=>{}, 1000);",
        "stdout: stream exceeds 33554432 bytes",
    );
    refuses(
        "fs.writeSync(2, Buffer.alloc(65537, 120)); setInterval(()=>{}, 1000);",
        "diagnostics: stream exceeds 65536 bytes",
    );
}
#[test]
fn early_failure_broken_pipe_and_malformed_output_remain_distinct() {
    refuses(
        "fs.writeSync(2, 'startup diagnostic'); process.exit(42);",
        "startup diagnostic",
    );
    refuses(
        "fs.readFileSync(0); fs.writeSync(2, 'deliberate failure'); process.exit(42);",
        "Pele render failed (exit status: 42): deliberate failure",
    );
    refuses(
        "fs.writeSync(1, '<svg></svg>'); process.exit(0);",
        "request write:",
    );
    refuses(
        "fs.closeSync(0); setInterval(()=>{}, 1000);",
        "request write:",
    );
    refuses(
        "fs.readFileSync(0); fs.writeSync(1, Buffer.from([255]));",
        "Pele SVG is not UTF-8:",
    );
    refuses(
        "fs.readFileSync(0); fs.writeSync(1, 'not an SVG');",
        "Pele returned no SVG document",
    );
}
#[test]
fn stdout_and_stderr_are_drained_while_a_large_request_is_written() {
    let fixture = project();
    let output = run(
        &fixture,
        "fs.writeSync(2, Buffer.alloc(60*1024, 120)); fs.writeSync(1, '<svg viewBox=\"0 0 100 100\" data-pipe-control=\"yes\">'+ ' '.repeat(256*1024)); fs.readFileSync(0); fs.writeSync(1, '</svg>');",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = fs::read_to_string(fixture.path().join("result.svg")).unwrap();
    assert!(svg.contains("data-pipe-control"));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.len() > 256 * 1024);
}
