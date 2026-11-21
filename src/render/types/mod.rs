//! Present contracts, retained fields, producers and shared preparation routes.
//! Each view keeps its own visual grammar and source evidence; rendering does not inspect source.
mod ascii;
mod contracts;
mod preparation;
mod producers;
mod retention;
pub fn ascii(v: &Views, panel: Panel) -> String {
    ascii::render(v, panel)
}
use crate::types::{Panel, Reference, Views};
use crate::{
    Result,
    render::{
        Artifact, Document, Pele,
        pele::{dimensions, html_text as xml, label, render_source_named},
        theme::Palette,
    },
};
use std::{collections::BTreeMap, fmt::Write};
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
fn style(p: &Palette) -> String {
    format!(
        "{}{}{}linkStyle default stroke:{},stroke-width:1.5px\n",
        p.object.def("object", ",font-size:24px,font-weight:bold"),
        p.operation.def("operation", ",font-size:19px"),
        p.detail.def("detail", ",font-size:16px"),
        p.link
    )
}
fn head() -> String {
    "%%{init: {\"flowchart\":{\"curve\":\"basis\",\"nodeSpacing\":22,\"rankSpacing\":46,\"wrappingWidth\":220}}}%%\nflowchart LR\n".into()
}

/// Compose native Pele panels into one SVG. No source edges are added by composition.
/// With `options.bare`, only the panels are drawn: no title, notes or panel headings.
pub fn pele(v: &Views, panel: Panel, options: &Pele) -> Result<Artifact> {
    render(v, panel, false, options.theme.palette(), options.bare)
}
pub(crate) fn vertical(v: &Views, panel: Panel, palette: &Palette, bare: bool) -> Result<Artifact> {
    render(v, panel, true, palette, bare)
}
fn render(
    v: &Views,
    panel: Panel,
    vertical: bool,
    palette: &Palette,
    bare: bool,
) -> Result<Artifact> {
    let sources = panel_sources(v, panel, palette)?;
    let mut panels = Vec::new();
    let mut mmd = String::new();
    for (index, (title, (source, edges))) in sources.into_iter().enumerate() {
        let source = if vertical {
            source.replacen("flowchart LR", "flowchart TB", 1)
        } else {
            source
        };
        writeln!(mmd, "%% {title}\n{source}").expect("formatting into a String cannot fail");
        let svg = render_source_named(&source, &format!("atlas-representation-{index}"), palette)?;
        let svg = contracts::provenance(&svg, &edges)?;
        let svg = provenance(v, &svg);
        let (w, h) = dimensions(&svg)?;
        panels.push((title, svg, w, h));
    }
    let layout = Layout::of(&panels, matches!(panel, Panel::All), bare);
    let (width, height) = (layout.width, layout.height);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" role=\"img\"><title>{}</title><rect width=\"100%\" height=\"100%\" fill=\"{}\"/>",
        xml(&v.query.title),
        palette.background
    );
    if !bare {
        write_headings(&mut out, v, panel, palette);
    }
    for (i, (title, svg, _, _)) in panels.iter().enumerate() {
        let (x, y) = layout.origin(i, &panels);
        if !bare {
            caption(&mut out, palette, (x, y - 20.), 23, title);
        }
        write!(out, "<g transform=\"translate({x},{y})\">{svg}</g>")
            .expect("formatting into a String cannot fail");
    }
    if !bare {
        write_footer(&mut out, panel, palette, height);
    }
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

/// A panel's heading, its Pele source and the contract edges in that source.
type PanelSource = (&'static str, (String, Vec<contracts::Edge>));

/// The titled Pele sources the selected panel draws, with any contract edges
/// whose provenance must be attached after rendering.
fn panel_sources(v: &Views, panel: Panel, palette: &Palette) -> Result<Vec<PanelSource>> {
    Ok(match panel {
        Panel::All => vec![
            ("A  Representation contracts", contracts::source(v, palette)),
            (
                "B  Observed returners and literal construction",
                (producers::source(v, palette), vec![]),
            ),
            (
                "C  Preparation returners and producer calls",
                (preparation::source(v, palette), vec![]),
            ),
        ],
        Panel::Contracts => vec![("A  Representation contracts", contracts::source(v, palette))],
        Panel::Producers => vec![(
            "B  Observed returners and literal construction",
            (producers::source(v, palette), vec![]),
        )],
        Panel::Retention => vec![(
            "B  Direct field retention",
            (retention::source(v, palette)?, vec![]),
        )],
        Panel::Preparation => vec![(
            "C  Preparation returners and producer calls",
            (preparation::source(v, palette), vec![]),
        )],
    })
}

/// Page geometry. Panel 0 spans the top; with every panel drawn, panels 1
/// and 2 sit side by side below it. A bare page keeps only a small margin.
struct Layout {
    margin: f64,
    top: f64,
    gap: f64,
    width: f64,
    height: f64,
}

impl Layout {
    fn of(panels: &[(&str, String, f64, f64)], all: bool, bare: bool) -> Self {
        let (margin, top, gap) = if bare {
            (20., 20., 40.)
        } else {
            (40., 205., 85.)
        };
        let lower = if all {
            panels[1].3.max(panels[2].3)
        } else {
            0.
        };
        let content = if all {
            panels[0].2.max(panels[1].2 + panels[2].2 + 40.)
        } else {
            panels[0].2
        };
        let (width, height) = if bare {
            (
                content + 2. * margin,
                top + panels[0].3 + if all { gap + lower } else { 0. } + margin,
            )
        } else {
            (
                (content + 2. * margin).max(1500.),
                panels[0].3 + if all { lower + 440. } else { 320. },
            )
        };
        Self {
            margin,
            top,
            gap,
            width,
            height,
        }
    }

    /// The top-left corner of panel `i`.
    fn origin(&self, i: usize, panels: &[(&str, String, f64, f64)]) -> (f64, f64) {
        if i == 0 {
            (self.margin, self.top)
        } else {
            (
                if i == 1 {
                    self.margin
                } else {
                    self.margin + panels[1].2 + 40.
                },
                self.top + panels[0].3 + self.gap,
            )
        }
    }
}

/// One line of page text. A bare page carries none, so callers skip it there.
fn caption(out: &mut String, palette: &Palette, (x, y): (f64, f64), size: u32, s: &str) {
    write!(
        out,
        "<text x=\"{x}\" y=\"{y}\" font-family=\"system-ui,sans-serif\" font-size=\"{size}\" fill=\"{}\">{}</text>",
        palette.heading,
        xml(s),
    ).expect("formatting into a String cannot fail");
}

/// The title and what the selected panel's shapes do and do not assert.
fn write_headings(out: &mut String, v: &Views, panel: Panel, palette: &Palette) {
    caption(out, palette, (40., 48.), 32, &v.query.title);
    caption(
        out,
        palette,
        (40., 82.),
        18,
        if matches!(panel, Panel::Preparation) {
            "Direct resolved producer calls. Boxes are functions, grouped by physical source file."
        } else {
            "Source contracts and construction observations; shared types do not assert shared runtime values or execution order."
        },
    );
    caption(
        out,
        palette,
        (40., 112.),
        17,
        if matches!(panel, Panel::Retention) {
            "Matrix: row contains column. & = reference; * = nested type; hover for the complete field expression."
        } else if matches!(panel, Panel::Preparation) {
            "A shared callee is one source implementation; it does not assert shared runtime values or execution order."
        } else {
            "Return contracts and literal construction are distinct observations. Repeated routes prompt reading; they do not prove duplication."
        },
    );
    caption(
        out,
        palette,
        (40., 140.),
        16,
        &format!(
            "{} unresolved type spellings (includes external/primitive) and {} calls in selected items; not proved absent. JSON retains details.",
            v.unknown_types.len(),
            v.unknown_calls.len()
        ),
    );
}

/// The evidence limits under the panels, anchored to the page's bottom edge.
fn write_footer(out: &mut String, panel: Panel, palette: &Palette, height: f64) {
    caption(
        out,
        palette,
        (40., height - 58.),
        16,
        if matches!(panel, Panel::Preparation) {
            "Call-site locations witness each edge. This is a selected call view, not a complete call graph or evaluation trace."
        } else {
            "Creation: literals and explicit Type::default() calls are separate. Default dispatch, Clone and macro construction may be unknown."
        },
    );
    caption(
        out,
        palette,
        (40., height - 30.),
        16,
        "Preparation shows direct producer calls; repeated patterns are observations, not a semantic duplication verdict.",
    );
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
