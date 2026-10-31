//! Native visual grammars: contract graph, retention matrix, preparation rows.
mod ascii;
mod contracts;
mod preparation;
mod producers;
mod retention;
pub fn ascii(v: &Views, panel: Panel) -> String {
    ascii::render(v, panel)
}
use super::{Reference, Views};
use crate::{
    Result,
    render::{
        Artifact, Document,
        pele::{dimensions, html_text as xml, label, render_source_named},
    },
};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Write};
#[derive(Clone, Copy, Debug, Default, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Panel {
    #[default]
    All,
    Contracts,
    Retention,
    Preparation,
    Producers,
}
fn name<'a>(v: &'a Views, id: &str) -> &'a str {
    v.types
        .iter()
        .find(|t| t.id == id)
        .map_or("?", |t| t.name.as_str())
}
fn shape(r: &Reference) -> String {
    let w = r
        .wrappers
        .iter()
        .map(|w| match w.as_str() {
            "Result[0]" => "Ok".into(),
            "Result[1]" => "Err".into(),
            "Vec[0]" => "elements".into(),
            w if w.starts_with("tuple[") => {
                format!(".{}", w.trim_start_matches("tuple[").trim_end_matches(']'))
            }
            w => w.into(),
        })
        .collect::<Vec<String>>()
        .join(" / ");
    format!(
        "{}{}{}",
        if r.borrow { "& " } else { "" },
        r.slot,
        if w.is_empty() {
            String::new()
        } else {
            format!(" · {w}")
        }
    )
}
const STYLE: &str = "classDef object fill:#e7f0f8,stroke:#3d6b8c,color:#183e59,font-size:24px,font-weight:bold\nclassDef operation fill:#ffffff,stroke:#8398a9,color:#253e51,font-size:19px\nclassDef detail fill:#f5f7f9,stroke:#b4c3cf,color:#3d586c,font-size:16px\nlinkStyle default stroke:#567b98,stroke-width:1.5px\n";
fn head() -> String {
    "%%{init: {\"flowchart\":{\"curve\":\"basis\",\"nodeSpacing\":22,\"rankSpacing\":46,\"wrappingWidth\":220}}}%%\nflowchart LR\n".into()
}

/// Compose native Pele panels into one SVG. No source edges are added by composition.
pub fn pele(v: &Views, panel: Panel) -> Result<Artifact> {
    render(v, panel, false)
}
pub(crate) fn vertical(v: &Views, panel: Panel) -> Result<Artifact> {
    render(v, panel, true)
}
fn render(v: &Views, panel: Panel, vertical: bool) -> Result<Artifact> {
    let sources = match panel {
        Panel::All => vec![
            ("A  Representation contracts", contracts::source(v)),
            (
                "B  Observed returners and literal construction",
                (producers::source(v), vec![]),
            ),
            (
                "C  Preparation returners and producer calls",
                (preparation::source(v), vec![]),
            ),
        ],
        Panel::Contracts => vec![("A  Representation contracts", contracts::source(v))],
        Panel::Producers => vec![(
            "B  Observed returners and literal construction",
            (producers::source(v), vec![]),
        )],
        Panel::Retention => vec![("B  Direct field retention", (retention::source(v)?, vec![]))],
        Panel::Preparation => vec![(
            "C  Preparation returners and producer calls",
            (preparation::source(v), vec![]),
        )],
    };
    let mut panels = Vec::new();
    let mut mmd = String::new();
    for (index, (title, (source, edges))) in sources.into_iter().enumerate() {
        let source = if vertical {
            source.replacen("flowchart LR", "flowchart TB", 1)
        } else {
            source
        };
        writeln!(mmd, "%% {title}\n{source}").expect("formatting into a String cannot fail");
        let svg = render_source_named(&source, &format!("atlas-representation-{index}"))?;
        let svg = contracts::provenance(&svg, &edges)?;
        let svg = provenance(v, &svg);
        let (w, h) = dimensions(&svg)?;
        panels.push((title, svg, w, h));
    }
    let all = matches!(panel, Panel::All);
    let width = if all {
        (panels[0].2 + 80.)
            .max(panels[1].2 + panels[2].2 + 120.)
            .max(1500.)
    } else {
        (panels[0].2 + 80.).max(1500.)
    };
    let height = if all {
        panels[0].3 + panels[1].3.max(panels[2].3) + 440.
    } else {
        panels[0].3 + 320.
    };
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" role=\"img\"><title>{}</title><rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
        xml(&v.query.title)
    );
    fn text(out: &mut String, x: f64, y: f64, size: u32, s: &str) {
        write!(out,"<text x=\"{x}\" y=\"{y}\" font-family=\"system-ui,sans-serif\" font-size=\"{size}\" fill=\"#24475f\">{}</text>",xml(s)).expect("formatting into a String cannot fail");
    }
    text(&mut out, 40., 48., 32, &v.query.title);
    text(
        &mut out,
        40.,
        82.,
        18,
        if matches!(panel, Panel::Preparation) {
            "Direct resolved producer calls. Boxes are functions, grouped by physical source file."
        } else {
            "Source contracts and construction observations; shared types do not assert shared runtime values or execution order."
        },
    );
    text(
        &mut out,
        40.,
        112.,
        17,
        if matches!(panel, Panel::Retention) {
            "Matrix: row contains column. & = reference; * = nested type; hover for the complete field expression."
        } else if matches!(panel, Panel::Preparation) {
            "A shared callee is one source implementation; it does not assert shared runtime values or execution order."
        } else {
            "Return contracts and literal construction are distinct observations. Repeated routes prompt reading; they do not prove duplication."
        },
    );
    text(
        &mut out,
        40.,
        140.,
        16,
        &format!(
            "{} unresolved type spellings (includes external/primitive) and {} calls in selected items; not proved absent. JSON retains details.",
            v.unknown_types.len(),
            v.unknown_calls.len()
        ),
    );
    for (i, (title, svg, _, _)) in panels.iter().enumerate() {
        let (x, y) = if i == 0 {
            (40., 205.)
        } else {
            (
                if i == 1 { 40. } else { panels[1].2 + 80. },
                panels[0].3 + 290.,
            )
        };
        text(&mut out, x, y - 20., 23, title);
        write!(out, "<g transform=\"translate({x},{y})\">{svg}</g>")
            .expect("formatting into a String cannot fail");
    }
    text(
        &mut out,
        40.,
        height - 58.,
        16,
        if matches!(panel, Panel::Preparation) {
            "Call-site locations witness each edge. This is a selected call view, not a complete call graph or evaluation trace."
        } else {
            "Creation: literals and explicit Type::default() calls are separate. Default dispatch, Clone and macro construction may be unknown."
        },
    );
    text(
        &mut out,
        40.,
        height - 30.,
        16,
        "Preparation shows direct producer calls; repeated patterns are observations, not a semantic duplication verdict.",
    );
    out.push_str("</svg>");
    Ok(Artifact {
        primary: Document {
            suffix: "svg",
            bytes: out.into_bytes(),
        },
        companions: vec![Document {
            suffix: "mmd",
            bytes: mmd.into_bytes(),
        }],
    })
}

// Add source titles to emitted objects without reconstructing or restyling Pele geometry.
fn provenance(v: &Views, svg: &str) -> String {
    let mut titles = BTreeMap::new();
    for (i, t) in v.types.iter().enumerate() {
        let title = format!(
            "{}:{}\n{}\n{}",
            t.file,
            t.line,
            t.declaration(),
            t.fields
                .iter()
                .map(|(n, t)| format!("{n}: {t}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        for id in [format!("a{i}"), format!("brow{i}"), format!("bcol{i}")] {
            titles.insert(id, title.clone());
        }
    }
    for (i, c) in v.contracts.iter().enumerate() {
        titles.insert(
            format!("op{i}"),
            format!(
                "{}:{}\n{}",
                c.function.file, c.function.line, c.function.signature
            ),
        );
    }
    for (i, function) in preparation::functions(v).iter().enumerate() {
        titles.insert(
            format!("prep{i}"),
            format!(
                "{}:{}\n{}",
                function.file, function.line, function.signature
            ),
        );
    }
    for (i, t) in v.types.iter().enumerate() {
        for (j, target) in v.types.iter().enumerate() {
            let title = v
                .retention
                .iter()
                .filter(|r| r.from == t.id && r.to == target.id)
                .map(|r| {
                    format!(
                        "{}.{}: {} @ {}:{}",
                        t.name, r.slot, r.expression, t.file, t.line
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            titles.insert(format!("cell{i}_{j}"), title);
        }
    }
    let mut out = svg.to_owned();
    for (id, title) in titles {
        let needle = format!("data-id=\"{id}\"");
        if let Some(start) = out.find(&needle)
            && let Some(end) = out[start..].find('>')
        {
            out.insert_str(start + end + 1, &format!("<title>{}</title>", xml(&title)));
        }
    }
    out
}
