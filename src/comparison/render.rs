//! Comparison is an aligned table, not an edgeless graph of unrelated changes.
use super::Comparison;
use crate::{
    Result,
    render::{Artifact, Document, pele},
};
use std::fmt::Write;

fn lines(value: &str) -> String {
    // Break at source-token separators, never in the middle of a filename or identifier.
    let mut out = Vec::new();
    for line in value.lines() {
        let mut row = String::new();
        for token in line.split_inclusive([' ', '/', ':', ',', '{', '}', '[', ']']) {
            if !row.is_empty() && row.chars().count() + token.chars().count() > 56 {
                out.push(pele::label(&row));
                row.clear();
            }
            row.push_str(token);
        }
        out.push(pele::label(&row));
    }
    out.join("<br/>")
}
fn value<'a>(c: &super::Change, v: Option<&'a str>) -> &'a str {
    match v {
        None => "not observed",
        Some("") if c.category == "preparation" => "(empty direct producer-call sequence)",
        Some(s) => s,
    }
}

pub(super) fn pele(c: &Comparison) -> Result<Artifact> {
    if c.changes.len() > 40 {
        return Err(
            "more than 40 changes; narrow --query or use ASCII/JSON (nothing truncated)".into(),
        );
    }
    let mut source = "block-beta\ncolumns 3\n".to_owned();
    if c.changes.is_empty() {
        source.push_str("unchanged[\"No changed observations in this question\"]:3\n");
    }
    for (i, change) in c.changes.iter().enumerate() {
        let status = match (&change.before, &change.after) {
            (None, _) => "ADDED",
            (_, None) => "REMOVED",
            _ => "CHANGED",
        };
        writeln!(
            source,
            "s{i}[\"{status} · {}<br/>{}\"] b{i}[\"Before<br/>{}\"] a{i}[\"After<br/>{}\"]",
            pele::label(&change.category),
            lines(&change.subject),
            lines(value(change, change.before.as_deref())),
            lines(value(change, change.after.as_deref()))
        )?;
        if change.before.is_some() {
            writeln!(source, "class b{i} before")?;
        }
        if change.after.is_some() {
            writeln!(source, "class a{i} after")?;
        }
    }
    source.push_str("classDef default fill:#f5f7f9,stroke:#b4c3cf,color:#24475f,font-size:17px\nclassDef before fill:#fff2ee,stroke:#b67b68,color:#59382e\nclassDef after fill:#eef7f1,stroke:#6d9c7e,color:#284b35\n");
    let svg = pele::render_source_named(&source, "atlas-comparison")?;
    let (w, h) = pele::dimensions(&svg)?;
    let width = (w + 80.).max(1500.);
    let height = h + 350.;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" role=\"img\"><title>Source observation comparison</title><rect width=\"100%\" height=\"100%\" fill=\"white\"/>"
    );
    for (y, size, text) in [
        (45,30,"Source observation comparison".into()),
        (80,17,format!("Before: {}",c.before)),
        (108,17,format!("After: {}",c.after)),
        (140,17,format!("{} changed observations; rows can describe the same source edit. Unchanged observations omitted.",c.changes.len())),
        (170,16,"Physical file + qualified symbol; moves are removal/addition, not inferred renames. No semantic equivalence claim.".into()),
    ] {
        writeln!(out,"<text x=\"40\" y=\"{y}\" font-family=\"system-ui,sans-serif\" font-size=\"{size}\" fill=\"#24475f\">{}</text>",pele::html_text(&text))?;
    }
    write!(out, "<g transform=\"translate(40,210)\">{svg}</g>")?;
    if let Some(unknowns) = c
        .notes
        .iter()
        .find(|n| n.starts_with("Unknown observations:"))
    {
        write!(
            out,
            "<text x=\"40\" y=\"{}\" font-family=\"system-ui,sans-serif\" font-size=\"16\" fill=\"#24475f\">{}</text>",
            height - 35.,
            pele::html_text(unknowns)
        )?;
    }
    out.push_str("</svg>");
    Ok(Artifact {
        primary: Document {
            suffix: "svg",
            bytes: out.into_bytes(),
        },
        companions: vec![Document {
            suffix: "mmd",
            bytes: source.into_bytes(),
        }],
    })
}
