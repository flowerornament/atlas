//! One projection for each level; type edges are a quotient of the same source graph.
use super::{Coverage, Diagram, Edge, EdgeEvidence, Level, Node, Query, View};
use crate::{
    Result,
    architecture::{Architecture, Kind, Module, contains},
    model::{Declaration, Resolution, SourceFile, SourceGraph},
};
use std::collections::{BTreeMap, BTreeSet};

/// Select an explicit boundary or physical path, then derive its requested view.
pub fn project(
    graph: &SourceGraph,
    architecture: Option<&Architecture>,
    query: &Query,
) -> Result<Diagram> {
    if matches!(query.view, View::Workflow | View::Program) {
        return Err("use atlas flow for source workflow analysis".into());
    }
    if architecture.is_some_and(|a| a.modules.is_empty()) {
        return Err(
            "map requires declared modules; this profile contains only workflow anchors".into(),
        );
    }
    let focus = resolve_focus(graph, architecture, query.focus.as_deref())?;
    if query.level == Level::Stages && architecture.is_none() {
        return Err(
            "stages requires --architecture; use modules for physical directory groups".into(),
        );
    }
    if query.view == View::Dependencies {
        let profile = architecture
            .ok_or("dependencies requires --architecture; types is the syntactic source view")?;
        if matches!(query.level, Level::Files | Level::Declarations) {
            return Err("dependencies is declared only at stages/modules level; use --view types to drill into source".into());
        }
        if query.level == Level::Modules && !focus.is_empty() {
            return Err("focused child-module allowed dependencies are not declared; use --view types, or --level stages --view dependencies".into());
        }
        return dependencies(graph, profile, query, &focus);
    }
    types(graph, architecture, query, &focus)
}

fn resolve_focus(
    graph: &SourceGraph,
    architecture: Option<&Architecture>,
    focus: Option<&str>,
) -> Result<String> {
    let Some(focus) = focus.filter(|f| !f.is_empty() && *f != ".") else {
        return Ok(String::new());
    };
    if let Some(module) = architecture.and_then(|a| a.modules.iter().find(|m| m.name == focus)) {
        return Ok(module.path.clone());
    }
    crate::discovery::resolve_scope(graph, Some(focus))
}

fn inventory(graph: &SourceGraph, architecture: Option<&Architecture>, focus: &str) -> Vec<String> {
    let prefix = architecture.map_or(String::new(), Architecture::common_directory);
    graph
        .physical_files()
        .filter(|f| contains(&prefix, &f.path) && contains(focus, &f.path))
        .map(|f| f.path.clone())
        .collect()
}
fn profile_node(module: &Module, context: bool, level: Level) -> Node {
    Node {
        id: format!("module:{}", module.name),
        label: module.name.clone(),
        group: if level == Level::Stages {
            std::path::Path::new(&module.path)
                .parent()
                .map_or(".", |p| p.to_str().unwrap_or("."))
                .into()
        } else {
            module.label().into()
        },
        source_sites: vec![module.path.clone()],
        context,
        internal_relationships: Vec::new(),
        surface: (!module.entry.is_empty() || !module.types.is_empty()).then(|| super::Surface {
            entry: module.entry.clone(),
            types: module.types.clone(),
        }),
    }
}
fn notes(view: View) -> Vec<String> {
    match view {
        View::Workflow | View::Program => vec![
            "Source call/control order, not a runtime trace.".into(),
        ],
        View::Dependencies => vec![
            "reads = declared allowed dependency from the architecture profile; not execution order or observed use.".into(),
            "Shared vocabulary/laws are permitted by the project's layer rule; absent declared edges do not mean no access.".into(),
        ],
        View::Types => vec![
            "consumes/produces/composes are approximate declaration mentions, not runtime data flow or ownership.".into(),
            "Internal relationships are collapsed, with exact IDs retained per node. Coarse ambiguous names are summarized, never resolved by layout.".into(),
        ],
    }
}

fn diagram(
    graph: &SourceGraph,
    architecture: Option<&Architecture>,
    query: &Query,
    focus: &str,
) -> Diagram {
    let mut n = notes(query.view);
    if let Some(profile) = architecture {
        n.extend(
            profile
                .provenance
                .iter()
                .map(|s| format!("Architecture source: {s}")),
        );
    }
    Diagram {
        title: format!(
            "{} / {}{}",
            query.level.label(),
            query.view.label(),
            if focus.is_empty() {
                String::new()
            } else {
                format!(" / {focus}")
            }
        ),
        level: query.level,
        view: query.view,
        nodes: Vec::new(),
        edges: Vec::new(),
        inventory: inventory(graph, architecture, focus),
        coverage: Coverage {
            source_relationships: graph.relationships.len(),
            ..Coverage::default()
        },
        notes: n,
        evidence_links: Vec::new(),
    }
}
fn dependencies(
    graph: &SourceGraph,
    profile: &Architecture,
    query: &Query,
    focus: &str,
) -> Result<Diagram> {
    let mut out = diagram(graph, Some(profile), query, focus);
    let mut nodes = BTreeMap::new();
    for module in &profile.modules {
        if (query.level != Level::Stages || module.kind == Kind::Stage)
            && (focus.is_empty() || contains(focus, &module.path) || contains(&module.path, focus))
        {
            nodes.insert(
                module.name.clone(),
                profile_node(module, false, query.level),
            );
        }
    }
    let selected: BTreeSet<_> = nodes.keys().cloned().collect();
    for module in &profile.modules {
        for dependency in &module.dependencies {
            let id = format!("reads:{}:{dependency}", module.name);
            let target = profile
                .modules
                .iter()
                .find(|m| m.name == *dependency)
                .ok_or("profile dependency missing")?;
            if !selected.contains(&module.name)
                || (query.level == Level::Stages && target.kind != Kind::Stage)
            {
                out.coverage.hidden_dependencies.push(id);
                continue;
            }
            nodes
                .entry(target.name.clone())
                .or_insert_with(|| profile_node(target, true, query.level));
            out.edges.push(Edge {
                label: None,
                from: format!("module:{}", module.name),
                to: format!("module:{dependency}"),
                evidence: EdgeEvidence::DeclaredRead,
                relationships: Vec::new(),
                declarations: vec![id],
            });
        }
    }
    out.coverage.hidden_relationships.insert(
        "source mentions not used by dependency view".into(),
        graph.relationships.iter().map(|r| r.id.clone()).collect(),
    );
    out.nodes = profile
        .modules
        .iter()
        .filter_map(|module| nodes.remove(&module.name))
        .collect();
    let represented: BTreeSet<_> = profile
        .modules
        .iter()
        .filter(|m| {
            out.nodes
                .iter()
                .any(|n| n.id == format!("module:{}", m.name))
        })
        .map(|m| m.name.as_str())
        .collect();
    for file in graph.physical_files() {
        let shown = profile
            .owner(&file.path)
            .is_some_and(|m| represented.contains(m.name.as_str()));
        account_file(&mut out.coverage, profile, file, shown);
    }
    out.validate()?;
    Ok(out)
}
fn account_file(coverage: &mut Coverage, profile: &Architecture, file: &SourceFile, shown: bool) {
    if shown {
        coverage.represented_files.push(file.path.clone());
    } else if !contains(&profile.common_directory(), &file.path) {
        coverage.outside_profile_files.push(file.path.clone());
    } else if let Some(owner) = profile.owner(&file.path) {
        coverage
            .collapsed_files
            .entry(owner.label().into())
            .or_default()
            .push(file.path.clone());
    } else {
        coverage.unassigned_files.push(file.path.clone());
    }
}

struct Groups {
    nodes: BTreeMap<String, Node>,
    source_nodes: BTreeMap<String, String>,
    file_nodes: BTreeMap<String, Vec<String>>,
}
impl Groups {
    fn new(
        graph: &SourceGraph,
        profile: Option<&Architecture>,
        query: &Query,
        focus: &str,
    ) -> Self {
        let mut result = Self {
            nodes: BTreeMap::new(),
            source_nodes: BTreeMap::new(),
            file_nodes: BTreeMap::new(),
        };
        let mut path_counts = BTreeMap::<&str, usize>::new();
        for file in &graph.files {
            *path_counts.entry(&file.path).or_default() += 1;
        }
        let focus_module = profile.and_then(|p| p.modules.iter().find(|m| m.path == focus));
        for file in &graph.files {
            if query.level == Level::Declarations {
                let id = format!("file:{}", file.path);
                result.add(
                    file,
                    None,
                    Node {
                        id,
                        label: format!("{} (file)", leaf(&file.path)),
                        group: folder(&file.path),
                        source_sites: vec![file.path.clone()],
                        context: !contains(focus, &file.path),
                        internal_relationships: Vec::new(),
                        surface: None,
                    },
                );
                for d in file.declarations.iter().filter(|d| !d.test_only) {
                    result.add(
                        file,
                        Some(d),
                        Node {
                            id: d.id.clone(),
                            label: format!(
                                "{} [{}:{}]{}",
                                d.name,
                                leaf(&file.path),
                                d.line,
                                if path_counts[file.path.as_str()] > 1 {
                                    format!(" @{}", file.crate_name)
                                } else {
                                    String::new()
                                }
                            ),
                            group: folder(&file.path),
                            source_sites: vec![d.id.clone()],
                            context: !contains(focus, &file.path),
                            internal_relationships: Vec::new(),
                            surface: None,
                        },
                    );
                }
                continue;
            }
            let node = match query.level {
                Level::Stages => profile
                    .and_then(|p| p.owner(&file.path))
                    .filter(|m| m.kind == Kind::Stage)
                    .map(|m| profile_node(m, !contains(focus, &file.path), query.level)),
                Level::Modules => {
                    if let Some(module) =
                        focus_module.filter(|m| m.directory && contains(&m.path, &file.path))
                    {
                        let suffix = file
                            .path
                            .strip_prefix(&format!("{}/", module.path))
                            .unwrap_or(&file.path)
                            .trim_end_matches(".rs");
                        let suffix = if suffix == "mod" {
                            ""
                        } else {
                            suffix.trim_end_matches("/mod")
                        };
                        let name = if suffix.is_empty() {
                            module.name.clone()
                        } else {
                            format!("{}::{}", module.name, suffix.replace('/', "::"))
                        };
                        Some(Node {
                            id: format!("child:{name}"),
                            label: name,
                            group: module.name.clone(),
                            source_sites: vec![file.path.clone()],
                            context: false,
                            internal_relationships: Vec::new(),
                            surface: if suffix.is_empty() {
                                profile_node(module, false, query.level).surface
                            } else {
                                None
                            },
                        })
                    } else if let Some(profile) = profile {
                        profile.owner(&file.path).map(|m| {
                            if !focus.is_empty()
                                && !contains(focus, &file.path)
                                && matches!(m.kind, Kind::Vocabulary | Kind::SharedLaw)
                            {
                                Node {
                                    id: "context:shared-layer".into(),
                                    label: "shared layer".into(),
                                    group: "context".into(),
                                    source_sites: vec![file.path.clone()],
                                    context: true,
                                    internal_relationships: Vec::new(),
                                    surface: None,
                                }
                            } else {
                                let mut node =
                                    profile_node(m, !contains(focus, &file.path), query.level);
                                node.source_sites = vec![file.path.clone()];
                                if node.context {
                                    node.group = "context".into();
                                }
                                node
                            }
                        })
                    } else {
                        let path = folder(&file.path);
                        Some(Node {
                            id: format!("directory:{path}"),
                            label: leaf(&path).into(),
                            group: folder(&path),
                            source_sites: vec![file.path.clone()],
                            context: !contains(focus, &file.path),
                            internal_relationships: Vec::new(),
                            surface: None,
                        })
                    }
                }
                Level::Files => Some(Node {
                    id: file.path.clone(),
                    label: leaf(&file.path).into(),
                    group: folder(&file.path),
                    source_sites: vec![file.path.clone()],
                    context: !contains(focus, &file.path),
                    internal_relationships: Vec::new(),
                    surface: None,
                }),
                Level::Declarations => None,
            };
            if let Some(mut node) = node {
                node.source_sites = vec![file.path.clone()];
                let id = node.id.clone();
                result.add(file, None, node);
                for d in file.declarations.iter().filter(|d| !d.test_only) {
                    result.source_nodes.insert(d.id.clone(), id.clone());
                }
            }
        }
        result
    }
    fn add(&mut self, file: &SourceFile, declaration: Option<&Declaration>, node: Node) {
        if let Some(d) = declaration {
            self.source_nodes.insert(d.id.clone(), node.id.clone());
        }
        self.file_nodes
            .entry(file.path.clone())
            .or_default()
            .push(node.id.clone());
        self.nodes
            .entry(node.id.clone())
            .and_modify(|n| {
                for site in &node.source_sites {
                    if !n.source_sites.contains(site) {
                        n.source_sites.push(site.clone());
                    }
                }
                n.context &= node.context;
            })
            .or_insert(node);
    }
}
fn leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
fn folder(path: &str) -> String {
    path.rsplit_once('/').map_or(".", |(p, _)| p).into()
}
fn hidden(out: &mut Diagram, reason: &str, id: &str) {
    out.coverage
        .hidden_relationships
        .entry(reason.into())
        .or_default()
        .push(id.into());
}

fn types(
    graph: &SourceGraph,
    profile: Option<&Architecture>,
    query: &Query,
    focus: &str,
) -> Result<Diagram> {
    let mut out = diagram(graph, profile, query, focus);
    if out.inventory.is_empty() {
        return Err("focus contains no architecture/source files".into());
    }
    let selected: BTreeSet<_> = out.inventory.iter().cloned().collect();
    let sites: BTreeMap<_, _> = graph
        .files
        .iter()
        .flat_map(|f| f.declarations.iter().map(move |d| (&d.id, &f.path)))
        .collect();
    let mut groups = Groups::new(graph, profile, query, focus);
    let mut visible = BTreeSet::new();
    for file in &out.inventory {
        if let Some(ids) = groups.file_nodes.get(file) {
            visible.extend(ids.iter().cloned());
        }
    }
    // Declared modules/stages without declarations still remain visible.
    if focus.is_empty()
        && matches!(query.level, Level::Stages | Level::Modules)
        && let Some(p) = profile
    {
        for module in &p.modules {
            if query.level != Level::Stages || module.kind == Kind::Stage {
                let node = profile_node(module, false, query.level);
                visible.insert(node.id.clone());
                groups.nodes.entry(node.id.clone()).or_insert(node);
            }
        }
    }
    let coarse = matches!(query.level, Level::Stages | Level::Modules);
    let mut edges: BTreeMap<(String, String, String), Edge> = BTreeMap::new();
    for relation in &graph.relationships {
        let Some(file) = sites.get(&relation.source) else {
            return Err("source relationship refers to absent declaration".into());
        };
        if !selected.contains(*file) {
            hidden(&mut out, "outside focus", &relation.id);
            continue;
        }
        let Some(from) = groups.source_nodes.get(&relation.source).cloned() else {
            hidden(&mut out, "source collapsed at level", &relation.id);
            continue;
        };
        let to = match &relation.target {
            Resolution::Unique(target) => {
                let Some(id) = groups.source_nodes.get(target) else {
                    hidden(&mut out, "target collapsed at level", &relation.id);
                    continue;
                };
                id.clone()
            }
            Resolution::Ambiguous(candidates) => {
                if coarse {
                    hidden(&mut out, "ambiguous target summarized", &relation.id);
                    continue;
                }
                let id = format!("ambiguous:{}:{}", relation.spelling, candidates.join("|"));
                groups.nodes.entry(id.clone()).or_insert_with(|| Node {
                    id: id.clone(),
                    label: format!("{} ? ({} candidates)", relation.spelling, candidates.len()),
                    group: "unresolved names".into(),
                    source_sites: candidates.clone(),
                    context: true,
                    internal_relationships: Vec::new(),
                    surface: None,
                });
                id
            }
            Resolution::Unresolved(reason) => {
                if coarse {
                    hidden(&mut out, "unresolved target summarized", &relation.id);
                    continue;
                }
                let id = format!("unresolved:{}:{reason}", relation.spelling);
                groups.nodes.entry(id.clone()).or_insert_with(|| Node {
                    id: id.clone(),
                    label: format!("{} ? ({reason})", relation.spelling),
                    group: "unresolved names".into(),
                    source_sites: Vec::new(),
                    context: true,
                    internal_relationships: Vec::new(),
                    surface: None,
                });
                id
            }
        };
        visible.insert(from.clone());
        visible.insert(to.clone());
        if from == to && query.level != Level::Declarations {
            groups
                .nodes
                .get_mut(&from)
                .ok_or("internal source node missing")?
                .internal_relationships
                .push(relation.id.clone());
            continue;
        }
        let key = (from.clone(), to.clone(), relation.evidence.label().into());
        edges
            .entry(key)
            .or_insert_with(|| Edge {
                label: None,
                from,
                to,
                evidence: relation.evidence.into(),
                relationships: Vec::new(),
                declarations: Vec::new(),
            })
            .relationships
            .push(relation.id.clone());
    }
    for file in graph.physical_files() {
        let shown = groups
            .file_nodes
            .get(&file.path)
            .is_some_and(|ids| ids.iter().any(|id| visible.contains(id)));
        if let Some(p) = profile {
            account_file(&mut out.coverage, p, file, shown);
        } else if shown {
            out.coverage.represented_files.push(file.path.clone());
        } else {
            out.coverage
                .collapsed_files
                .entry("outside focus".into())
                .or_default()
                .push(file.path.clone());
        }
    }
    out.nodes = groups
        .nodes
        .into_iter()
        .filter(|(id, _)| visible.contains(id))
        .map(|(_, node)| node)
        .collect();
    out.edges = edges.into_values().collect();
    if query.level == Level::Stages
        && let Some(profile) = profile
    {
        out.nodes.sort_by_key(|n| {
            profile
                .modules
                .iter()
                .position(|m| n.id == format!("module:{}", m.name))
        });
    }
    out.notes.push(format!(
        "{} source diagnostics retained in source evidence; no runtime relationships are inferred.",
        graph.diagnostics.len()
    ));
    out.validate()?;
    Ok(out)
}
