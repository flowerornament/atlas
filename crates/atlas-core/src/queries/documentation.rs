//! Documentation observations with caller-supplied policy; no project citation vocabulary.
use crate::{
    Result,
    architecture::contains,
    discovery::{Snapshot, resolve_scope},
    population::Population,
    source::ItemKind,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderPopulation {
    AllFiles,
    OutsideTestFiles,
}
/// Which lexical //! sites a header rule considers.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderScope {
    File,
    AnyModule,
}
impl std::str::FromStr for HeaderScope {
    type Err = String;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "file" => Ok(Self::File),
            "any-module" => Ok(Self::AnyModule),
            _ => Err(format!(
                "unknown header scope `{value}`; choose file or any-module"
            )),
        }
    }
}

pub struct Policy<'a> {
    pub items: Population,
    pub headers: HeaderPopulation,
    pub header_scope: HeaderScope,
    pub item_kinds: &'a [ItemKind],
    /// None means citation checking was not requested, not that citations were found.
    pub citation_markers: Option<&'a [&'a str]>,
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Finding {
    MissingHeader {
        file: String,
    },
    UncitedHeader {
        file: String,
        lines: Vec<usize>,
    },
    MissingOuterLineDoc {
        file: String,
        name: String,
        line: usize,
    },
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub scope: String,
    pub population: Population,
    pub headers: HeaderPopulation,
    pub header_scope: HeaderScope,
    pub item_kinds: Vec<String>,
    pub citation_markers: Option<Vec<String>>,
    pub checked_files: usize,
    pub checked_items: usize,
    pub findings: Vec<Finding>,
    pub limits: Vec<&'static str>,
    pub sources: std::collections::BTreeMap<String, String>,
}
/// Audit captured facts. Unknown scopes refuse; an unrequested citation rule is explicit.
pub fn audit(snapshot: &Snapshot, scope: &str, policy: &Policy<'_>) -> Result<Report> {
    if policy
        .citation_markers
        .is_some_and(|markers| markers.is_empty() || markers.iter().any(|marker| marker.is_empty()))
    {
        return Err("citation policy must contain nonempty markers; use None when citation checking is not requested".into());
    }
    let scope = resolve_scope(snapshot.graph(), Some(scope))?;
    let mut report = Report {
        schema: 1,
        scope,
        population: policy.items,
        headers: policy.headers,
        header_scope: policy.header_scope,
        item_kinds: policy
            .item_kinds
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect(),
        citation_markers: policy
            .citation_markers
            .map(|markers| markers.iter().map(|marker| (*marker).to_owned()).collect()),
        checked_files: 0,
        checked_items: 0,
        findings: Vec::new(),
        limits: vec![
            "Counts lexical //! and /// line comments, not doc attributes or block doc comments.",
            "File header scope excludes sites inside recognized item spans; macro expansion is not inferred.",
        ],
        sources: std::collections::BTreeMap::new(),
    };
    for file in snapshot
        .graph()
        .physical_files()
        .filter(|file| contains(&report.scope, &file.path))
    {
        report
            .sources
            .insert(file.path.clone(), file.content_fingerprint.clone());
        let source = snapshot
            .source_facts(&file.path)
            .ok_or_else(|| format!("snapshot lacks source facts for {}", file.path))?;
        if matches!(policy.headers, HeaderPopulation::AllFiles) || !file.test_only {
            report.checked_files += 1;
            let header: Vec<_> = source
                .projection()
                .module_header_sites()
                .filter(|(line, _)| {
                    matches!(policy.header_scope, HeaderScope::AnyModule)
                        || !source.outline().iter().any(|item| item.span.contains(line))
                })
                .collect();
            if header.is_empty() {
                report.findings.push(Finding::MissingHeader {
                    file: file.path.clone(),
                });
            } else if policy.citation_markers.is_some_and(|markers| {
                let text = header
                    .iter()
                    .map(|(_, text)| *text)
                    .collect::<Vec<_>>()
                    .join("\n");
                !markers.iter().any(|marker| text.contains(marker))
            }) {
                report.findings.push(Finding::UncitedHeader {
                    file: file.path.clone(),
                    lines: header.iter().map(|(line, _)| *line).collect(),
                });
            }
        }
        for item in source.outline().iter().filter(|item| {
            policy.item_kinds.contains(&item.kind) && policy.items.admits(file, source, item)
        }) {
            report.checked_items += 1;
            if item.doc.is_none() {
                report.findings.push(Finding::MissingOuterLineDoc {
                    file: file.path.clone(),
                    name: item.name.clone(),
                    line: item.line,
                });
            }
        }
    }
    Ok(report)
}
