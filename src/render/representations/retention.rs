//! Stable sorted matrix of direct named-field syntax.
use super::{Result, Views, label};
use std::fmt::Write;
fn cell(s: &str) -> String {
    // Reserve two lines in every matrix cell: sparse additions don't move its coordinates.
    let chars: Vec<_> = s.chars().collect();
    let mut lines: Vec<_> = chars
        .chunks(14)
        .map(|c| c.iter().collect::<String>())
        .collect();
    while lines.len() < 2 {
        lines.push("\u{200b}".into());
    }
    lines
        .into_iter()
        .map(|s| label(&s))
        .collect::<Vec<_>>()
        .join("<br/>")
}
pub(super) fn source(v: &Views) -> Result<String> {
    let mut s = format!(
        "block-beta\ncolumns {}\ncorner[\"{}\"]\n",
        v.types.len() + 1,
        cell("row contains →")
    );
    for (i, t) in v.types.iter().enumerate() {
        writeln!(s, "bcol{i}[\"{}\"]", cell(&t.name))
            .expect("formatting into a String cannot fail");
    }
    let mut headers = vec!["corner".to_owned()];
    headers.extend((0..v.types.len()).map(|i| format!("bcol{i}")));
    for (i, t) in v.types.iter().enumerate() {
        let id = format!("brow{i}");
        headers.push(id.clone());
        writeln!(s, "{id}[\"{}\"]", cell(&t.name)).expect("formatting into a String cannot fail");
        for (j, target) in v.types.iter().enumerate() {
            let fields: Vec<_> = v
                .retention
                .iter()
                .filter(|r| r.from == t.id && r.to == target.id)
                .map(|r| {
                    if r.wrappers.is_empty() {
                        format!("{}{}", if r.borrow { "&" } else { "" }, r.slot)
                    } else {
                        format!("{}{}*", if r.borrow { "&" } else { "" }, r.slot)
                    }
                })
                .collect();
            let value = if fields.is_empty() {
                "·".to_owned()
            } else {
                fields.join(", ")
            };
            if value.chars().count() > 28 {
                return Err("retention cell exceeds two lines; narrow selected types or use ASCII/JSON for full field expressions".into());
            }
            writeln!(s, "cell{i}_{j}[\"{}\"]", cell(&value))
                .expect("formatting into a String cannot fail");
            writeln!(
                s,
                "class cell{i}_{j} {}",
                if fields.is_empty() { "empty" } else { "stored" }
            )
            .expect("formatting into a String cannot fail");
        }
    }
    s.push_str("classDef header fill:#e7f0f8,stroke:#9ab0c2,color:#24475f,font-size:18px\nclassDef empty fill:#ffffff,stroke:#dce5ec,color:#b7c5d0,font-size:17px\nclassDef stored fill:#e4edf5,stroke:#698ca7,color:#183e59,font-size:18px\n");
    writeln!(s, "class {} header", headers.join(","))
        .expect("formatting into a String cannot fail");
    Ok(s)
}
