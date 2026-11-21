//! Each reachable coordinator is inspected once; recursion stays a graph cycle.
use super::{
    Connection, Hop, Observation, Program,
    index::{Index, Target},
};
use crate::{
    Result,
    architecture::{self, Architecture, Kind},
    diagram::{Node, Surface},
    workflow::{Site, Step},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn build(
    index: &Index,
    architecture: &Architecture,
    entry: Option<(&str, &str)>,
) -> Result<Program> {
    let roots = if let Some((path, function)) = entry {
        vec![index.select(path, function)?]
    } else {
        index.roots().to_vec()
    };
    if roots.is_empty() {
        return Err(
            "no Cargo binary entry point in the profile's packages; select --path and --function"
                .into(),
        );
    }
    let anchors: Vec<_> = architecture
        .workflows
        .iter()
        .map(|a| {
            index
                .select(&a.path, &a.function)
                .map(|i| index.functions()[i].id.clone())
        })
        .collect::<Result<_>>()?;
    let mut trace = Trace {
        pending: roots.clone(),
        ..Trace::default()
    };
    let mut seen = BTreeSet::new();
    while let Some(i) = trace.pending.pop() {
        if !seen.insert(i) {
            continue;
        }
        trace.inspect(index, i, roots.contains(&i))?;
    }
    if entry.is_none() {
        add_unsurfaced_stages(index, &mut trace.nodes);
    }
    let Trace {
        nodes,
        connections,
        observations,
        files,
        ..
    } = trace;
    let roots = roots
        .iter()
        .map(|&i| index.functions()[i].id.clone())
        .collect();
    let connections = connections
        .into_iter()
        .map(|((from, to), chains)| Connection { from, to, chains })
        .collect();
    Ok(Program {
        schema: 1,
        roots,
        anchors,
        connections,
        observations,
        represented_files: files.iter().cloned().collect(),
        inventory: index.inventory().to_vec(),
        outside_view_files: index
            .inventory()
            .iter()
            .filter(|f| !files.contains(*f))
            .cloned()
            .collect(),
        nodes: nodes.into_values().collect(),
    })
}

/// What the walk from the roots has observed so far, and the coordinators it
/// has still to inspect.
#[derive(Default)]
struct Trace {
    pending: Vec<usize>,
    nodes: BTreeMap<String, Node>,
    connections: BTreeMap<(String, String), Vec<Vec<Hop>>>,
    observations: Vec<Observation>,
    files: BTreeSet<String>,
}

impl Trace {
    /// Record coordinator `i` and each of its calls: a workspace function is
    /// queued for inspection, a declared boundary becomes its surface node,
    /// and a summarized or unknown resolution stays an observation.
    fn inspect(&mut self, index: &Index, i: usize, root: bool) -> Result<()> {
        let f = &index.functions()[i];
        self.files.insert(f.flow.site.path.clone());
        self.nodes.insert(
            f.id.clone(),
            Node {
                id: f.id.clone(),
                label: f.flow.function.clone(),
                group: if root {
                    "Entry".into()
                } else {
                    "Coordinators".into()
                },
                source_sites: vec![f.flow.site.link()],
                context: false,
                internal_relationships: vec![],
                surface: None,
            },
        );
        opaque(&f.id, &f.flow.steps, &mut self.observations);
        for (ordinal, call) in f.flow.calls.iter().enumerate() {
            let target = match index.resolve(i, call) {
                Target::Function(target) => {
                    self.pending.push(target);
                    index.functions()[target].id.clone()
                }
                Target::Boundary(boundary, member) => {
                    self.boundary(index, boundary, member.as_deref(), &call.site)
                }
                Target::InvalidFunction(position) => {
                    return Err(format!("invalid indexed function position {position}").into());
                }
                Target::Summarized(reason) => {
                    self.observations.push(Observation {
                        function: f.id.clone(),
                        call: Some(call.clone()),
                        reason: reason.message,
                    });
                    continue;
                }
                // A resolution this view does not know stays unresolved.
                other => {
                    self.observations.push(Observation {
                        function: f.id.clone(),
                        call: Some(call.clone()),
                        reason: format!("unrecognized call resolution: {other:?}"),
                    });
                    continue;
                }
            };
            self.connections
                .entry((f.id.clone(), target))
                .or_default()
                .push(vec![Hop {
                    function: f.id.clone(),
                    ordinal: ordinal + 1,
                    call: call.clone(),
                    context: context(&f.flow.steps, &call.site),
                }]);
        }
        Ok(())
    }

    /// The surface node for a call into a declared boundary (or one named
    /// member of it), citing the call site; the boundary's files become
    /// represented. Returns the node's id.
    fn boundary(
        &mut self,
        index: &Index,
        boundary: usize,
        member: Option<&str>,
        site: &Site,
    ) -> String {
        let b = &index.boundaries()[boundary];
        let module = &b.module;
        let id = member.map_or_else(
            || module.name.clone(),
            |member| format!("{}::{member}", module.name),
        );
        let label = member.map_or_else(
            || module.name.clone(),
            |member| format!("{}::{member}", module.name),
        );
        let node = self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            label,
            group: if b.path.len() > 2 {
                b.path[1].clone()
            } else {
                module.label().into()
            },
            source_sites: vec![],
            context: false,
            internal_relationships: vec![],
            surface: Some(Surface {
                entry: module.entry.clone(),
                types: module.types.clone(),
            }),
        });
        let site = site.link();
        if !node.source_sites.contains(&site) {
            node.source_sites.push(site);
        }
        for file in index.inventory() {
            if architecture::contains(&module.path, file) {
                self.files.insert(file.clone());
            }
        }
        id
    }
}

/// A declared stage that no call reached is still shown, as context.
fn add_unsurfaced_stages(index: &Index, nodes: &mut BTreeMap<String, Node>) {
    for b in index.boundaries() {
        let module = &b.module;
        if module.kind == Kind::Stage
            && !nodes.values().any(|n| {
                n.surface.as_ref().is_some_and(|s| s.entry == module.entry)
                    && n.id.starts_with(&module.name)
            })
        {
            nodes.insert(
                module.name.clone(),
                Node {
                    id: module.name.clone(),
                    label: module.name.clone(),
                    group: "Not surfaced".into(),
                    source_sites: vec![module.path.clone()],
                    context: true,
                    internal_relationships: vec![],
                    surface: Some(Surface {
                        entry: module.entry.clone(),
                        types: module.types.clone(),
                    }),
                },
            );
        }
    }
}
fn opaque(function: &str, steps: &[Step], out: &mut Vec<Observation>) {
    for step in steps {
        match step {
            Step::Opaque { label, site, .. } => out.push(Observation {
                function: function.into(),
                call: None,
                reason: format!("opaque source expression at {}: {label}", site.link()),
            }),
            Step::Branch { arms, .. } => {
                for arm in arms {
                    opaque(function, &arm.steps, out);
                }
            }
            Step::Match { arms, .. } => {
                for arm in arms {
                    opaque(function, &arm.guard, out);
                    opaque(function, &arm.body, out);
                }
            }
            Step::Loop { head, body, .. } => {
                opaque(function, head, out);
                opaque(function, body, out);
            }
            Step::Block { body, .. } => opaque(function, body, out),
            _ => {}
        }
    }
}
fn context(steps: &[Step], site: &Site) -> Vec<String> {
    for step in steps.iter().filter(|s| s.site().contains(site)) {
        match step {
            Step::Branch { label, arms, .. } => {
                for arm in arms {
                    if arm.steps.iter().any(|s| s.site().contains(site)) {
                        let mut result = vec![format!("{label}: {}", arm.label)];
                        result.extend(context(&arm.steps, site));
                        return result;
                    }
                }
            }
            Step::Match { label, arms, .. } => {
                for arm in arms {
                    for (part, steps) in [("guard", &arm.guard), ("body", &arm.body)] {
                        if steps.iter().any(|s| s.site().contains(site)) {
                            let mut result = vec![format!("{label}: {} {part}", arm.pattern)];
                            result.extend(context(steps, site));
                            return result;
                        }
                    }
                }
            }
            Step::Loop {
                label, body, head, ..
            } => {
                if head.iter().any(|s| s.site().contains(site)) {
                    let mut result = vec![format!("{label}: header")];
                    result.extend(context(head, site));
                    return result;
                }
                let mut result = vec![label.clone()];
                result.extend(context(body, site));
                return result;
            }
            Step::Block { label, body, .. } => {
                let mut result = vec![label.clone()];
                result.extend(context(body, site));
                return result;
            }
            Step::Opaque { .. } => {
                return vec![
                    "opaque surrounding expression; only explicit immediate calls inspected".into(),
                ];
            }
            _ => {}
        }
    }
    vec![]
}
