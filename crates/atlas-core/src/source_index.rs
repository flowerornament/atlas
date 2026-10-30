//! Conservative source names: Cargo roots, explicit imports, and inspected signatures.
use crate::{
    Result,
    architecture::{Architecture, Kind, Module},
    model::SourceGraph,
    workflow::{self, Call, Workflow},
};
use cargo_metadata::{Metadata, TargetKind};
use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
use syn::Type;

pub struct Function {
    pub id: String,
    pub scope: Vec<String>,
    pub flow: Workflow,
}
pub struct Boundary {
    pub module: Module,
    pub path: Vec<String>,
}
pub struct Index {
    pub functions: Vec<Function>,
    pub boundaries: Vec<Boundary>,
    pub roots: Vec<usize>,
    pub inventory: Vec<String>,
    scopes: BTreeMap<String, Vec<Vec<String>>>,
    names: BTreeMap<Vec<String>, Vec<usize>>,
    crates: BTreeSet<String>,
    exports: BTreeMap<Vec<String>, Vec<Vec<String>>>,
}
#[derive(Clone, Debug)]
pub enum Target {
    Function(usize),
    Boundary(usize, Option<String>),
    Summarized(String),
}
fn path(text: &str) -> Option<Vec<String>> {
    let path = syn::parse_str::<syn::Path>(text).ok()?;
    Some(path.segments.iter().map(|s| s.ident.to_string()).collect())
}
fn ty(text: &str) -> Option<Vec<String>> {
    fn inner(ty: &Type) -> Option<Vec<String>> {
        match ty {
            Type::Reference(r) => inner(&r.elem),
            Type::Path(p) if p.qself.is_none() => Some(
                p.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect(),
            ),
            _ => None,
        }
    }
    inner(&syn::parse_str(text).ok()?)
}
impl Index {
    pub fn new(
        root: &Path,
        metadata: &Metadata,
        graph: &SourceGraph,
        architecture: &Architecture,
    ) -> Result<Self> {
        let sources = read_sources(&root.canonicalize()?, graph)?;
        Self::from_sources(&sources, metadata, graph, architecture)
    }
    pub(crate) fn from_sources(
        sources: &Sources,
        metadata: &Metadata,
        graph: &SourceGraph,
        architecture: &Architecture,
    ) -> Result<Self> {
        Self::from_selected_sources(
            sources,
            metadata,
            graph,
            architecture,
            crate::population::TestSelection::Production,
        )
    }
    pub(crate) fn from_selected_sources(
        sources: &Sources,
        metadata: &Metadata,
        graph: &SourceGraph,
        architecture: &Architecture,
        tests: crate::population::TestSelection,
    ) -> Result<Self> {
        let root = metadata.workspace_root.as_std_path();
        let mut functions = Vec::new();
        let mut boundaries = Vec::new();
        let mut roots = Vec::new();
        let mut crates = BTreeSet::new();
        let mut files = BTreeMap::new();
        let mut exports = BTreeMap::new();
        for package in metadata.workspace_packages() {
            let library = package
                .targets
                .iter()
                .find(|t| {
                    t.kind.iter().any(|k| {
                        matches!(
                            k,
                            TargetKind::Lib | TargetKind::RLib | TargetKind::ProcMacro
                        )
                    })
                })
                .map_or_else(
                    || package.name.replace('-', "_"),
                    |t| t.name.replace('-', "_"),
                );
            if !crates.insert(library.clone()) {
                return Err(format!("Cargo library namespace {library} belongs to multiple packages; qualified lookup cannot choose one").into());
            }
            let mut targets = BTreeMap::<String, Vec<(String, bool, String)>>::new();
            for target in &package.targets {
                let path = crate::discovery::target_key(root, target.src_path.as_std_path());
                let binary = target
                    .kind
                    .iter()
                    .any(|kind| matches!(kind, TargetKind::Bin));
                let label = crate::discovery::target_label(target);
                let namespace = if target.kind.iter().any(|kind| {
                    matches!(
                        kind,
                        TargetKind::Lib | TargetKind::RLib | TargetKind::ProcMacro
                    )
                }) {
                    library.clone()
                } else {
                    format!("{library}#{label}")
                };
                targets
                    .entry(path)
                    .or_default()
                    .push((namespace, binary, label));
            }
            for file in graph
                .files
                .iter()
                .filter(|f| f.crate_name == package.name.as_str() && tests.admits_file(f.test_only))
            {
                let mounts: Vec<_> = file
                    .mounts
                    .iter()
                    .filter(|mount| tests.admits_file(mount.test_only))
                    .flat_map(|mount| {
                        targets
                            .get(&mount.root)
                            .into_iter()
                            .flatten()
                            .filter(|(_, _, label)| {
                                mount.target.as_ref().is_none_or(|target| target == label)
                            })
                            .map(|(namespace, binary, _)| {
                                let mut scope = vec![namespace.clone()];
                                scope.extend(mount.modules.clone());
                                (scope, *binary && mount.modules.is_empty())
                            })
                    })
                    .collect();
                files
                    .entry(file.path.clone())
                    .or_insert_with(Vec::new)
                    .extend(mounts);
            }
        }
        for mounts in files.values_mut() {
            mounts.sort();
            mounts.dedup();
        }
        for module in &architecture.modules {
            let root_path = if module.directory {
                format!("{}/mod.rs", module.path)
            } else {
                module.path.clone()
            };
            let mounts = files.get(&root_path).ok_or_else(|| {
                format!(
                    "program boundary {} needs a mounted Rust root at {root_path}",
                    module.name
                )
            })?;
            let [(scope, _)] = mounts.as_slice() else {
                return Err(format!(
                    "program boundary {} has {} Cargo module mounts; cannot choose an owner",
                    module.name,
                    mounts.len()
                )
                .into());
            };
            boundaries.push(Boundary {
                module: module.clone(),
                path: scope.clone(),
            });
        }
        let packages = crates.clone();
        for file in graph
            .physical_files()
            .filter(|f| tests.admits_file(f.test_only))
        {
            let Some(mounts) = files.get(&file.path) else {
                continue;
            };
            let parsed = sources[&file.path].workflow(&file.path, tests)?;
            for (scope, binary) in mounts {
                let namespace = scope[0].split('#').next().unwrap_or(&scope[0]);
                if !packages.contains(namespace) {
                    continue;
                }
                for (name, target) in &parsed.exports {
                    let mut key = scope.clone();
                    key.push(name.clone());
                    let values: Vec<_> = target
                        .iter()
                        .map(|target| normalize(scope, target, &crates))
                        .collect();
                    exports.entry(key).or_insert_with(Vec::new).extend(values);
                }
                for flow in &parsed.functions {
                    let id = format!(
                        "{}::{} [{}]",
                        scope.join("::"),
                        flow.function,
                        flow.site.link()
                    );
                    if *binary && flow.function == "main" {
                        roots.push(functions.len());
                    }
                    let mut function_scope = scope.clone();
                    function_scope.extend(flow.module.clone());
                    functions.push(Function {
                        id,
                        scope: function_scope,
                        flow: flow.clone(),
                    });
                }
            }
        }
        let mut names = BTreeMap::<Vec<String>, Vec<usize>>::new();
        for (i, f) in functions.iter().enumerate() {
            if let Some(p) = path(&f.flow.function) {
                let mut key = f.scope.clone();
                key.extend(p.into_iter().skip(f.flow.module.len()));
                names.entry(key).or_default().push(i);
            }
        }
        let mut index = Self {
            functions,
            boundaries,
            roots,
            inventory: graph.physical_files().map(|f| f.path.clone()).collect(),
            scopes: files
                .into_iter()
                .map(|(path, mounts)| (path, mounts.into_iter().map(|(scope, _)| scope).collect()))
                .collect(),
            names,
            crates,
            exports,
        };
        // Resolve an impl owner imported from another physical file only once.
        for (i, f) in index.functions.iter().enumerate() {
            if let Some(owner) = f.flow.owner.as_deref().and_then(ty)
                && let Some(mut key) = index.canonical(i, &owner).and_then(|p| index.exported(&p))
            {
                key.push(
                    f.flow
                        .function
                        .rsplit("::")
                        .next()
                        .ok_or("method name missing")?
                        .into(),
                );
                if let Some(p) = path(&f.flow.function) {
                    let mut original = f.scope.clone();
                    original.extend(p.into_iter().skip(f.flow.module.len()));
                    if let Some(values) = index.names.get_mut(&original) {
                        values.retain(|v| *v != i);
                    }
                }
                index.names.entry(key).or_default().push(i);
            }
        }
        Ok(index)
    }
    pub(crate) fn scopes(&self, file: &str, module: &[String]) -> Vec<Vec<String>> {
        self.scopes
            .get(file)
            .into_iter()
            .flatten()
            .map(|scope| scope.iter().chain(module).cloned().collect())
            .collect()
    }
    pub(crate) fn in_scope(
        &self,
        scope: &[String],
        imports: &BTreeMap<String, Vec<Vec<String>>>,
        p: &[String],
    ) -> Option<Vec<String>> {
        let mut p = p.to_vec();
        if let Some(candidates) = p.first().and_then(|name| imports.get(name)) {
            let [import] = candidates.as_slice() else {
                return None;
            };
            p = import.iter().chain(&p[1..]).cloned().collect();
        }
        self.exported(&normalize(scope, &p, &self.crates))
    }
    pub(crate) fn canonical(&self, function: usize, p: &[String]) -> Option<Vec<String>> {
        let f = &self.functions[function];
        let mut p = p.to_vec();
        if p.first()
            .is_some_and(|name| f.flow.type_parameters.contains(name))
        {
            return None;
        }
        if p.first().is_some_and(|name| name == "Self") {
            let mut owner = path(f.flow.owner.as_deref()?)?;
            owner.extend_from_slice(&p[1..]);
            p = owner;
        }
        self.in_scope(&f.scope, &f.flow.imports, &p)
    }

    pub(crate) fn exported(&self, path: &[String]) -> Option<Vec<String>> {
        let mut path = path.to_vec();
        let mut seen = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        while seen.insert(path.clone()) {
            let found = (1..=path.len()).rev().find_map(|len| {
                self.exports
                    .get(&path[..len])
                    .map(|target| (len, target.clone()))
            });
            let Some((len, candidates)) = found else {
                return Some(path);
            };
            if !aliases.insert(path[..len].to_vec()) {
                return None;
            }
            let [target] = candidates.as_slice() else {
                return None;
            };
            let mut target = target.clone();
            target.extend_from_slice(&path[len..]);
            path = target;
        }
        None
    }
    fn classify(&self, p: &[String]) -> Target {
        let Some(resolved) = self.exported(p) else {
            return Target::Summarized("ambiguous or cyclic public reexport".into());
        };
        let p = resolved.as_slice();
        for (i, b) in self.boundaries.iter().enumerate() {
            if p.starts_with(&b.path) {
                let mut entry = b.path.clone();
                entry.push(b.module.entry.clone());
                let Some(entry) = self.exported(&entry) else {
                    return Target::Summarized("ambiguous stage entry reexport".into());
                };
                if matches!(b.module.kind, Kind::Instrument | Kind::Edge)
                    || (if self.names.contains_key(&entry) {
                        p == entry
                    } else {
                        p.len() == entry.len() + 1 && p.starts_with(&entry)
                    })
                {
                    let member = if matches!(b.module.kind, Kind::Instrument | Kind::Edge) {
                        Some(p[b.path.len()..].join("::"))
                    } else if p.len() > entry.len() {
                        Some(p[entry.len()..].join("::"))
                    } else {
                        None
                    };
                    return Target::Boundary(i, member);
                }
                return Target::Summarized(format!(
                    "{} support API, outside its declared entry",
                    b.module.name
                ));
            }
        }
        match self.names.get(p).map(Vec::as_slice) {
            Some([i]) => Target::Function(*i),
            Some(candidates) => Target::Summarized(format!(
                "ambiguous static call: {} candidates",
                candidates.len()
            )),
            None => Target::Summarized(format!("unresolved or external path {}", p.join("::"))),
        }
    }
    pub fn resolve(&self, function: usize, call: &Call) -> Target {
        let f = &self.functions[function];
        if let Some(receiver) = &call.receiver {
            if receiver == "self" {
                return self
                    .canonical(function, &["Self".into(), call.spelling.clone()])
                    .map_or_else(
                        || Target::Summarized("self owner not established".into()),
                        |p| self.classify(&p),
                    );
            }
            let bindings: Vec<_> = f
                .flow
                .bindings
                .iter()
                .filter(|b| {
                    &b.name == receiver
                        && (b.site.end_line, b.site.end_column)
                            <= (call.site.line, call.site.column)
                })
                .collect();
            if bindings.len() != 1 {
                return Target::Summarized(format!(
                    "receiver {receiver}: no unique explicit source type"
                ));
            }
            let binding = bindings[0];
            let owner = if let Some(t) = binding.ty.as_deref().and_then(ty) {
                self.canonical(function, &t)
            } else if let Some(init) = binding.initializer.as_deref().and_then(path) {
                self.canonical(function, &init)
                    .and_then(|p| self.exported(&p))
                    .and_then(|p| {
                        self.names.get(&p).filter(|v| v.len() == 1).and_then(|v| {
                            let target = &self.functions[v[0]];
                            match target.flow.output.as_str() {
                                "Self" => Some(p[..p.len() - 1].to_vec()),
                                output => ty(output).and_then(|t| self.canonical(v[0], &t)),
                            }
                        })
                    })
            } else {
                None
            };
            return owner.map_or_else(
                || {
                    Target::Summarized(format!(
                        "receiver {receiver}: initializer result not established"
                    ))
                },
                |mut p| {
                    p.push(call.spelling.clone());
                    self.classify(&p)
                },
            );
        }
        let Some(p) = path(&call.spelling) else {
            return Target::Summarized("indirect or unsupported call syntax".into());
        };
        if p.len() == 1
            && f.flow.bindings.iter().any(|b| {
                b.name == p[0]
                    && (b.site.end_line, b.site.end_column) <= (call.site.line, call.site.column)
            })
        {
            return Target::Summarized(format!("value call {}", p[0]));
        }
        if p.len() == 1 && f.flow.imports.contains_key(&p[0]) {
            let local = normalize(&f.scope, &p, &self.crates);
            if self.names.contains_key(&local) {
                return Target::Summarized(
                    "local value and import share a name; namespace unresolved".into(),
                );
            }
        }
        self.canonical(function, &p).map_or_else(
            || Target::Summarized("ambiguous source import".into()),
            |p| self.classify(&p),
        )
    }
    pub fn select(&self, path: &str, function: &str) -> Result<usize> {
        let found: Vec<_> = self
            .functions
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.flow.site.path == path
                    && (f.flow.function == function
                        || (!function.contains("::")
                            && f.flow.function.rsplit("::").next() == Some(function)))
            })
            .map(|(i, _)| i)
            .collect();
        match found.as_slice() {
            [i] => Ok(*i),
            _ => Err(format!(
                "program entry {path}::{function}: expected one function, found {}",
                found.len()
            )
            .into()),
        }
    }
}

fn normalize(scope: &[String], path: &[String], crates: &BTreeSet<String>) -> Vec<String> {
    let mut path = path.to_vec();
    if path.first().is_some_and(|name| crates.contains(name)) {
        return path;
    }
    let mut scope = scope.to_vec();
    if path.first().is_some_and(|name| name == "crate") {
        scope.truncate(1);
        path.remove(0);
    } else if path.first().is_some_and(|name| name == "self") {
        path.remove(0);
    } else {
        while path.first().is_some_and(|name| name == "super") {
            scope.pop();
            path.remove(0);
        }
    }
    scope.extend(path);
    scope
}

/// One parsed physical source; context-independent workflow facts are shared across indexes.
pub(crate) struct ParsedSource {
    source: Arc<crate::source::Source>,
    ast: OnceCell<std::result::Result<syn::File, String>>,
    workflow: OnceCell<workflow::ParsedFile>,
}
impl ParsedSource {
    pub(crate) fn text(&self) -> &str {
        self.source.text()
    }
    pub(crate) fn source(&self) -> &crate::source::Source {
        &self.source
    }
    pub(crate) fn ast(&self, path: &str) -> Result<&syn::File> {
        self.ast
            .get_or_init(|| {
                syn::parse_file(self.source.text())
                    .map_err(|error| format!("parse source {path}: {error}"))
            })
            .as_ref()
            .map_err(|error| error.clone().into())
    }
    pub(crate) fn clear_workflow(&mut self) {
        self.workflow.take();
    }
    fn workflow(
        &self,
        path: &str,
        tests: crate::population::TestSelection,
    ) -> Result<&workflow::ParsedFile> {
        let ast = self.ast(path)?;
        Ok(self
            .workflow
            .get_or_init(|| workflow::read_ast(&self.source, path, ast, tests)))
    }
}
pub(crate) type Sources = BTreeMap<String, ParsedSource>;
/// Retain each physical source once; selected ASTs are cached on first use.
pub(crate) fn parse_sources(
    sources: impl IntoIterator<Item = Result<(String, Arc<crate::source::Source>)>>,
) -> Result<Sources> {
    sources
        .into_iter()
        .map(|input| {
            let (path, source) = input?;
            Ok((
                path,
                ParsedSource {
                    source,
                    ast: OnceCell::new(),
                    workflow: OnceCell::new(),
                },
            ))
        })
        .collect()
}
pub(crate) fn read_sources(root: &Path, graph: &SourceGraph) -> Result<Sources> {
    parse_sources(graph.physical_files().filter(|f| !f.test_only).map(|f| {
        let source = std::fs::read_to_string(root.join(&f.path))?;
        if crate::discovery::fingerprint(source.as_bytes()) != f.content_fingerprint {
            return Err(format!(
                "source changed after discovery: {}; rescan before extracting",
                f.path
            )
            .into());
        }
        Ok((
            f.path.clone(),
            Arc::new(crate::source::Source::of(Path::new(&f.path), source)),
        ))
    }))
}
