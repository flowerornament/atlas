//! Conservative call lookup through explicit imports, reexports and signature types.
//! Ambiguous, indirect and unsupported calls stay summarized; no Rust type inference.
use super::{Function, Index, Target};
use crate::{
    analysis::workflow::{Binding, Call},
    architecture::Kind,
};
use std::collections::{BTreeMap, BTreeSet};
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
                    || Target::Summarized("self owner not established".into()),
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
            return Target::Summarized(format!(
                "receiver {receiver}: no unique explicit source type"
            ));
        }
        let owner = self.binding_owner(function, bindings[0]);
        owner.map_or_else(
            || {
                Target::Summarized(format!(
                    "receiver {receiver}: initializer result not established"
                ))
            },
            |mut p| {
                p.push(call.spelling.clone());
                self.classify(&p)
            },
        )
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
