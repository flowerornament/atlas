//! API contracts are type-level relations. All literal sites remain in their source titles.
use super::{STYLE, Views, head, label, shape, xml};
use crate::Result;
use std::{collections::BTreeMap, fmt::Write};
pub(super) struct Edge {
    from: String,
    to: String,
    label: String,
    title: String,
}
pub(super) fn source(v: &Views) -> (String, Vec<Edge>) {
    let ids: BTreeMap<_, _> = v
        .types
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), format!("a{i}")))
        .collect();
    let mut groups: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut edges = Vec::new();
    for t in &v.types {
        groups.entry(&t.file).or_default().push(format!(
            "{}([\"{}\"]):::object",
            ids[t.id.as_str()],
            label(&t.declaration())
        ));
    }
    for (i, c) in v.contracts.iter().enumerate() {
        let extra = if c.additional_constructor {
            " · literal initializer"
        } else {
            ""
        };
        let title = format!(
            "{}:{}\n{}\n{}",
            c.function.file,
            c.function.line,
            c.function.signature,
            c.literals
                .iter()
                .map(|l| format!(
                    "literal {} at line {}",
                    v.types
                        .iter()
                        .find(|t| t.id == l.to)
                        .map_or("?", |t| t.name.as_str()),
                    l.line
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut edge = |from: &str, to: &str, label: String| {
            edges.push(Edge {
                from: from.into(),
                to: to.into(),
                label,
                title: title.clone(),
            });
        };
        // A return already shows this type-level relation, not the count/identity of values.
        let local: Vec<_> = c
            .literals
            .iter()
            .filter(|l| !c.outputs.iter().any(|r| r.to == l.to))
            .collect();
        if c.inputs.len() == 1 && !c.outputs.is_empty() && local.is_empty() && c.failures.is_empty()
        {
            for r in &c.outputs {
                edge(
                    &ids[c.inputs[0].to.as_str()],
                    &ids[r.to.as_str()],
                    format!("{}(){} · {}", c.function.name, extra, shape(r)),
                );
            }
        } else {
            let id = format!("op{i}");
            groups.entry(&c.function.file).or_default().push(format!(
                "{id}[\"{}(){extra}\"]:::operation",
                label(&c.function.name)
            ));
            for r in &c.inputs {
                edge(&ids[r.to.as_str()], &id, shape(r));
            }
            for r in &c.outputs {
                edge(&id, &ids[r.to.as_str()], shape(r));
            }
            for (j, (t, r)) in c.failures.iter().enumerate() {
                let failure = format!("err{i}_{j}");
                groups
                    .entry(&t.file)
                    .or_default()
                    .push(format!("{failure}[\"{}\"]:::detail", label(&t.name)));
                edge(&id, &failure, shape(r));
            }
            for l in local {
                edge(&id, &ids[l.to.as_str()], format!("literal at {}", l.line));
            }
        }
    }
    let mut s = head();
    for (i, (file, rows)) in groups.into_iter().enumerate() {
        writeln!(
            s,
            "subgraph af{i}[\"{}\"]\n{}\nend",
            label(file),
            rows.join("\n")
        )
        .expect("formatting into a String cannot fail");
    }
    for e in &edges {
        writeln!(s, "{} -->|\"{}\"| {}", e.from, label(&e.label), e.to)
            .expect("formatting into a String cannot fail");
    }
    s.push_str(STYLE);
    (s, edges)
}
/// Check the pinned renderer's edge order/endpoints before attaching API provenance.
pub(super) fn provenance(svg: &str, edges: &[Edge]) -> Result<String> {
    if edges.is_empty() {
        return Ok(svg.into());
    }
    let needle = "<g class=\"pele-edge\" ";
    let groups: Vec<_> = svg.match_indices(needle).collect();
    if groups.len() != edges.len() {
        return Err("Pele contract edge population changed; refusing mismatched provenance".into());
    }
    let mut out = String::new();
    let mut cursor = 0;
    for ((start, _), edge) in groups.into_iter().zip(edges) {
        let end = start
            + svg[start..]
                .find('>')
                .ok_or("unterminated Pele edge group")?
            + 1;
        let expected = format!("data-id=\"L_{}_{}_", edge.from, edge.to);
        if !svg[start..end].contains(&expected) {
            return Err("Pele contract edge order changed; refusing mismatched provenance".into());
        }
        out.push_str(&svg[cursor..end]);
        write!(out, "<title>{}</title>", xml(&edge.title))
            .expect("formatting into a String cannot fail");
        cursor = end;
    }
    out.push_str(&svg[cursor..]);
    Ok(out)
}
