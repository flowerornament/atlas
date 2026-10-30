//! Returners, literals and explicit default calls occupy separate labelled cells.
use super::{Views, label, shape};
use std::fmt::Write;
fn sites(rows: &[(crate::representations::Function, Vec<usize>)]) -> String {
    rows.iter()
        .map(|(f, lines)| format!("{}()<br/>{}:{:?}", label(&f.name), label(&f.file), lines))
        .collect::<Vec<_>>()
        .join("<br/><br/>")
}
pub(super) fn source(v: &Views) -> String {
    let mut s = "block-beta\ncolumns 3\n".to_owned();
    for (i, p) in v.producers.iter().enumerate() {
        let returns = p
            .returned
            .iter()
            .map(|(f, refs)| {
                format!(
                    "{}()<br/>{}<br/>{}:{}",
                    label(&f.name),
                    label(
                        &refs
                            .iter()
                            .map(shape)
                            .collect::<std::collections::BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    label(&f.file),
                    f.line
                )
            })
            .collect::<Vec<_>>()
            .join("<br/><br/>");
        let literal = sites(&p.constructed);
        let defaults = sites(&p.default_calls);
        writeln!(s,"p{i}[\"{}<br/>{}:{}\"] r{i}[\"Returned by<br/><br/>{}\"] l{i}[\"Observed immediate struct literals<br/><br/>{}<br/><br/>Explicit Type::default() calls<br/><br/>{}\"]",
            label(&p.representation.name),label(&p.representation.file),p.representation.line,
            if returns.is_empty(){"none observed"}else{&returns},
            if literal.is_empty(){"none observed"}else{&literal},
            if defaults.is_empty(){"none observed"}else{&defaults}).expect("formatting into a String cannot fail");
    }
    s.push_str("classDef default fill:#f5f7f9,stroke:#9ab0c2,color:#24475f,font-size:17px\n");
    s
}
