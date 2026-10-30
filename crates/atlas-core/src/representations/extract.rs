//! Join AST observations to Cargo namespaces without selecting a diagram population.
use super::{
    collect::{Collector, Record},
    types::{
        Constructor, Facts, Function, Reference, ResolvedCall, Role, Type, UnknownCall, UnknownType,
    },
};
use crate::{
    Result,
    architecture::Architecture,
    model::SourceGraph,
    source_index::{Index, Target},
};
use cargo_metadata::Metadata;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use syn::{spanned::Spanned, visit::Visit};
type Addresses = BTreeMap<Vec<String>, Vec<String>>;
use crate::source_text::text;

fn resolve(r: &Record, p: &syn::Path, index: &Index, addresses: &Addresses) -> Option<String> {
    let parts: Vec<_> = p.segments.iter().map(|s| s.ident.to_string()).collect();
    if parts
        .first()
        .is_some_and(|name| r.parameters.contains(name))
    {
        return None;
    }
    let mut targets = BTreeSet::new();
    for scope in index.scopes(&r.file, &r.module) {
        let path = index.in_scope(&scope, &r.imports, &parts)?;
        let [one] = addresses.get(&path)?.as_slice() else {
            return None;
        };
        targets.insert(one.clone());
    }
    if targets.len() == 1 {
        targets.pop_first()
    } else {
        None
    }
}
struct References<'a> {
    r: &'a Record,
    index: &'a Index,
    addresses: &'a Addresses,
    source: &'a str,
    line: usize,
    out: Vec<Reference>,
    unknown: Vec<UnknownType>,
}
impl References<'_> {
    fn field(&mut self, t: &syn::Type, role: Role, slot: &str) {
        self.line = t.span().start().line;
        let expression = text(self.source, t.span());
        self.ty(t, &role, slot, &expression, false, &[]);
    }
    fn ty(
        &mut self,
        t: &syn::Type,
        role: &Role,
        slot: &str,
        expression: &str,
        borrow: bool,
        wrappers: &[String],
    ) {
        match t {
            syn::Type::Reference(r) => self.ty(&r.elem, role, slot, expression, true, wrappers),
            syn::Type::Path(p) if p.qself.is_none() => {
                if p.path
                    .segments
                    .first()
                    .is_some_and(|p| self.r.parameters.contains(&p.ident.to_string()))
                {
                    return;
                }
                if p.path.is_ident("Self") {
                    if let Some(owner) = &self.r.owner {
                        self.ty(owner, role, slot, expression, borrow, wrappers);
                    }
                    return;
                }
                if let Some(to) = resolve(self.r, &p.path, self.index, self.addresses) {
                    self.out.push(Reference {
                        from: self.r.id.clone(),
                        to,
                        role: *role,
                        slot: slot.into(),
                        expression: expression.into(),
                        line: self.line,
                        borrow,
                        wrappers: wrappers.to_vec(),
                    });
                } else {
                    self.unknown.push(UnknownType{from:self.r.id.clone(),spelling:text(self.source,p.span()),role: *role,reason:"not a uniquely resolved workspace struct/enum (may be external, alias, trait, primitive or ambiguous)".into()});
                }
                for segment in &p.path.segments {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        for (i, t) in args
                            .args
                            .iter()
                            .filter_map(|a| {
                                if let syn::GenericArgument::Type(t) = a {
                                    Some(t)
                                } else {
                                    None
                                }
                            })
                            .enumerate()
                        {
                            let mut w = wrappers.to_vec();
                            w.push(format!("{}[{i}]", segment.ident));
                            self.ty(t, role, slot, expression, borrow, &w);
                        }
                    }
                }
            }
            syn::Type::Tuple(t) => {
                for (i, t) in t.elems.iter().enumerate() {
                    let mut w = wrappers.to_vec();
                    w.push(format!("tuple[{i}]"));
                    self.ty(t, role, slot, expression, borrow, &w);
                }
            }
            syn::Type::Slice(t) => {
                let mut w = wrappers.to_vec();
                w.push("slice".into());
                self.ty(&t.elem, role, slot, expression, borrow, &w);
            }
            syn::Type::Array(t) => {
                let mut w = wrappers.to_vec();
                w.push("array".into());
                self.ty(&t.elem, role, slot, expression, borrow, &w);
            }
            syn::Type::Paren(t) => self.ty(&t.elem, role, slot, expression, borrow, wrappers),
            syn::Type::Group(t) => self.ty(&t.elem, role, slot, expression, borrow, wrappers),
            _ => self.unknown.push(UnknownType {
                from: self.r.id.clone(),
                spelling: text(self.source, t.span()),
                role: *role,
                reason: "unsupported type syntax".into(),
            }),
        }
    }
    fn signature(&mut self, sig: &syn::Signature) {
        for input in &sig.inputs {
            match input {
                syn::FnArg::Typed(a) => {
                    self.field(&a.ty, Role::Input, &text(self.source, a.pat.span()));
                }
                syn::FnArg::Receiver(receiver) => {
                    self.line = receiver.span().start().line;
                    let Some(owner) = &self.r.owner else { continue };
                    match &receiver.kind {
                        syn::ReceiverKind::Typed(_, ty) => self.field(ty, Role::Input, "self"),
                        kind => {
                            let expression = text(self.source, receiver.span());
                            self.ty(
                                owner,
                                &Role::Input,
                                "self",
                                &expression,
                                matches!(kind, syn::ReceiverKind::Reference(..)),
                                &[],
                            );
                        }
                    }
                }
            }
        }
        if let syn::ReturnType::Type(_, t) = &sig.output {
            self.field(t, Role::Output, "return");
        }
    }
}
pub(super) fn extract(root: &Path, metadata: &Metadata, graph: &SourceGraph) -> Result<Facts> {
    let root = root.canonicalize()?;
    let sources = crate::source_index::read_sources(&root, graph)?;
    from_sources(&sources, metadata, graph)
}
pub(crate) fn from_sources(
    sources: &crate::source_index::Sources,
    metadata: &Metadata,
    graph: &SourceGraph,
) -> Result<Facts> {
    from_selected_sources(
        sources,
        metadata,
        graph,
        crate::population::TestSelection::Production,
    )
}
pub(crate) fn from_selected_sources(
    sources: &crate::source_index::Sources,
    metadata: &Metadata,
    graph: &SourceGraph,
    tests: crate::population::TestSelection,
) -> Result<Facts> {
    let index = Index::from_selected_sources(
        sources,
        metadata,
        graph,
        &Architecture {
            schema: 1,
            provenance: vec![],
            modules: vec![],
            workflows: vec![],
        },
        tests,
    )?;
    let mut records = Vec::new();
    let mut literals = Vec::new();
    let mut constants = Vec::new();
    for file in graph
        .physical_files()
        .filter(|f| tests.admits_file(f.test_only))
    {
        let parsed = &sources[&file.path];
        let (source, ast) = (parsed.text(), parsed.ast(&file.path)?);
        let selection = crate::population::SyntaxSelection::Selected {
            source: parsed.source(),
            tests,
        };
        let mut c = Collector {
            file: &file.path,
            source,
            selection,
            parameters: BTreeSet::new(),
            trait_impl: false,
            module: vec![],
            imports: crate::workflow::facts::imports(&ast.items, selection),
            owner: None,
            function: None,
            records: vec![],
            constructors: vec![],
            constants: vec![],
        };
        c.visit_file(ast);
        constants.extend(c.constants);
        records.extend(c.records);
        literals.extend(c.constructors);
    }
    let mut addresses = Addresses::new();
    for r in records.iter().filter(|r| r.signature.is_none()) {
        for mut scope in index.scopes(&r.file, &r.module) {
            scope.push(r.name.clone());
            addresses.entry(scope).or_default().push(r.id.clone());
        }
    }
    let limitations = [
        match tests {
            crate::population::TestSelection::Production => "Written production source only; test-implying gates and test-only files excluded; other cfg alternatives retained.",
            crate::population::TestSelection::All => "All written source, including test-only targets, modules and declarations; no cfg feasibility evaluation.",
            crate::population::TestSelection::OutsideTestModules => "Written source outside test-only files/modules; individually test-gated declarations retained.",
        },
        "No macro expansion, alias/trait normalization, indirect retention or complete Rust type inference.",
        "Literal sites and explicit Type::default() calls are distinct observations; no exhaustive construction claim (macros, Clone, inferred Default::default and deferred bodies excluded).",
        "Calls resolve only conservative explicit paths and receivers. Unknowns are retained; contracts do not prove value identity, runtime order or semantic duplication.",
    ].map(str::to_owned).to_vec();
    let mut facts = Facts {
        schema: 1,
        types: vec![],
        functions: vec![],
        references: vec![],
        constructors: vec![],
        constants,
        default_calls: vec![],
        calls: vec![],
        unknown_types: vec![],
        unknown_calls: vec![],
        fingerprints: graph
            .physical_files()
            .filter(|f| tests.admits_file(f.test_only))
            .map(|f| (f.path.clone(), f.content_fingerprint.clone()))
            .collect(),
        limitations,
    };
    for r in &records {
        let source = sources[&r.file].text();
        let mut refs = References {
            r,
            index: &index,
            addresses: &addresses,
            source,
            line: r.line,
            out: vec![],
            unknown: vec![],
        };
        for (name, t) in &r.fields {
            refs.field(t, Role::Field, name);
        }
        if let Some(sig) = &r.signature {
            refs.signature(sig);
            facts.functions.push(Function {
                id: r.id.clone(),
                name: r.name.clone(),
                qualified: r
                    .module
                    .iter()
                    .chain(std::iter::once(&r.name))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::"),
                file: r.file.clone(),
                line: r.line,
                signature: r.text.clone(),
                public: r.public,
                trait_impl: r.trait_impl,
            });
        } else {
            facts.types.push(Type {
                id: r.id.clone(),
                name: r.name.clone(),
                qualified: r
                    .module
                    .iter()
                    .chain(std::iter::once(&r.name))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::"),
                file: r.file.clone(),
                line: r.line,
                fields: r
                    .fields
                    .iter()
                    .map(|(name, t)| (name.clone(), text(source, t.span())))
                    .collect(),
            });
        }
        facts.references.extend(refs.out);
        facts.unknown_types.extend(refs.unknown);
    }
    let by_id: BTreeMap<_, _> = records.iter().map(|r| (&r.id, r)).collect();
    for (from, path, owner, line) in literals {
        let r = by_id[&from];
        let path = if path.is_ident("Self") {
            if let Some(syn::Type::Path(p)) = owner {
                p.path
            } else {
                continue;
            }
        } else {
            path
        };
        if let Some(to) = resolve(r, &path, &index, &addresses) {
            facts.constructors.push(Constructor { from, to, line });
        }
    }
    let sites: BTreeMap<_, _> = records
        .iter()
        .filter(|r| r.signature.is_some())
        .map(|r| ((r.file.as_str(), r.line, r.column), r.id.clone()))
        .collect();
    let mut call_positions = BTreeMap::new();
    let mut observed = Vec::<(
        String,
        crate::workflow::Call,
        BTreeSet<String>,
        BTreeSet<String>,
    )>::new();
    for (i, f) in index.functions.iter().enumerate() {
        let Some(from) = sites.get(&(
            f.flow.site.path.as_str(),
            f.flow.site.line,
            f.flow.site.column,
        )) else {
            continue;
        };
        for call in &f.flow.calls {
            // Classify syntax separately from callee resolution: a derived Default
            // body need not be present, and an inherent `default` may also exist.
            if call.receiver.is_none()
                && call.arguments.is_empty()
                && let Ok(mut path) = syn::parse_str::<syn::Path>(&call.spelling)
                && path.segments.len() > 1
                && path.segments.last().is_some_and(|s| s.ident == "default")
            {
                path.segments.pop();
                path.segments.pop_punct();
                let r = by_id[from];
                if path.is_ident("Self")
                    && let Some(syn::Type::Path(owner)) = &r.owner
                {
                    path = owner.path.clone();
                }
                if let Some(to) = resolve(r, &path, &index, &addresses) {
                    facts.default_calls.push(Constructor {
                        from: from.clone(),
                        to,
                        line: call.site.line,
                    });
                }
            }
            let key = (from.clone(), call.site.clone());
            let position = *call_positions.entry(key).or_insert_with(|| {
                observed.push((from.clone(), call.clone(), BTreeSet::new(), BTreeSet::new()));
                observed.len() - 1
            });
            let (_, _, targets, unknowns) = &mut observed[position];
            match index.resolve(i, call) {
                Target::Function(j) => {
                    let site = &index.functions[j].flow.site;
                    if let Some(to) = sites.get(&(site.path.as_str(), site.line, site.column)) {
                        targets.insert(to.clone());
                    } else {
                        unknowns
                            .insert("resolved body lacks a selected representation site".into());
                    }
                }
                Target::Summarized(reason) => {
                    unknowns.insert(reason);
                }
                Target::Boundary(..) => {
                    unknowns.insert("unresolved boundary".into());
                }
            }
        }
    }
    for (from, call, mut targets, unknowns) in observed {
        if targets.len() == 1 && unknowns.is_empty() {
            if let Some(to) = targets.pop_first() {
                facts.calls.push(ResolvedCall {
                    from,
                    to,
                    site: call.site,
                });
            }
        } else {
            let reason = if targets.len() > 1 {
                "Cargo/module contexts resolve to different physical bodies".into()
            } else {
                unknowns.into_iter().collect::<Vec<_>>().join("; ")
            };
            facts.unknown_calls.push(UnknownCall { from, call, reason });
        }
    }
    let mut defaults = BTreeSet::new();
    facts
        .default_calls
        .retain(|call| defaults.insert((call.from.clone(), call.to.clone(), call.line)));

    Ok(facts)
}
