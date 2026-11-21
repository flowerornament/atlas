//! Join AST observations to Cargo namespaces without selecting a diagram population.
use super::{
    collect::{Collector, Record},
    types::{
        Constructor, Facts, Function, Reference, ResolvedCall, Role, Type, UnknownCall, UnknownType,
    },
};
use crate::{
    Result,
    analysis::index::{Index, Summary, SummaryKind, Target},
    architecture::Architecture,
    model::SourceGraph,
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
        let wrapped = |label: String| {
            let mut w = wrappers.to_vec();
            w.push(label);
            w
        };
        match t {
            syn::Type::Reference(r) => self.ty(&r.elem, role, slot, expression, true, wrappers),
            syn::Type::Path(p) if p.qself.is_none() => {
                self.path_ty(&p.path, role, slot, expression, borrow, wrappers);
            }
            syn::Type::Tuple(t) => {
                for (i, t) in t.elems.iter().enumerate() {
                    self.ty(
                        t,
                        role,
                        slot,
                        expression,
                        borrow,
                        &wrapped(format!("tuple[{i}]")),
                    );
                }
            }
            syn::Type::Slice(t) => {
                self.ty(
                    &t.elem,
                    role,
                    slot,
                    expression,
                    borrow,
                    &wrapped("slice".into()),
                );
            }
            syn::Type::Array(t) => {
                self.ty(
                    &t.elem,
                    role,
                    slot,
                    expression,
                    borrow,
                    &wrapped("array".into()),
                );
            }
            syn::Type::Paren(t) => self.ty(&t.elem, role, slot, expression, borrow, wrappers),
            syn::Type::Group(t) => self.ty(&t.elem, role, slot, expression, borrow, wrappers),
            _ => self.unknown(t.span(), role, slot, "unsupported type syntax"),
        }
    }
    /// A written type path: a type parameter is skipped, `Self` is the
    /// owner, anything else is a reference or an unknown, and each generic
    /// type argument is read inside its wrapper (`Vec[0]`).
    fn path_ty(
        &mut self,
        path: &syn::Path,
        role: &Role,
        slot: &str,
        expression: &str,
        borrow: bool,
        wrappers: &[String],
    ) {
        if path
            .segments
            .first()
            .is_some_and(|p| self.r.parameters.contains(&p.ident.to_string()))
        {
            return;
        }
        if path.is_ident("Self") {
            if let Some(owner) = &self.r.owner {
                self.ty(owner, role, slot, expression, borrow, wrappers);
            }
            return;
        }
        if let Some(to) = resolve(self.r, path, self.index, self.addresses) {
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
            self.unknown(
                path.span(),
                role,
                slot,
                "not a uniquely resolved workspace struct/enum/alias (may be external, trait, primitive or ambiguous)",
            );
        }
        for segment in &path.segments {
            let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                continue;
            };
            let types = args.args.iter().filter_map(|a| match a {
                syn::GenericArgument::Type(t) => Some(t),
                _ => None,
            });
            for (i, t) in types.enumerate() {
                let mut w = wrappers.to_vec();
                w.push(format!("{}[{i}]", segment.ident));
                self.ty(t, role, slot, expression, borrow, &w);
            }
        }
    }
    fn unknown(&mut self, span: proc_macro2::Span, role: &Role, slot: &str, reason: &str) {
        let start = span.start();
        self.unknown.push(UnknownType {
            from: self.r.id.clone(),
            spelling: text(self.source, span),
            role: *role,
            reason: reason.into(),
            slot: slot.into(),
            line: start.line,
            column: start.column + 1,
        });
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
    let sources = crate::analysis::sources::read_sources(&root, graph)?;
    from_sources(&sources, metadata, graph)
}
pub(in crate::analysis) fn from_sources(
    sources: &crate::analysis::sources::Sources,
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
/// The facts of the files `tests` admits: each record's declaration and
/// references, constructions, `Type::default()` calls, and calls resolved or
/// left unknown, in that order.
pub(in crate::analysis) fn from_selected_sources(
    sources: &crate::analysis::sources::Sources,
    metadata: &Metadata,
    graph: &SourceGraph,
    tests: crate::population::TestSelection,
) -> Result<Facts> {
    let collected = collect(sources, graph, tests)?;
    let index =
        Index::from_selected_sources(sources, metadata, graph, &Architecture::empty(), tests)?;
    let addresses = addresses(&collected.records, &index);
    let joined = Joined {
        index: &index,
        addresses: &addresses,
    };
    let mut facts = Facts {
        schema: 1,
        types: vec![],
        functions: vec![],
        references: vec![],
        constructors: vec![],
        constants: collected.constants,
        default_calls: vec![],
        calls: vec![],
        unknown_types: vec![],
        unknown_calls: vec![],
        fingerprints: graph
            .physical_files()
            .filter(|f| tests.admits_file(f.test_only))
            .map(|f| (f.path.clone(), f.content_fingerprint.clone()))
            .collect(),
        limitations: limitations(tests),
    };
    for r in &collected.records {
        declare(sources, r, &joined, &mut facts);
    }
    let by_id: BTreeMap<_, _> = collected.records.iter().map(|r| (&r.id, r)).collect();
    for (from, path, owner, line) in collected.literals {
        let r = by_id[&from];
        let path = match (path.is_ident("Self"), owner) {
            (false, _) => path,
            (true, Some(syn::Type::Path(p))) => p.path,
            (true, _) => continue,
        };
        if let Some(to) = resolve(r, &path, &index, &addresses) {
            facts.constructors.push(Constructor { from, to, line });
        }
    }
    let observed = observe_calls(&collected.records, &by_id, &joined, &mut facts)?;
    settle_calls(observed, &mut facts);
    let mut defaults = BTreeSet::new();
    facts
        .default_calls
        .retain(|call| defaults.insert((call.from.clone(), call.to.clone(), call.line)));
    Ok(facts)
}

/// What one pass over each syntax tree collects.
struct Collected {
    constants: Vec<super::types::Constant>,
    records: Vec<Record>,
    literals: Vec<(String, syn::Path, Option<syn::Type>, usize)>,
}
/// The index and type addresses the collected records are joined through.
struct Joined<'a> {
    index: &'a Index,
    addresses: &'a Addresses,
}
/// One call site: its caller, the call, the bodies it resolves to, and the
/// reasons it may not.
type Observed = (
    String,
    crate::workflow::Call,
    BTreeSet<String>,
    BTreeMap<String, Summary>,
);

/// Collect first, one syntax tree at a time; the index reuses the workflow
/// facts each visit cached.
fn collect(
    sources: &crate::analysis::sources::Sources,
    graph: &SourceGraph,
    tests: crate::population::TestSelection,
) -> Result<Collected> {
    let mut collected = Collected {
        constants: Vec::new(),
        records: Vec::new(),
        literals: Vec::new(),
    };
    for file in graph
        .physical_files()
        .filter(|f| tests.admits_file(f.test_only))
    {
        let parsed = &sources[&file.path];
        let source = parsed.text();
        let selection = crate::population::SyntaxSelection::Selected {
            source: parsed.source(),
            tests,
        };
        let c = parsed.visit_once(&file.path, tests, |ast| {
            let mut c = Collector {
                file: &file.path,
                test_file: file.test_only,
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
            (c.constants, c.records, c.constructors)
        })?;
        collected.constants.extend(c.0);
        collected.records.extend(c.1);
        collected.literals.extend(c.2);
    }
    Ok(collected)
}
/// Each type record's paths, in every scope it is reachable from.
fn addresses(records: &[Record], index: &Index) -> Addresses {
    let mut addresses = Addresses::new();
    for r in records.iter().filter(|r| r.signature.is_none()) {
        for mut scope in index.scopes(&r.file, &r.module) {
            scope.push(r.name.clone());
            addresses.entry(scope).or_default().push(r.id.clone());
        }
    }
    addresses
}
fn limitations(tests: crate::population::TestSelection) -> Vec<String> {
    [
        match tests {
            crate::population::TestSelection::Production => "Written production source only; test-implying gates and test-only files excluded; other cfg alternatives retained.",
            crate::population::TestSelection::All => "All written source, including test-only targets, modules and declarations; no cfg feasibility evaluation.",
            crate::population::TestSelection::OutsideTestModules => "Written source outside test-only files/modules; individually test-gated declarations retained.",
        },
        "No macro expansion, alias/trait normalization, indirect retention or complete Rust type inference.",
        "Literal sites and explicit Type::default() calls are distinct observations; no exhaustive construction claim (macros, Clone, inferred Default::default and deferred bodies excluded).",
        "Calls resolve only conservative explicit paths and receivers. Unknowns are retained; contracts do not prove value identity, runtime order or semantic duplication.",
    ].map(str::to_owned).to_vec()
}
/// One record as a function or a type, with the types it references.
fn declare(
    sources: &crate::analysis::sources::Sources,
    r: &Record,
    joined: &Joined<'_>,
    facts: &mut Facts,
) {
    let source = sources[&r.file].text();
    let mut refs = References {
        r,
        index: joined.index,
        addresses: joined.addresses,
        source,
        line: r.line,
        out: vec![],
        unknown: vec![],
    };
    for (name, t) in &r.fields {
        refs.field(t, Role::Field, name);
    }
    if let Some(alias) = &r.alias {
        refs.field(alias, Role::AliasTarget, "alias");
    }
    let qualified = r
        .module
        .iter()
        .chain(std::iter::once(&r.name))
        .cloned()
        .collect::<Vec<_>>()
        .join("::");
    if let Some(sig) = &r.signature {
        refs.signature(sig);
        // The syntax pass measured every function at the same site.
        let measure = sources[&r.file]
            .source()
            .functions()
            .iter()
            .find(|m| (m.line, m.column) == (r.line, r.column));
        facts.functions.push(Function {
            lines: measure.map_or(0, |m| m.lines),
            nesting: measure.map_or(0, |m| m.nesting),
            id: r.id.clone(),
            name: r.name.clone(),
            qualified,
            file: r.file.clone(),
            line: r.line,
            signature: r.text.clone(),
            public: r.public,
            trait_impl: r.trait_impl,
            test_only: r.test_only,
        });
    } else {
        facts.types.push(Type {
            id: r.id.clone(),
            name: r.name.clone(),
            qualified,
            file: r.file.clone(),
            line: r.line,
            alias: r.alias.as_ref().map(|t| text(source, t.span())),
            fields: r
                .fields
                .iter()
                .map(|(name, t)| (name.clone(), text(source, t.span())))
                .collect(),
            test_only: r.test_only,
        });
    }
    facts.references.extend(refs.out);
    facts.unknown_types.extend(refs.unknown);
}
/// Each function's calls, once per site: a `Type::default()` call is also a
/// default-call observation, and each call is resolved through the index.
fn observe_calls(
    records: &[Record],
    by_id: &BTreeMap<&String, &Record>,
    joined: &Joined<'_>,
    facts: &mut Facts,
) -> Result<Vec<Observed>> {
    let index = joined.index;
    let sites: BTreeMap<_, _> = records
        .iter()
        .filter(|r| r.signature.is_some())
        .map(|r| ((r.file.as_str(), r.line, r.column), r.id.clone()))
        .collect();
    let mut call_positions = BTreeMap::new();
    let mut observed = Vec::<Observed>::new();
    for (i, f) in index.functions().iter().enumerate() {
        let Some(from) = sites.get(&(
            f.flow.site.path.as_str(),
            f.flow.site.line,
            f.flow.site.column,
        )) else {
            continue;
        };
        for call in &f.flow.calls {
            if let Some(default) = default_call(from, by_id[from], call, joined) {
                facts.default_calls.push(default);
            }
            let key = (from.clone(), call.site.clone());
            let position = *call_positions.entry(key).or_insert_with(|| {
                observed.push((from.clone(), call.clone(), BTreeSet::new(), BTreeMap::new()));
                observed.len() - 1
            });
            let (_, _, targets, unknowns) = &mut observed[position];
            match index.resolve(i, call) {
                Target::Function(j) => {
                    let site = &index.functions()[j].flow.site;
                    if let Some(to) = sites.get(&(site.path.as_str(), site.line, site.column)) {
                        targets.insert(to.clone());
                    } else {
                        note(
                            unknowns,
                            Summary::new(
                                SummaryKind::BodyUnselected,
                                "resolved body lacks a selected representation site",
                            ),
                        );
                    }
                }
                Target::InvalidFunction(position) => {
                    return Err(format!("invalid indexed function position {position}").into());
                }
                Target::Summarized(reason) => note(unknowns, reason),
                Target::Boundary(..) => note(
                    unknowns,
                    Summary::new(SummaryKind::BoundaryUnresolved, "unresolved boundary"),
                ),
            }
        }
    }
    Ok(observed)
}
/// An explicit `Type::default()` call with no receiver or arguments, as a
/// construction of `Type`. Syntax is classified apart from callee
/// resolution: a derived Default body need not be present, and an inherent
/// `default` may also exist.
fn default_call(
    from: &str,
    r: &Record,
    call: &crate::workflow::Call,
    joined: &Joined<'_>,
) -> Option<Constructor> {
    if call.receiver.is_some() || !call.arguments.is_empty() {
        return None;
    }
    let mut path = syn::parse_str::<syn::Path>(&call.spelling).ok()?;
    if path.segments.len() < 2 || path.segments.last().is_none_or(|s| s.ident != "default") {
        return None;
    }
    path.segments.pop();
    path.segments.pop_punct();
    if path.is_ident("Self")
        && let Some(syn::Type::Path(owner)) = &r.owner
    {
        path = owner.path.clone();
    }
    let to = resolve(r, &path, joined.index, joined.addresses)?;
    Some(Constructor {
        from: from.to_owned(),
        to,
        line: call.site.line,
    })
}
/// A call with one body and no doubt is resolved; any other is unknown,
/// with its reasons (contexts disagreeing, or each distinct summary).
fn settle_calls(observed: Vec<Observed>, facts: &mut Facts) {
    for (from, call, mut targets, unknowns) in observed {
        if targets.len() == 1 && unknowns.is_empty() {
            if let Some(to) = targets.pop_first() {
                facts.calls.push(ResolvedCall {
                    from,
                    to,
                    site: call.site,
                    test_only: call.test_only,
                });
            }
            continue;
        }
        let reasons: Vec<Summary> = if targets.len() > 1 {
            vec![Summary::new(
                SummaryKind::ContextsDisagree,
                "Cargo/module contexts resolve to different physical bodies",
            )]
        } else {
            unknowns.into_values().collect()
        };
        let reason = reasons
            .iter()
            .map(|reason| reason.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        facts.unknown_calls.push(UnknownCall {
            from,
            call,
            reason,
            reasons,
        });
    }
}

/// Keep one reason per distinct reading, ordered by its message.
fn note(unknowns: &mut BTreeMap<String, Summary>, reason: Summary) {
    unknowns.entry(reason.message.clone()).or_insert(reason);
}
