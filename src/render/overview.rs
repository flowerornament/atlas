//! Present source-declared contracts and instrument calls as separate panels.
use crate::{
    Result,
    overview::Overview,
    render::{Artifact, Document, pele::dimensions, theme::Palette},
    types,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;
/// Append a separate native Pele instrument panel; never join call and contract edges.
/// With `options.bare`, no titles, notes or headings are drawn around the panels.
pub fn pele(overview: &Overview, options: &crate::render::Pele) -> Result<crate::render::Artifact> {
    let (palette, bare) = (options.theme.palette(), options.bare);
    let mut artifact = contracts(overview, palette, bare)?;
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
    source.push_str(&palette.cell.def("default", ",font-size:17px"));
    let bottom = crate::render::pele::render_source_named(&source, "atlas-instruments", palette)?;
    let main = String::from_utf8(artifact.primary.bytes)?;
    let (w, h) = crate::render::pele::dimensions(&main)?;
    let (bw, bh) = crate::render::pele::dimensions(&bottom)?;
    let width = w.max(bw + 80.);
    let height = h + bh + if bare { 40. } else { 120. };
    artifact.primary.bytes = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\"><rect width=\"100%\" height=\"100%\" fill=\"{background}\"/>{main}{heading}<g transform=\"translate(40,{})\">{bottom}</g></svg>",
        h + if bare { 20. } else { 60. },
        background = palette.background,
        heading = if bare {
            String::new()
        } else {
            format!(
                "<text x=\"40\" y=\"{}\" font-family=\"system-ui\" font-size=\"24\" fill=\"{}\">Verification instruments: observed direct calls, separate from contracts</text>",
                h + 30.,
                palette.heading
            )
        },
    ).into_bytes();
    for d in &mut artifact.companions {
        if d.suffix == "mmd" {
            d.bytes
                .extend_from_slice(format!("\n%% Verification instruments\n{source}").as_bytes());
        }
    }
    Ok(artifact)
}

/// Each panel's contract figure, stacked top to bottom in panel order.
fn contracts(overview: &Overview, palette: &Palette, bare: bool) -> Result<Artifact> {
    let mut figures = overview
        .panels
        .iter()
        .map(|panel| {
            crate::render::types::vertical(&panel.views, types::Panel::Contracts, palette, bare)
        })
        .collect::<Result<Vec<_>>>()?;
    if figures.len() == 1 {
        return Ok(figures.remove(0));
    }
    let count = figures.len();
    let (mut body, mut mmd, mut width, mut height) = (String::new(), String::new(), 0f64, 0f64);
    for (number, figure) in figures.into_iter().enumerate() {
        let svg = String::from_utf8(figure.primary.bytes)?;
        let (w, h) = dimensions(&svg)?;
        write!(body, "<g transform=\"translate(0,{height})\">{svg}</g>")?;
        width = width.max(w);
        height += h;
        for document in figure.companions.into_iter().filter(|d| d.suffix == "mmd") {
            writeln!(mmd, "%% Panel {} of {count}", number + 1)?;
            mmd.push_str(&String::from_utf8(document.bytes)?);
        }
    }
    Ok(Artifact {
        primary: Document {
            suffix: "svg",
            bytes: format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\"><rect width=\"100%\" height=\"100%\" fill=\"{}\"/>{body}</svg>",
                palette.background
            )
            .into_bytes(),
        },
        companions: vec![Document {
            suffix: "mmd",
            bytes: mmd.into_bytes(),
        }],
    })
}

/// Text of the same contracts and instrument calls as the Pele overview.
pub fn ascii(overview: &Overview) -> String {
    let count = overview.panels.len();
    let mut text = String::new();
    for (number, panel) in overview.panels.iter().enumerate() {
        if count > 1 {
            writeln!(
                text,
                "PANEL {} OF {count}: {}",
                number + 1,
                panel.modules.join(", ")
            )
            .expect("formatting into a String cannot fail");
        }
        text.push_str(&crate::render::types::ascii(
            &panel.views,
            types::Panel::Contracts,
        ));
        if number + 1 < count {
            text.push('\n');
        }
    }
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
