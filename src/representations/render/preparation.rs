//! Shared producer identities expose reuse without asserting shared runtime values.
use super::{STYLE, Views, head, label};
use crate::representations::Function;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

pub(super) fn functions(v: &Views) -> Vec<&Function> {
    let mut functions = BTreeMap::new();
    for row in &v.preparation {
        for function in std::iter::once(&row.function).chain(row.calls.iter().map(|(_, f)| f)) {
            functions.insert((&function.file, function.line, &function.id), function);
        }
    }
    functions.into_values().collect()
}
pub(super) fn source(v: &Views) -> String {
    let mut s = head();
    let functions = functions(v);
    let ids: BTreeMap<_, _> = functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id.as_str(), format!("prep{i}")))
        .collect();
    let mut groups = BTreeMap::<&str, Vec<String>>::new();
    for function in &functions {
        let output = v
            .preparation
            .iter()
            .find(|row| row.function.id == function.id)
            .map(|row| {
                row.outputs
                    .iter()
                    .map(|r| r.expression.as_str())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            });
        groups.entry(&function.file).or_default().push(format!(
            "{}[\"{}(){}\"]:::operation",
            ids[function.id.as_str()],
            label(&function.name),
            output.map_or_else(String::new, |s| format!("<br/>returns {}", label(&s)))
        ));
    }
    for (i, (file, nodes)) in groups.into_iter().enumerate() {
        writeln!(
            s,
            "subgraph pf{i}[\"{}\"]\n{}\nend",
            label(file),
            nodes.join("\n")
        )
        .expect("formatting into a String cannot fail");
    }
    for row in &v.preparation {
        for (call, target) in &row.calls {
            writeln!(
                s,
                "{} -->|\"calls @ {}:{}\"| {}",
                ids[row.function.id.as_str()],
                call.site.line,
                call.site.column,
                ids[target.id.as_str()]
            )
            .expect("formatting into a String cannot fail");
        }
    }
    if functions.is_empty() {
        s.push_str("cempty[\"No returners in selected preparation files\"]:::detail\n");
    }
    s.push_str(STYLE);
    s
}
