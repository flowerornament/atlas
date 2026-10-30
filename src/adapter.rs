//! Project declarations from a compiled library adapter or an explicit executable.
//!
//! The host validates custody and references, not the truth of an adapter's interpretation.
mod process;
mod snapshot;
use crate::{Result, architecture::Architecture};
use serde::{Deserialize, Serialize};
pub use snapshot::{InputFile, Snapshot};
use std::{
    io::{Read, Write},
    path::Path,
};

pub const PROTOCOL: u32 = 1;
pub const INTERFACE: &str = "project-declarations/v1";
pub const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REPLY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u32,
    pub interface: String,
    pub source: String,
    pub snapshot: Snapshot,
}
impl Request {
    pub fn new(snapshot: Snapshot, source: String) -> Result<Self> {
        let input = Self {
            protocol: PROTOCOL,
            interface: INTERFACE.into(),
            snapshot,
            source,
        };
        input.validate()?;
        Ok(input)
    }
    pub fn validate(&self) -> Result<()> {
        if self.protocol != PROTOCOL {
            return Err(format!(
                "unsupported adapter protocol {}; expected {PROTOCOL}",
                self.protocol
            )
            .into());
        }
        if self.interface != INTERFACE {
            return Err(format!("unsupported adapter interface {}", self.interface).into());
        }
        self.snapshot.validate()?;
        if !self.snapshot.files.iter().any(|f| f.path == self.source) {
            return Err("declaration source is outside the snapshot".into());
        }
        if self
            .snapshot
            .constants
            .iter()
            .any(|c| c.site.path != self.source)
        {
            return Err("snapshot constants must belong to the declaration source".into());
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Producer {
    pub name: String,
    pub version: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum Outcome {
    Declarations { architecture: Architecture },
    Refused { message: String },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub protocol: u32,
    pub interface: String,
    pub snapshot_id: String,
    pub producer: Producer,
    pub result: Outcome,
}
/// This interface consumes normalized source syntax, never a live project path.
pub trait ProjectAdapter {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn declarations(&self, input: &Request) -> Result<Architecture>;
}
pub fn adapt(adapter: &impl ProjectAdapter, input: &Request) -> Reply {
    let result = match input.validate().and_then(|()| adapter.declarations(input)) {
        Ok(architecture) => Outcome::Declarations { architecture },
        Err(e) => Outcome::Refused {
            message: e.to_string(),
        },
    };
    Reply {
        protocol: PROTOCOL,
        interface: INTERFACE.into(),
        snapshot_id: input.snapshot.id.clone(),
        producer: Producer {
            name: adapter.name().into(),
            version: adapter.version().into(),
        },
        result,
    }
}
/// Serve one bounded JSON request. Diagnostics belong on stderr, never stdout.
pub fn serve(adapter: &impl ProjectAdapter) -> Result<()> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err("adapter request exceeds 16 MiB".into());
    }
    let request: Request = serde_json::from_slice(&bytes)?;
    let bytes = serde_json::to_vec(&adapt(adapter, &request))?;
    if bytes.len() > MAX_REPLY_BYTES {
        return Err("adapter reply exceeds 2 MiB".into());
    }
    std::io::stdout().lock().write_all(&bytes)?;
    Ok(())
}
/// Invoke only the explicitly chosen executable; no shell, PATH search or installation.
#[derive(Debug, Serialize)]
pub struct Invocation {
    pub reply: Reply,
    pub diagnostics: String,
}
pub fn invoke(program: &Path, root: &Path, input: &Request) -> Result<Invocation> {
    input.validate()?;
    input.snapshot.check_bytes(root)?;
    let request = serde_json::to_vec(input)?;
    if request.len() > MAX_REQUEST_BYTES {
        return Err("adapter request exceeds 16 MiB; narrow declaration input".into());
    }
    let output = process::exchange(program, &request)?;
    if !output.status.success() {
        return Err(format!(
            "adapter {} failed: {}",
            program.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let reply: Reply =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("adapter reply JSON: {e}"))?;
    validate_reply_for_input(input, &reply)?;
    input.snapshot.check_bytes(root)?;
    Ok(Invocation {
        reply,
        diagnostics: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
/// Validate a process reply or a directly embedded adapter through the same boundary.
pub fn validate_reply(input: &Request, reply: &Reply) -> Result<()> {
    input.validate()?;
    validate_reply_for_input(input, reply)
}
fn validate_reply_for_input(input: &Request, reply: &Reply) -> Result<()> {
    if reply.protocol != PROTOCOL || reply.interface != INTERFACE {
        return Err("incompatible adapter reply protocol/interface".into());
    }
    if reply.snapshot_id != input.snapshot.id {
        return Err("adapter reply names a different source snapshot".into());
    }
    if reply.producer.name.trim().is_empty() || reply.producer.version.trim().is_empty() {
        return Err("adapter must identify its producer and version".into());
    }
    if let Outcome::Declarations { architecture } = &reply.result {
        if architecture.provenance.is_empty() {
            return Err("adapter declarations need source witnesses".into());
        }
        for id in &architecture.provenance {
            if input
                .snapshot
                .constants
                .iter()
                .filter(|c| c.id() == *id)
                .count()
                != 1
            {
                return Err(format!("adapter source witness is absent or ambiguous: {id}").into());
            }
        }
    }
    Ok(())
}
impl Reply {
    pub fn architecture(&self) -> Result<&Architecture> {
        match &self.result {
            Outcome::Declarations { architecture } => Ok(architecture),
            Outcome::Refused { message } => {
                Err(format!("{} refused: {message}", self.producer.name).into())
            }
        }
    }
}
