//! The embedded renderer's request, bounded exchange and SVG protocol checks.
use crate::{
    Result,
    process::{self, Limits},
};
use serde::Serialize;
use std::{
    io::{self, Write},
    process::Command,
};

const MAX_REQUEST: usize = 16 * 1024 * 1024;
const MAX_SVG: usize = 32 * 1024 * 1024;
const MAX_DIAGNOSTICS: usize = 64 * 1024;

#[derive(Serialize)]
struct Request<'a> {
    bundle: &'a str,
    source: &'a str,
    prefix: &'a str,
}
struct RequestBytes(Vec<u8>);
impl Write for RequestBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_REQUEST - self.0.len() {
            return Err(io::Error::other(
                "Pele request exceeds 16 MiB; narrow the view",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn request(source: &str, prefix: &str) -> Result<Vec<u8>> {
    let mut bytes = RequestBytes(Vec::new());
    serde_json::to_writer(
        &mut bytes,
        &Request {
            bundle: include_str!("../../../vendor/pele/pele.js"),
            source,
            prefix,
        },
    )?;
    Ok(bytes.0)
}

pub(super) fn render(source: &str, prefix: &str) -> Result<String> {
    // Serialize within the cap before starting a child. Borrow the inputs;
    // escaped JSON bytes, not source character counts, determine this limit.
    let request = request(source, prefix)?;
    let mut command = Command::new("node");
    command.args([
        "--input-type=module",
        "--eval",
        include_str!("../../../renderer/pele.mjs"),
    ]);
    let exchange = process::exchange(
        command,
        &request,
        Limits {
            stdout: MAX_SVG,
            stderr: MAX_DIAGNOSTICS,
        },
    )
    .map_err(|e| {
        if e.downcast_ref::<io::Error>()
            .is_some_and(|e| e.kind() == io::ErrorKind::NotFound)
        {
            format!("start Pele renderer: {e}; install Node >=20, or use --backend ascii")
        } else {
            format!("Pele transport: {e}")
        }
    })?;
    let output = exchange.output;
    if !output.status.success() {
        return Err(format!(
            "Pele render failed ({}): {}{}; narrow the view for renderer size limits",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
            exchange
                .input_error
                .map_or_else(String::new, |e| format!("; request write: {e}"))
        )
        .into());
    }
    if let Some(error) = exchange.input_error {
        return Err(format!("Pele request write: {error}").into());
    }
    let svg =
        String::from_utf8(output.stdout).map_err(|e| format!("Pele SVG is not UTF-8: {e}"))?;
    let body = svg.trim();
    if !body
        .strip_prefix("<svg")
        .and_then(|tail| tail.chars().next())
        .is_some_and(|c| c.is_whitespace() || c == '>')
        || !body.ends_with("</svg>")
    {
        return Err("Pele returned no SVG document".into());
    }
    Ok(svg)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_limit_counts_escaped_json_and_refuses_before_spawning() {
        let error = render(&"\0".repeat(MAX_REQUEST / 6), "atlas").unwrap_err();
        assert!(error.to_string().contains("Pele request exceeds 16 MiB"));
        let source = "label: café \"quoted\"\n";
        let bytes = request(source, "prefix").unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(wire["source"], source);
        assert_eq!(wire["prefix"], "prefix");
    }
    #[test]
    fn real_embedded_renderer_accepts_a_large_labelled_pipeline() {
        let mut source = "flowchart TB\n".to_owned();
        for i in 0..550 {
            use std::fmt::Write as _;
            writeln!(
                source,
                "n{i}[\"Stage {i}: consume Input{i}; produce Output{i}\"]"
            )
            .unwrap();
            if i > 0 {
                writeln!(source, "n{} -->|Output{}| n{i}", i - 1, i - 1).unwrap();
            }
        }
        let svg = render(&source, "pipeline").unwrap();
        assert!(
            svg.len() > 347_197,
            "large pipeline SVG bytes: {}",
            svg.len()
        );
        assert!(svg.contains("Stage 549"));
        assert!(svg.contains("Output548"));
    }
}
