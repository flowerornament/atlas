//! Conservative call lookup through explicit imports, globs of workspace
//! modules, reexports and signature types. Ambiguous, indirect and
//! unsupported calls stay summarized; no Rust type inference.
use super::{Function, Index, Summary, SummaryKind, Target};
use crate::{
    analysis::workflow::{Binding, Call, facts},
    architecture::Kind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

pub(super) fn path(text: &str) -> Option<Vec<String>> {
    let path = syn::parse_str::<syn::Path>(text).ok()?;
    Some(facts::path_segments(&path))
}
pub(super) fn ty(text: &str) -> Option<Vec<String>> {
    facts::type_path(&syn::parse_str(text).ok()?)
}
/// Whether a path's first segment is a root, which no import, glob or
/// block-local item can bind: a path keyword, or the empty segment of a
/// leading `::`.
pub(super) fn path_root(segment: &str) -> bool {
    matches!(segment, "" | "crate" | "self" | "super" | "Self")
}
impl Index {
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
        } else {
            match self.glob(scope, imports, &p) {
                Glob::Bound(path) => return Some(path),
                Glob::Ambiguous => return None,
                Glob::Unbound => {}
            }
        }
        self.exported(&self.relative(scope, &p))
    }
    /// What the module's globs of workspace modules bind a path's first
    /// name to. Unbound when no glob applies: the module declares the name,
    /// no workspace glob binds it, or it is a path root. A glob binds before
    /// the extern prelude, so a name it binds shadows a crate of that name.
    /// Ambiguous when globs bind it to different items (or through an
    /// ambiguous import or re-export), which is never picked.
    ///
    /// A glob of the module itself or of an ancestor binds every name there,
    /// private items and `use` declarations included; a glob of any other
    /// module binds its non-private items and its re-exports. A name a
    /// glob of an external module, or a glob inside the globbed module,
    /// might also bind is not considered: in code that compiles, two globs
    /// cannot both bind a name that is used.
    fn glob(
        &self,
        scope: &[String],
        imports: &BTreeMap<String, Vec<Vec<String>>>,
        p: &[String],
    ) -> Glob {
        let (Some(first), Some(globs)) = (p.first(), imports.get("*")) else {
            return Glob::Unbound;
        };
        if path_root(first) || self.declares_name(scope, first) {
            return Glob::Unbound;
        }
        let mut found = BTreeSet::new();
        for glob in globs {
            let Some(module) = self.exported(&self.relative(scope, glob)) else {
                continue;
            };
            let Some(names) = self.modules.get(&module) else {
                continue;
            };
            let inside = scope.starts_with(&module);
            let mut key = module.clone();
            key.push(first.clone());
            let bound = if names
                .items
                .get(first)
                .is_some_and(|private| inside || !private)
                || self.exports.contains_key(&key)
            {
                key.iter().chain(&p[1..]).cloned().collect::<Vec<_>>()
            } else if inside && let Some(sources) = names.imports.get(first) {
                let [source] = sources.as_slice() else {
                    return Glob::Ambiguous;
                };
                let mut path = self.relative(&module, source);
                path.extend_from_slice(&p[1..]);
                path
            } else {
                continue;
            };
            let Some(bound) = self.exported(&bound) else {
                return Glob::Ambiguous;
            };
            found.insert(bound);
        }
        match found.len() {
            0 => Glob::Unbound,
            1 => found.pop_first().map_or(Glob::Unbound, Glob::Bound),
            _ => Glob::Ambiguous,
        }
    }
    /// `path` written in the module at `scope`, made absolute. A relative
    /// path names the module's own items first: a local `mod json` is
    /// `self::json` even in a workspace with a crate `json`, as in Rust,
    /// where a name in scope shadows the extern prelude.
    pub(super) fn relative(&self, scope: &[String], path: &[String]) -> Vec<String> {
        match path.first() {
            Some(first) if self.crates.contains(first) && self.declares_path_root(scope, first) => {
                let mut local = scope.to_vec();
                local.extend_from_slice(path);
                local
            }
            _ => normalize(scope, path, &self.crates),
        }
    }
    /// Whether the module at `scope` declares `name` in the type namespace,
    /// where a path's first segment is read: a module (inline or file) or a
    /// type. A function or constant of that name does not shadow a crate.
    fn declares_path_root(&self, scope: &[String], name: &str) -> bool {
        let mut key = scope.to_vec();
        key.push(name.to_owned());
        self.modules.contains_key(&key)
            || self.types.contains(&key)
            || self.aliases.contains_key(&key)
    }
    /// Whether the module at `scope` declares `name` itself: an item, a
    /// re-export, or a module with items under it. Such a name shadows every
    /// glob import.
    pub(super) fn declares_name(&self, scope: &[String], name: &str) -> bool {
        if self
            .modules
            .get(scope)
            .is_some_and(|names| names.items.contains_key(name))
        {
            return true;
        }
        let mut key = scope.to_vec();
        key.push(name.to_owned());
        let from = (Bound::Included(key.as_slice()), Bound::Unbounded);
        let under = |k: &Vec<String>| k.starts_with(&key);
        self.names
            .range::<[String], _>(from)
            .next()
            .is_some_and(|(k, _)| under(k))
            || self
                .types
                .range::<[String], _>(from)
                .next()
                .is_some_and(under)
            || self
                .aliases
                .range::<[String], _>(from)
                .next()
                .is_some_and(|(k, _)| under(k))
            || self
                .exports
                .range::<[String], _>(from)
                .next()
                .is_some_and(|(k, _)| under(k))
    }
    pub(crate) fn canonical(&self, function: usize, p: &[String]) -> Option<Vec<String>> {
        let f = self.functions.get(function)?;
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
            return Target::Summarized(Summary::new(
                SummaryKind::ReexportAmbiguous,
                "ambiguous or cyclic public reexport",
            ));
        };
        let p = resolved.as_slice();
        for (i, b) in self.boundaries.iter().enumerate() {
            if p.starts_with(&b.path) {
                let mut entry = b.path.clone();
                entry.push(b.module.entry.clone());
                let Some(entry) = self.exported(&entry) else {
                    return Target::Summarized(Summary::new(
                        SummaryKind::StageEntryAmbiguous,
                        "ambiguous stage entry reexport",
                    ));
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
                return Target::Summarized(Summary::about(
                    SummaryKind::SupportApi,
                    &b.module.name,
                    format!("{} support API, outside its declared entry", b.module.name),
                ));
            }
        }
        match self.names.get(p).map(Vec::as_slice) {
            Some([i]) => Target::Function(*i),
            Some(candidates) => Target::Summarized(Summary::new(
                SummaryKind::StaticCallAmbiguous,
                format!("ambiguous static call: {} candidates", candidates.len()),
            )),
            None if p.len() > 1 && self.foreign.contains(&p[..p.len() - 1]) => {
                let owner = p[..p.len() - 1].join("::");
                Target::Summarized(Summary::about(
                    SummaryKind::ForeignImplOwner,
                    &owner,
                    format!(
                        "{owner} is not a type the workspace source declares (a foreign type, or one a macro declares); its trait impls dispatch on argument types and inherent methods, so the call is not resolved"
                    ),
                ))
            }
            None => {
                let path = p.join("::");
                Target::Summarized(Summary::about(
                    SummaryKind::UnresolvedPath,
                    &path,
                    format!("unresolved or external path {path}"),
                ))
            }
        }
    }
    /// Resolve a call in a function selected from this index. Positions from
    /// another index have no identity here, even when numerically in range.
    /// An out-of-range position returns Target::InvalidFunction.
    pub fn resolve(&self, function: usize, call: &Call) -> Target {
        let Some(f) = self.functions.get(function) else {
            return Target::InvalidFunction(function);
        };
        if let Some(receiver) = &call.receiver {
            return self.resolve_receiver(function, f, receiver, call);
        }
        let Some(p) = path(&call.spelling) else {
            return Target::Summarized(Summary::new(
                SummaryKind::IndirectCall,
                "indirect or unsupported call syntax",
            ));
        };
        if p.len() == 1
            && f.flow
                .bindings
                .iter()
                .any(|b| b.name == p[0] && b.visible_at(&call.site))
        {
            return Target::Summarized(Summary::about(
                SummaryKind::ValueCall,
                &p[0],
                format!("value call {}", p[0]),
            ));
        }
        if shadowed(&f.flow.local_names, &p[0]) {
            return Target::Summarized(Summary::about(
                SummaryKind::LocalShadow,
                &p[0],
                format!(
                    "block-local item or import {} shadows the module's names; not resolved",
                    p[0]
                ),
            ));
        }
        if p.len() == 1 && f.flow.imports.contains_key(&p[0]) {
            let local = self.relative(&f.scope, &p);
            if self.names.contains_key(&local) {
                return Target::Summarized(Summary::about(
                    SummaryKind::ImportNamespaceClash,
                    &p[0],
                    "local value and import share a name; namespace unresolved",
                ));
            }
        }
        let target = self.canonical(function, &p).map_or_else(
            || {
                Target::Summarized(Summary::new(
                    SummaryKind::ImportAmbiguous,
                    "ambiguous source import",
                ))
            },
            |p| self.classify(&p),
        );
        self.as_written(f, target, &p)
    }
    fn resolve_receiver(
        &self,
        function: usize,
        f: &Function,
        receiver: &str,
        call: &Call,
    ) -> Target {
        if receiver == "self" {
            return self
                .canonical(function, &["Self".into(), call.spelling.clone()])
                .map_or_else(
                    || {
                        Target::Summarized(Summary::new(
                            SummaryKind::SelfOwnerUnknown,
                            "self owner not established",
                        ))
                    },
                    |p| self.classify(&p),
                );
        }
        let bindings: Vec<_> = f
            .flow
            .bindings
            .iter()
            .filter(|b| b.name == receiver && b.visible_at(&call.site))
            .collect();
        if bindings.len() != 1 {
            return Target::Summarized(Summary::about(
                SummaryKind::ReceiverTypeUnknown,
                receiver,
                format!("receiver {receiver}: no unique explicit source type"),
            ));
        }
        if let Some(name) = local_owner(f, bindings[0]) {
            return Target::Summarized(Summary::about(
                SummaryKind::LocalShadow,
                &name,
                format!(
                    "receiver {receiver}: its type comes through {name}, which the body binds itself (a local item, `use` or value) and shadows the module's; not resolved"
                ),
            ));
        }
        let owner = self.binding_owner(function, bindings[0]);
        let target = owner.map_or_else(
            || {
                Target::Summarized(Summary::about(
                    SummaryKind::ReceiverInitializerUnknown,
                    receiver,
                    format!("receiver {receiver}: initializer result not established"),
                ))
            },
            |mut p| {
                p.push(call.spelling.clone());
                self.classify(&p)
            },
        );
        match bindings[0].ty.as_deref().and_then(ty) {
            Some(mut written) => {
                written.push(call.spelling.clone());
                self.as_written(f, target, &written)
            }
            None => target,
        }
    }

    /// An unresolved path whose first name nothing in scope binds (no import,
    /// item, module or crate root) is a prelude or external name such as
    /// `Box` or `Vec`. Report it as written, not under the caller's module,
    /// which no source text says.
    fn as_written(&self, f: &Function, target: Target, written: &[String]) -> Target {
        let Target::Summarized(reason) = &target else {
            return target;
        };
        let Some(first) = written.first() else {
            return target;
        };
        if reason.kind != SummaryKind::UnresolvedPath || self.binds(f, first) {
            return target;
        }
        let path = written.join("::");
        let message = if f.flow.imports.contains_key("*") {
            format!(
                "unresolved or external path {path}: nothing in this module, its explicit imports or the workspace modules it glob-imports binds {first}, so it may come from an external glob, the prelude or another package"
            )
        } else {
            format!(
                "unresolved or external path {path}: nothing in this module or its imports binds {first}, so it may be a prelude or external name"
            )
        };
        Target::Summarized(Summary::about(SummaryKind::UnresolvedPath, &path, message))
    }

    /// Whether `first` names something in `f`'s scope: a path keyword, an
    /// import, a workspace crate, or an item or module declared there.
    fn binds(&self, f: &Function, first: &str) -> bool {
        if matches!(first, "crate" | "self" | "super" | "Self")
            || f.flow.imports.contains_key(first)
            || self.crates.contains(first)
        {
            return true;
        }
        self.declares_name(&f.scope, first)
    }

    /// Prefer the written type; otherwise inspect a unique initializer's return type.
    /// A binding whose type or initializer the body shadows has none: see
    /// [`local_owner`].
    fn binding_owner(&self, function: usize, binding: &Binding) -> Option<Vec<String>> {
        let f = self.functions.get(function)?;
        if local_owner(f, binding).is_some() {
            return None;
        }
        if let Some(written) = binding.ty.as_deref().and_then(ty) {
            return self.canonical(function, &written);
        }
        let initializer = path(binding.initializer.as_deref()?)?;
        let canonical = self.canonical(function, &initializer)?;
        let exported = self.exported(&canonical)?;
        let [target] = self.names.get(&exported)?.as_slice() else {
            return None;
        };
        match self.functions[*target].flow.output.as_str() {
            "Self" => Some(exported[..exported.len() - 1].to_vec()),
            output => self.canonical(*target, &ty(output)?),
        }
    }
}

/// The name through which a binding's type would come that the body binds
/// itself: the first segment of its written type, or of its initializer's
/// path, when a local item or `use` shadows it, or an initializer that is a
/// local value. The module-level reading of it would name another item.
fn local_owner(f: &Function, binding: &Binding) -> Option<String> {
    if let Some(written) = binding.ty.as_deref().and_then(ty) {
        let first = written.into_iter().next()?;
        return shadowed(&f.flow.local_names, &first).then_some(first);
    }
    let initializer = path(binding.initializer.as_deref()?)?;
    let value = initializer.len() == 1
        && f.flow
            .bindings
            .iter()
            .any(|b| b.name == initializer[0] && b.visible_at(&binding.site));
    let first = initializer.into_iter().next()?;
    (value || shadowed(&f.flow.local_names, &first)).then_some(first)
}

/// One step of following a path: to a new path, or ambiguous.
pub(super) enum Resolved {
    To(Vec<String>),
    Ambiguous,
}

/// What glob imports bind a name to.
enum Glob {
    Unbound,
    Bound(Vec<String>),
    Ambiguous,
}

pub(super) fn normalize(
    scope: &[String],
    path: &[String],
    crates: &BTreeSet<String>,
) -> Vec<String> {
    let mut path = path.to_vec();
    // A leading `::`: the path is rooted at the crate it names.
    if path.first().is_some_and(String::is_empty) {
        path.remove(0);
        return path;
    }
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

/// Whether a body-local item or import may bind a path's first segment: by
/// name, or through a body-local glob unless the segment is a path keyword.
fn shadowed(local_names: &[String], first: &str) -> bool {
    local_names.iter().any(|name| name == first)
        || (local_names.iter().any(|name| name == "*") && !path_root(first))
}
