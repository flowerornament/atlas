//! Comparable rows of observed direct producer calls.
use super::{STYLE, Views, head, label, name};
use std::fmt::Write;
pub(super) fn source(v: &Views) -> String {
    let mut s = head();
    if v.preparation.is_empty() {
        s.push_str("cempty[\"No returners in selected preparation files\"]:::detail\n");
    }
    for (i, row) in v.preparation.iter().enumerate() {
        let outputs = row
            .outputs
            .iter()
            .map(|r| format!("{} ({})", name(v, &r.to), r.wrappers.join(" / ")))
            .collect::<Vec<_>>()
            .join(", ");
        let calls = row
            .calls
            .iter()
            .map(|(_, f)| format!("{}()", f.name))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(s,"c{i}[\"{}()<br/>returns {}\"]:::operation\nd{i}[\"{}\"]:::detail\nc{i} -->|\"calls\"| d{i}",label(&row.function.name),label(&outputs),label(if calls.is_empty(){"no resolved producer calls"}else{&calls})).expect("formatting into a String cannot fail");
    }
    s.push_str(STYLE);
    s
}
