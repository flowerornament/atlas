//! The official Girard declaration adapter. This executable uses only public Atlas APIs.
#![expect(
    clippy::print_stderr,
    reason = "the adapter executable reports failures on stderr, reserving stdout for its protocol"
)]
use atlas::{
    Result,
    adapter::{self, ProjectAdapter, Request},
    architecture::{Architecture, Kind, Module},
    representations::Literal,
};
use std::collections::BTreeMap;
struct Girard;
fn string(v: &Literal) -> Result<String> {
    v.string()
        .map(str::to_owned)
        .ok_or_else(|| "expected a literal string".into())
}
fn array(v: &Literal) -> Result<&[Literal]> {
    v.array().ok_or_else(|| "expected a literal array".into())
}
fn tuple(v: &Literal, len: usize) -> Result<&[Literal]> {
    let t = v.tuple().ok_or("expected a literal tuple")?;
    if t.len() != len {
        return Err(format!("expected {len} tuple fields, found {}", t.len()).into());
    }
    Ok(t)
}
fn strings(v: &Literal) -> Result<Vec<String>> {
    array(v)?.iter().map(string).collect()
}
impl ProjectAdapter for Girard {
    fn name(&self) -> &'static str {
        "atlas-girard"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn declarations(&self, input: &Request) -> Result<Architecture> {
        let modules = input.snapshot.constant(&input.source, "MODULES")?;
        let surfaces = input.snapshot.constant(&input.source, "SURFACES")?;
        let mut entries = BTreeMap::new();
        for row in array(&surfaces.value)? {
            let t = tuple(row, 3)?;
            let name = string(&t[0])?;
            if entries
                .insert(name, (string(&t[1])?, strings(&t[2])?))
                .is_some()
            {
                return Err("duplicate source surface".into());
            }
        }
        let mut out = Vec::new();
        for row in array(&modules.value)? {
            let t = tuple(row, 4)?;
            let name = string(&t[0])?;
            let kind = match t[2].path().and_then(|p| p.last()).map(String::as_str) {
                Some("Vocabulary") => Kind::Vocabulary,
                Some("SharedLaw") => Kind::SharedLaw,
                Some("Stage") => Kind::Stage,
                Some("Instrument") => Kind::Instrument,
                Some("Edge") => Kind::Edge,
                _ => return Err("unsupported source module layer".into()),
            };
            let path = input.snapshot.module_root(&name)?;
            let (entry, types) = entries.remove(&name).unwrap_or_default();
            if kind == Kind::Stage && entry.is_empty() {
                return Err(format!("stage {name} has no surface").into());
            }
            out.push(Module {
                name,
                kind,
                label: None,
                path,
                entry,
                types,
                dependencies: strings(&t[3])?,
                directory: false,
            });
        }
        if !entries.is_empty() {
            return Err("surface names a module absent from MODULES".into());
        }
        Ok(Architecture {
            schema: 1,
            provenance: vec![modules.id(), surfaces.id()],
            modules: out,
            workflows: vec![],
        })
    }
}
fn main() {
    if let Err(e) = adapter::serve(&Girard) {
        eprintln!("atlas-girard: {e}");
        std::process::exit(1);
    }
}
