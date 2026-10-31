//! A default question derived from declared entry APIs, with observed contracts kept independent.
use crate::{
    Result,
    architecture::{Architecture, Kind},
    representations::{self, Facts, Query, Role, Selector, Views},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
fn success(r: &representations::Reference) -> bool {
    r.role == Role::Output && !r.borrow && !r.wrappers.iter().any(|w| w == "Result[1]")
}
#[derive(Serialize)]
pub struct Overview {
    pub views: Views,
    pub architecture: Architecture,
    pub instruments: Vec<Instrument>,
    pub selection_rule: String,
}
#[derive(Serialize)]
pub struct Instrument {
    pub name: String,
    pub path: String,
    pub calls: Vec<InstrumentCall>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstrumentCall {
    pub from: String,
    pub to: String,
    pub file: String,
    pub line: usize,
}
/// Entries return the main forms; surface methods enter the picture when they
/// produce a form another stage accepts, or construct the declared entry type.
/// Metadata/error types are not promoted merely because a surface lists them.
pub fn project(facts: &Facts, architecture: &Architecture) -> Result<Overview> {
    let candidates: Vec<_> = facts
        .functions
        .iter()
        .filter_map(|f| {
            let m = architecture.owner(&f.file)?;
            if m.kind != Kind::Stage {
                return None;
            }
            let owner = f.owner_name();
            ((owner.is_none() && f.name == m.entry)
                || (f.public
                    && owner
                        .as_deref()
                        .is_some_and(|o| o == m.entry || m.types.iter().any(|s| s == o))))
            .then_some((f, m))
        })
        .collect();
    let by_fn: BTreeMap<_, _> = candidates
        .iter()
        .map(|(f, m)| (f.id.as_str(), *m))
        .collect();
    let mut operations = Vec::new();
    let mut type_ids = BTreeSet::new();
    for (f, m) in candidates {
        let outputs: Vec<_> = facts
            .references
            .iter()
            .filter(|r| r.from == f.id && success(r))
            .collect();
        let primary: Vec<_> = outputs
            .into_iter()
            .filter(|r| {
                let cross = facts.references.iter().any(|input| {
                    input.to == r.to
                        && input.role == Role::Input
                        && by_fn
                            .get(input.from.as_str())
                            .is_some_and(|other| other.name != m.name)
                });
                let constructor = f.name.ends_with("::new")
                    && facts
                        .types
                        .iter()
                        .any(|t| t.id == r.to && t.name == m.entry);
                f.owner_name().is_none() || cross || constructor
            })
            .collect();
        if primary.is_empty() && f.owner_name().is_some() {
            continue;
        }
        operations.push(Selector {
            name: f.name.clone(),
            file: Some(f.file.clone()),
        });
        type_ids.extend(primary.iter().map(|r| r.to.clone()));
    }
    let types: Vec<_> = facts
        .types
        .iter()
        .filter(|t| type_ids.contains(&t.id))
        .map(|t| Selector {
            name: t.name.clone(),
            file: Some(t.file.clone()),
        })
        .collect();
    let query = Query {
        title: "Representation contracts from declared stage APIs".into(),
        types,
        apis: operations,
        preparation_files: vec![],
        preparation_functions: vec![],
    };
    let views = representations::project(facts, &query)?;
    let functions: BTreeMap<_, _> = facts.functions.iter().map(|f| (f.id.as_str(), f)).collect();
    let instruments = architecture
        .modules
        .iter()
        .filter(|m| m.kind == Kind::Instrument)
        .map(|m| {
            let calls: BTreeSet<_> = facts
                .calls
                .iter()
                .filter_map(|c| {
                    let (Some(a), Some(b)) =
                        (functions.get(c.from.as_str()), functions.get(c.to.as_str()))
                    else {
                        return None;
                    };
                    if architecture
                        .owner(&a.file)
                        .is_some_and(|owner| owner.name == m.name)
                        && architecture
                            .owner(&b.file)
                            .is_none_or(|owner| owner.name != m.name)
                    {
                        Some(InstrumentCall {
                            from: a.name.clone(),
                            to: b.name.clone(),
                            file: c.site.path.clone(),
                            line: c.site.line,
                        })
                    } else {
                        None
                    }
                })
                .collect();
            Instrument {
                name: m.name.clone(),
                path: m.path.clone(),
                calls: calls.into_iter().collect(),
            }
        })
        .collect();
    Ok(Overview{views,architecture:architecture.clone(),instruments,selection_rule:"Successful output forms of declared stage entry functions; public surface methods producing forms accepted by other stages or constructing an entry type. Error alternatives and isolated surface metadata do not become main forms. Extra observed literal initializers survive selection. Instrument calls are separate observations; no runtime order inferred.".into()})
}

/// Append a separate native Pele instrument panel; never join call and contract edges.
pub fn pele(overview: &Overview) -> Result<crate::render::Artifact> {
    let mut artifact =
        representations::vertical(&overview.views, representations::Panel::Contracts)?;
    if overview.instruments.is_empty() {
        return Ok(artifact);
    }
    let mut source = format!("block-beta\ncolumns {}\n", overview.instruments.len());
    use std::fmt::Write;
    for (i, instrument) in overview.instruments.iter().enumerate() {
        let targets: BTreeSet<_> = instrument.calls.iter().map(|c| c.to.as_str()).collect();
        let calls = targets
            .into_iter()
            .map(crate::render::pele::label)
            .collect::<Vec<_>>()
            .join("<br/>");
        writeln!(
            source,
            "instrument{i}[\"{}<br/>{}<br/>direct calls:<br/>{}\"]",
            crate::render::pele::label(&instrument.name),
            crate::render::pele::label(&instrument.path),
            if calls.is_empty() {
                "none resolved<br/>see JSON unknowns"
            } else {
                &calls
            }
        )
        .expect("formatting into a String cannot fail");
    }
    source.push_str("classDef default fill:#f5f7f9,stroke:#9ab0c2,color:#24475f,font-size:17px\n");
    let bottom = crate::render::pele::render_source_named(&source, "atlas-instruments")?;
    let main = String::from_utf8(artifact.primary.bytes)?;
    let (w, h) = crate::render::pele::dimensions(&main)?;
    let (bw, bh) = crate::render::pele::dimensions(&bottom)?;
    let width = w.max(bw + 80.);
    let height = h + bh + 120.;
    artifact.primary.bytes=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{main}<text x=\"40\" y=\"{}\" font-family=\"system-ui\" font-size=\"24\" fill=\"#24475f\">Verification instruments: observed direct calls, separate from contracts</text><g transform=\"translate(40,{})\">{bottom}</g></svg>",h+30.,h+60.).into_bytes();
    for d in &mut artifact.companions {
        if d.suffix == "mmd" {
            d.bytes
                .extend_from_slice(format!("\n%% Verification instruments\n{source}").as_bytes());
        }
    }
    Ok(artifact)
}

/// Text of the same contracts and instrument calls as the Pele overview.
pub fn ascii(overview: &Overview) -> String {
    let mut text = representations::ascii(&overview.views, representations::Panel::Contracts);
    text.push_str("\nVERIFICATION INSTRUMENTS (observed direct calls; not pipeline edges)\n");
    for i in &overview.instruments {
        write!(
            text,
            "{} [{}]\n  {}\n",
            i.name,
            i.path,
            i.calls
                .iter()
                .map(|c| format!("{} → {} ({}:{})", c.from, c.to, c.file, c.line))
                .collect::<Vec<_>>()
                .join("\n  ")
        )
        .expect("formatting into a String cannot fail");
    }
    text
}
