//! Conservative call lookup through explicit imports, reexports and signature types.
//! Ambiguous, indirect and unsupported calls stay summarized; no Rust type inference.
use super::{Function, Index, Summary, SummaryKind, Target};
use crate::{
    analysis::workflow::{Binding, Call},
    architecture::Kind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use syn::Type;

pub(super) fn path(text: &str) -> Option<Vec<String>> {
    let path = syn::parse_str::<syn::Path>(text).ok()?;
    Some(path.segments.iter().map(|s| s.ident.to_string()).collect())
}
pub(super) fn ty(text: &str) -> Option<Vec<String>> {
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
            && f.flow.bindings.iter().any(|b| {
                b.name == p[0]
                    && (b.site.end_line, b.site.end_column) <= (call.site.line, call.site.column)
            })
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
            let local = normalize(&f.scope, &p, &self.crates);
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
            .filter(|b| {
                b.name == receiver
                    && (b.site.end_line, b.site.end_column) <= (call.site.line, call.site.column)
            })
            .collect();
        if bindings.len() != 1 {
            return Target::Summarized(Summary::about(
                SummaryKind::ReceiverTypeUnknown,
                receiver,
                format!("receiver {receiver}: no unique explicit source type"),
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
        Target::Summarized(Summary::about(
            SummaryKind::UnresolvedPath,
            &path,
            format!(
                "unresolved or external path {path}: nothing in this module or its imports binds {first}, so it may be a prelude or external name"
            ),
        ))
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
        let mut key = f.scope.clone();
        key.push(first.to_owned());
        // The first key at or after `key` is under it when any key is.
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

    /// Prefer the written type; otherwise inspect a unique initializer's return type.
    fn binding_owner(&self, function: usize, binding: &Binding) -> Option<Vec<String>> {
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

pub(super) fn normalize(
    scope: &[String],
    path: &[String],
    crates: &BTreeSet<String>,
) -> Vec<String> {
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

/// Whether a body-local item or import may bind a path's first segment: by
/// name, or through a body-local glob unless the segment is a path keyword.
fn shadowed(local_names: &[String], first: &str) -> bool {
    local_names.iter().any(|name| name == first)
        || (local_names.iter().any(|name| name == "*")
            && !matches!(first, "crate" | "self" | "super" | "Self"))
}
