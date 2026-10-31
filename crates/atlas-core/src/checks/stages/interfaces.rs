//! Apply declared stage interfaces to observed paths and assemble their refusals.
//! Unjudged paths remain evidence gaps; declarations never become observed reads.
use super::{Edge, StageReport, policy};
use policy::StageOrder;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};
/// The selected stage orders with their reports, in policy order.
impl StageReport {
    /// Paths per (reader, read) stage pair, upstream reads only.
    pub fn reads(&self) -> BTreeMap<String, BTreeMap<String, usize>> {
        let mut reads: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for edge in &self.backward {
            *reads
                .entry(edge.from.clone())
                .or_default()
                .entry(edge.to.clone())
                .or_default() += 1;
        }
        reads
    }

    /// The first site of each read the declared interfaces do not list. A
    /// crate without declared interfaces lists nothing and is not judged.
    pub fn undeclared_reads(&self) -> Vec<&Edge> {
        let Some(interfaces) = &self.interfaces else {
            return Vec::new();
        };
        let mut seen = BTreeSet::new();
        self.backward
            .iter()
            .filter(|edge| {
                !interfaces
                    .get(&edge.from)
                    .is_some_and(|declared| declared.admits(&edge.to, &self.floor))
            })
            .filter(|edge| seen.insert((edge.from.clone(), edge.to.clone())))
            .collect()
    }

    /// Declared reads with no path left: each can leave its row.
    pub fn unused_reads(&self) -> Vec<(String, String)> {
        let Some(interfaces) = &self.interfaces else {
            return Vec::new();
        };
        let reads = self.reads();
        interfaces
            .iter()
            .flat_map(|(reader, declared)| {
                declared
                    .reads
                    .iter()
                    .chain(&declared.input)
                    .map(String::as_str)
                    .chain(declared.debt.stages())
                    .map(move |read| (reader.clone(), read.to_owned()))
            })
            .filter(|(reader, read)| {
                !reads
                    .get(reader)
                    .is_some_and(|found| found.contains_key(read))
            })
            .collect()
    }

    /// One refusal per breach of the law, each naming its fix.
    pub fn refusals(&self, name: &str) -> Vec<String> {
        let report = self;
        let mut violations = Vec::new();
        violations.extend(report.forward.iter().map(|edge| {
            let mut refusal = format!(
                "stage order `{name}`: {}:{} {} → {} names `{}`, a later stage; move the type to the stage that produces it, or amend the stage_order row if the order is wrong; never alias it",
                edge.file, edge.line, edge.from, edge.to, edge.path
            );
            if let Some(shared) = edge
                .path
                .rsplit("::")
                .next()
                .filter(|last| report.shared_names.contains(*last) && edge.path.matches("::").count() == 1)
            {
                let _ = write!(
                    refusal,
                    "; `{shared}` is both a module and a lib.rs re-export: if only the module is meant, write `use crate::{shared}::{{self}};`"
                );
            }
            refusal
        }));
        violations.extend(report.through_unplaced.iter().map(|edge| {
            format!(
                "stage order `{name}`: {}:{} {} reaches the later stage {} through unplaced {} ({}:{}); name each unplaced module in its stage_order row, then move the type to its producer",
                edge.reader.file,
                edge.reader.line,
                edge.reader.from,
                edge.onward.to,
                edge.via.join(" → "),
                edge.onward.file,
                edge.onward.line
            )
        }));
        violations.extend(report.unjudged.iter().map(|path| {
            format!(
                "stage order `{name}`: {}:{} `{}` is unjudged ({}); {}",
                path.file,
                path.line,
                path.path,
                path.why,
                path.why.remedy()
            )
        }));
        // A crate with seams reads only the floor whole: the remedy is an
        // input or a debt row, never a whole-stage read.
        let declare = if report.floor.is_empty() {
            "add `{}` to the row's reads with the reason in a comment"
        } else {
            "name `{}` as the row's input (its boundary items) or give it a debt row with its owner"
        };
        violations.extend(report.undeclared_reads().into_iter().map(|edge| {
            format!(
                "stage interface `{name}`: {}:{} {} reads {} (`{}`), which its stage_reads row does not list; read it through the stage's input, or {}",
                edge.file,
                edge.line,
                edge.from,
                edge.to,
                edge.path,
                declare.replace("{}", &edge.to)
            )
        }));
        violations.extend(report.unplaced.iter().map(|module| {
            format!(
                "stage order `{name}`: top-level module `{module}` is not in its stage_order row; add it at its place in the row in .config/architecture.toml, or move its items into the stages that produce them"
            )
        }));
        violations.extend(report.inline_modules.iter().map(|module| {
            format!(
                "stage order `{name}`: lib.rs declares inline module `{module}`, which the census cannot read; move it to its own file and name it in the stage_order row"
            )
        }));
        violations
    }
}

pub(in crate::checks) fn interface_policy_violations(
    orders: &[StageOrder],
    interfaces: &BTreeMap<String, BTreeMap<String, policy::StageReads>>,
    seams: &BTreeMap<String, policy::StageSeams>,
    key: Option<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (crate_name, declared) in seams
        .iter()
        .filter(|(name, _)| key.is_none_or(|key| name.as_str() == key))
    {
        let Some(order) = orders.iter().find(|order| &order.crate_name == crate_name) else {
            violations.push(format!(
                "stage seams `{crate_name}`: stage_seams names a crate with no stage_order row; fix the crate name or add its stage_order row"
            ));
            continue;
        };
        for stage in declared
            .floor
            .iter()
            .filter(|stage| !order.stages.contains(stage))
        {
            violations.push(format!(
                "stage seams `{crate_name}`: floor names `{stage}`, which is not a stage in its stage_order row"
            ));
        }
    }
    for (crate_name, rows) in interfaces
        .iter()
        .filter(|(name, _)| key.is_none_or(|key| name.as_str() == key))
    {
        let Some(order) = orders.iter().find(|order| &order.crate_name == crate_name) else {
            violations.push(format!(
                "stage interface `{crate_name}`: stage_reads names a crate with no stage_order row; fix the crate name or add its stage_order row"
            ));
            continue;
        };
        let floor = seams.get(crate_name).map(|seams| &seams.floor);
        let rank = |stage: &str| order.stages.iter().position(|named| named == stage);
        for (reader, declared) in rows {
            let Some(reader_rank) = rank(reader) else {
                violations.push(format!(
                    "stage interface `{crate_name}`: `{reader}` is not a stage in its stage_order row"
                ));
                continue;
            };
            let listed: Vec<&str> = declared
                .reads
                .iter()
                .chain(&declared.input)
                .map(String::as_str)
                .chain(declared.debt.stages())
                .collect();
            // An input may also owe a debt row (its interior); a stage read
            // whole needs neither.
            let mut once = BTreeSet::new();
            let whole_or_owed = declared
                .reads
                .iter()
                .map(String::as_str)
                .chain(declared.debt.stages());
            for read in whole_or_owed
                .chain(
                    declared
                        .input
                        .iter()
                        .map(String::as_str)
                        .filter(|read| !declared.debt.contains(read)),
                )
                .filter(|read| !once.insert(*read))
            {
                violations.push(format!(
                    "stage interface `{crate_name}`: {reader} lists `{read}` twice among its reads, input and debt; keep one"
                ));
            }
            if let policy::StageDebt::Bare(stages) = &declared.debt
                && !stages.is_empty()
            {
                violations.push(format!(
                    "stage interface `{crate_name}`: {reader} names its debt bare; write each as a row, `{} = {{ items = N, owner = \"<bead>\" }}`, counting the distinct items `atlas seams` lists",
                    stages[0]
                ));
            }
            if let Some(floor) = floor {
                // The floor holds names and laws every stage shares: it
                // reads nothing but the floor.
                if floor.contains(reader) && !listed.is_empty() {
                    violations.push(format!(
                        "stage interface `{crate_name}`: {reader} is a floor stage and lists {}; a floor stage reads only the floor, so move what it needs below it or take {reader} out of the floor",
                        listed.join(", ")
                    ));
                }
                let named: BTreeSet<&str> = listed.iter().copied().collect();
                for read in named
                    .iter()
                    .filter(|read| floor.contains(&(**read).to_owned()))
                {
                    violations.push(format!(
                        "stage interface `{crate_name}`: {reader} lists `{read}`, a floor stage every stage reads whole; drop it from the row"
                    ));
                }
                for read in &declared.reads {
                    violations.push(format!(
                        "stage interface `{crate_name}`: {reader} reads `{read}` whole, but in a crate with item-level seams only the floor is read whole; name `{read}` as an input (its boundary items) or a debt row"
                    ));
                }
            }
            let mut ranked = BTreeSet::new();
            for read in listed.into_iter().filter(|read| ranked.insert(*read)) {
                match rank(read) {
                    None => violations.push(format!(
                        "stage interface `{crate_name}`: {reader} reads `{read}`, which is not a stage in its stage_order row"
                    )),
                    Some(read_rank) if read_rank >= reader_rank => violations.push(format!(
                        "stage interface `{crate_name}`: {reader} reads `{read}`, which is not upstream of it; a stage reads only earlier stages"
                    )),
                    Some(_) => {}
                }
            }
        }
    }
    violations
}
