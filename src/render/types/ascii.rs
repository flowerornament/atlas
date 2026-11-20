//! Text representation of the same selected facts.
use super::{Panel, Views, name, shape};
use std::fmt::Write;
/// Text carries the same facts as Pele without pretending ASCII lines are call traces.
pub(super) fn render(v: &Views, panel: Panel) -> String {
    let mut s = format!(
        "{}\nContracts and direct fields, not runtime value flow.\n",
        v.query.title
    );
    for t in &v.types {
        if t.alias.is_some() {
            writeln!(
                s,
                "{} @ {}:{} (written alias; not expanded)",
                t.declaration(),
                t.file,
                t.line
            )
            .expect("formatting into a String cannot fail");
        }
    }
    if matches!(panel, Panel::All | Panel::Contracts) {
        s.push_str("\nA  REPRESENTATION CONTRACTS\n");
        for c in &v.contracts {
            let ins = c
                .inputs
                .iter()
                .map(|r| format!("{} [{}]", name(v, &r.to), shape(r)))
                .collect::<Vec<_>>()
                .join(", ");
            let outs = c
                .outputs
                .iter()
                .map(|r| format!("{} [{}]", name(v, &r.to), shape(r)))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                s,
                "  {}:{}  {}{}\n    ({ins}) -> ({outs})",
                c.function.file,
                c.function.line,
                c.function.name,
                if c.additional_constructor {
                    " [additional literal initializer]"
                } else {
                    ""
                }
            )
            .expect("formatting into a String cannot fail");
            for (t, r) in &c.failures {
                writeln!(
                    s,
                    "    Err: {} [{}] @ {}:{}",
                    t.name,
                    shape(r),
                    t.file,
                    t.line
                )
                .expect("formatting into a String cannot fail");
            }
            for literal in &c.literals {
                writeln!(
                    s,
                    "    literal {} at line {}",
                    name(v, &literal.to),
                    literal.line
                )
                .expect("formatting into a String cannot fail");
            }
        }
    }
    if matches!(panel, Panel::Retention) {
        s.push_str("\nB  DIRECT FIELD RETENTION (row contains column; numbered fields below)\n                  ");
        for i in 0..v.types.len() {
            write!(s, " {:>3}", i + 1).expect("formatting into a String cannot fail");
        }
        s.push('\n');
        for (i, t) in v.types.iter().enumerate() {
            write!(s, " {:>2} {:<14}", i + 1, t.name)
                .expect("formatting into a String cannot fail");
            for target in &v.types {
                let count = v
                    .retention
                    .iter()
                    .filter(|r| r.from == t.id && r.to == target.id)
                    .count();
                write!(
                    s,
                    " {:>3}",
                    if count == 0 {
                        "·".to_owned()
                    } else {
                        count.to_string()
                    }
                )
                .expect("formatting into a String cannot fail");
            }
            s.push('\n');
        }
        for r in &v.retention {
            writeln!(
                s,
                "  {}.{} : {} -> {}",
                name(v, &r.from),
                r.slot,
                r.expression,
                name(v, &r.to)
            )
            .expect("formatting into a String cannot fail");
        }
    }
    if matches!(panel, Panel::All | Panel::Producers) {
        s.push_str("\nB  OBSERVED RETURNERS / IMMEDIATE LITERAL CONSTRUCTION\n");
        for p in &v.producers {
            writeln!(
                s,
                "  {} [{}:{}]",
                p.representation.name, p.representation.file, p.representation.line
            )
            .expect("formatting into a String cannot fail");
            for (f, refs) in &p.returned {
                writeln!(
                    s,
                    "    returns: {} @ {}:{} ({})",
                    f.name,
                    f.file,
                    f.line,
                    refs.iter()
                        .map(|r| r.expression.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .expect("formatting into a String cannot fail");
            }
            for (f, lines) in &p.default_calls {
                writeln!(
                    s,
                    "    explicit Type::default() call: {} @ {} lines {:?}",
                    f.name, f.file, lines
                )
                .expect("formatting into a String cannot fail");
            }
            for (f, lines) in &p.constructed {
                writeln!(s, "    literals: {} @ {} lines {:?}", f.name, f.file, lines)
                    .expect("formatting into a String cannot fail");
            }
        }
    }
    if matches!(panel, Panel::All | Panel::Preparation) {
        s.push_str("\nC  PREPARATION RETURNERS / DIRECT PRODUCER CALLS\n");
        for row in &v.preparation {
            writeln!(
                s,
                "  {}:{}  {} -> {}\n    calls: {}",
                row.function.file,
                row.function.line,
                row.function.name,
                row.outputs
                    .iter()
                    .map(|r| name(v, &r.to))
                    .collect::<Vec<_>>()
                    .join(", "),
                row.calls
                    .iter()
                    .map(|(c, f)| format!("{} @ {}:{}", f.name, c.site.line, c.site.column))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("formatting into a String cannot fail");
        }
        if v.preparation.is_empty() {
            s.push_str("  No returners in selected preparation files.\n");
        }
    }
    writeln!(s,"\nUnknown observations in this selection: {} type spellings (includes external/primitive), {} calls.",v.unknown_types.len(),v.unknown_calls.len()).expect("formatting into a String cannot fail");
    for note in &v.notes {
        writeln!(s, "- {note}").expect("formatting into a String cannot fail");
    }
    s
}
