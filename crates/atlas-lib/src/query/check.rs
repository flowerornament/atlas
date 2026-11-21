//! From parsed clauses to an evaluation plan's inputs: every relation and
//! field resolved against the schema or a rule head, every variable given a
//! kind, every rule shown safe (each variable bound by a positive relation)
//! and the program stratified, so negation and `count` only read relations
//! that are complete when they run. Only the rules a query needs are kept.
use super::{
    Error, ErrorKind, Location, Origin,
    honesty::{self, Check},
    syntax::{Atom, Builtin, ClauseKind, Const, Field, HeadTerm, Literal, Op, Program, Term},
};
use crate::relations::{Family, schema};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(crate) use schema::Kind;

pub(crate) type RelId = usize;

/// A relation as the program sees it: a stored one from the schema, a rule
/// head, or a query's answer.
#[derive(Debug)]
pub(crate) struct RelInfo {
    pub(crate) name: String,
    pub(crate) columns: Vec<(String, Kind)>,
    pub(crate) stored: Option<&'static schema::Relation>,
    pub(crate) origin: Option<Origin>,
}

#[derive(Clone, Debug)]
pub(crate) enum CTerm {
    Var(usize),
    Const(Const),
    Wild,
}

#[derive(Clone, Debug)]
pub(crate) enum CHead {
    Term(CTerm),
    Count(Vec<usize>),
}

#[derive(Clone, Debug)]
pub(crate) enum CLit {
    Pos(RelId, Vec<CTerm>),
    Neg(RelId, Vec<CTerm>),
    Cmp(Op, CTerm, CTerm),
    Builtin(Builtin, [CTerm; 2]),
}

/// One rule (or a query) with relations and fields resolved to positions.
#[derive(Debug)]
pub(crate) struct CRule {
    pub(crate) head: RelId,
    /// One per head column, in the relation's column order.
    pub(crate) terms: Vec<CHead>,
    pub(crate) body: Vec<CLit>,
    pub(crate) vars: Vec<String>,
    pub(crate) origin: Origin,
    pub(crate) at: Location,
    /// The clause as written, for findings that quote it.
    pub(crate) text: String,
}
impl CRule {
    pub(crate) fn aggregates(&self) -> bool {
        self.terms.iter().any(|t| matches!(t, CHead::Count(_)))
    }
}

#[derive(Debug)]
pub(crate) struct CQuery {
    pub(crate) rel: RelId,
    pub(crate) text: String,
    /// Column positions, with `true` for descending.
    pub(crate) order: Vec<(usize, bool)>,
    pub(crate) limit: Option<usize>,
}

/// A program resolved against the schema and ready to run: the rules its
/// queries need, in strata, and the families of facts they read.
#[derive(Debug)]
pub struct Checked {
    pub(crate) rels: Vec<RelInfo>,
    pub(crate) rules: Vec<CRule>,
    /// Positions in `rules`, one list per stratum, lowest first.
    pub(crate) strata: Vec<Vec<usize>>,
    pub(crate) queries: Vec<CQuery>,
    pub(crate) families: Vec<Family>,
    /// Stored relations the needed rules read.
    pub(crate) stored: BTreeSet<RelId>,
    /// Honesty findings the caller accepted, as they would have refused.
    pub(crate) accepted: Vec<String>,
    /// What the answer cannot show, from the relations the program reads.
    pub(crate) notes: Vec<String>,
}
impl Checked {
    /// Honesty findings accepted with [`super::Options::accept`].
    pub fn accepted(&self) -> &[String] {
        &self.accepted
    }
    /// The fact families the program reads; a caller need capture only these.
    pub fn families(&self) -> &[Family] {
        &self.families
    }
    /// The stored relations the program reads, by name.
    pub fn reads(&self) -> impl Iterator<Item = &str> {
        self.stored.iter().map(|&r| self.rels[r].name.as_str())
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Integer => "a number",
        Kind::Boolean => "a boolean",
        _ => "text",
    }
}
fn const_kind(c: &Const) -> Kind {
    match c {
        Const::Text(_) => Kind::Text,
        Const::Integer(_) => Kind::Integer,
        Const::Boolean(_) => Kind::Boolean,
    }
}
fn list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let items: Vec<String> = items.map(|i| format!("`{i}`")).collect();
    if items.is_empty() {
        "none".into()
    } else {
        items.join(", ")
    }
}

/// Edit distance, for "closest" suggestions.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = if ca == *cb {
                previous
            } else {
                1 + previous.min(row[j]).min(row[j + 1])
            };
            previous = old;
        }
    }
    row[b.len()]
}

struct Catalog<'p> {
    rels: Vec<RelInfo>,
    by_name: HashMap<String, RelId>,
    /// Where each derived relation's first head was written.
    defined_at: HashMap<RelId, (&'p Program, Location)>,
    /// Column kinds of derived relations while they are being inferred.
    kinds: HashMap<RelId, Vec<Option<Kind>>>,
}

/// A clause and the program that holds it, for error messages.
#[derive(Clone, Copy)]
struct Source<'p> {
    program: &'p Program,
    index: usize,
}

pub(crate) fn check(
    user: &Program,
    prelude: &Program,
    families: &[Family],
    accept: &BTreeSet<Check>,
) -> Result<Checked, Error> {
    let mut catalog = Catalog::of_schema();
    let sources: Vec<Source<'_>> = [prelude, user]
        .into_iter()
        .flat_map(|program| (0..program.clauses.len()).map(move |index| Source { program, index }))
        .collect();
    // Heads first, so a rule may read a relation defined further down.
    for source in &sources {
        let clause = &source.program.clauses[source.index];
        if let ClauseKind::Rule { relation, head, .. } = &clause.kind {
            catalog.declare(source.program, clause.at, relation, head)?;
        }
    }
    catalog.kinds = infer_kinds(&catalog, &sources)?;
    catalog.settle_kinds()?;
    let (rules, queries) = compile_clauses(&mut catalog, &sources, families)?;
    if queries.is_empty() {
        return Err(Error::plain(
            ErrorKind::Syntax,
            "the program has no query; add one such as `?- call{from: F, to: T}.`".into(),
        ));
    }
    let needed = demanded(&catalog.rels, &rules, &queries);
    let rules: Vec<CRule> = rules
        .into_iter()
        .enumerate()
        .filter(|(i, _)| needed.contains(i))
        .map(|(_, r)| r)
        .collect();
    prelude_reads_available(&catalog.rels, &rules, families)?;
    let strata = stratify(&catalog.rels, user, prelude, &rules)?;
    let stored: BTreeSet<RelId> = rules
        .iter()
        .flat_map(|r| r.body.iter())
        .filter_map(|l| match l {
            CLit::Pos(rel, _) | CLit::Neg(rel, _) if catalog.rels[*rel].stored.is_some() => {
                Some(*rel)
            }
            _ => None,
        })
        .collect();
    let families: Vec<Family> = Family::ALL
        .into_iter()
        .filter(|f| {
            stored
                .iter()
                .any(|&r| catalog.rels[r].stored.is_some_and(|s| s.family == *f))
        })
        .collect();
    let accepted = honesty_accepted(&catalog.rels, &rules, user, prelude, accept)?;
    let notes = honesty::unread_unknowns(&catalog.rels, &stored);
    Ok(Checked {
        rels: catalog.rels,
        rules,
        strata,
        queries,
        families,
        stored,
        accepted,
        notes,
    })
}

/// Compile every clause in order: each rule, and each query as a rule for
/// its answer. The caller's atoms are refused where they are written when
/// their family is missing; a prelude rule only if a query needs it.
fn compile_clauses(
    catalog: &mut Catalog<'_>,
    sources: &[Source<'_>],
    families: &[Family],
) -> Result<(Vec<CRule>, Vec<CQuery>), Error> {
    let mut rules = Vec::new();
    let mut queries = Vec::new();
    let available = |program: &Program| -> &[Family] {
        if program.origin == Origin::Prelude {
            &Family::ALL
        } else {
            families
        }
    };
    for source in sources {
        let clause = &source.program.clauses[source.index];
        let compiled = Clause {
            program: source.program,
            at: clause.at,
            text: &clause.text,
        };
        match &clause.kind {
            ClauseKind::Rule {
                relation,
                head,
                body,
            } => {
                let head_rel = catalog.by_name[relation.as_str()];
                rules.push(compile(
                    catalog,
                    compiled,
                    head_rel,
                    Some(head),
                    body,
                    available(source.program),
                )?);
            }
            ClauseKind::Query { body, order, limit } => {
                let (rule, query) = compile_query(
                    catalog,
                    compiled,
                    body,
                    order,
                    *limit,
                    available(source.program),
                    queries.len(),
                )?;
                rules.push(rule);
                queries.push(query);
            }
        }
    }
    Ok((rules, queries))
}

/// Refuse a needed prelude rule that reads a stored relation whose family
/// the facts do not include.
fn prelude_reads_available(
    rels: &[RelInfo],
    rules: &[CRule],
    families: &[Family],
) -> Result<(), Error> {
    for rule in rules.iter().filter(|r| r.origin == Origin::Prelude) {
        for literal in &rule.body {
            let (CLit::Pos(rel, _) | CLit::Neg(rel, _)) = literal else {
                continue;
            };
            if let Some(stored) = rels[*rel].stored
                && !families.contains(&stored.family)
            {
                return Err(Error::plain(
                    ErrorKind::Schema,
                    format!(
                        "the prelude's `{}` reads `{}`, in the {} family, which these facts do not include",
                        rels[rule.head].name,
                        stored.name,
                        stored.family.name()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// The honesty findings `accept` admits, as reported with the answer; any
/// other finding refuses the program, all of them listed in one error.
fn honesty_accepted(
    rels: &[RelInfo],
    rules: &[CRule],
    user: &Program,
    prelude: &Program,
    accept: &BTreeSet<Check>,
) -> Result<Vec<String>, Error> {
    let mut accepted = Vec::new();
    let mut refused = Vec::new();
    for (check, finding) in honesty::review(rels, rules, user, prelude) {
        if accept.contains(&check) {
            accepted.push(format!("accepted {}: {finding}", check.name()));
        } else {
            refused.push((check, finding));
        }
    }
    let Some((_, first)) = refused.first() else {
        return Ok(accepted);
    };
    let checks: BTreeSet<&str> = refused.iter().map(|(c, _)| c.name()).collect();
    let mut error = first.clone();
    error.message = refused
        .iter()
        .map(|(_, f)| f.to_string())
        .chain([format!(
            "{} honesty finding(s); fix the rules, or accept with {} to run anyway (the acceptance is reported with the answer)",
            refused.len(),
            checks
                .iter()
                .map(|c| format!("--accept {c}"))
                .collect::<Vec<_>>()
                .join(" ")
        )])
        .collect::<Vec<_>>()
        .join("\n");
    error.location = None;
    error.line = None;
    Err(error)
}

impl<'p> Catalog<'p> {
    /// The stored relations of the schema, before any rule is declared.
    fn of_schema() -> Self {
        let mut catalog = Self {
            rels: Vec::new(),
            by_name: HashMap::new(),
            defined_at: HashMap::new(),
            kinds: HashMap::new(),
        };
        for relation in schema::RELATIONS {
            catalog
                .by_name
                .insert(relation.name.to_owned(), catalog.rels.len());
            catalog.rels.push(RelInfo {
                name: relation.name.to_owned(),
                columns: relation
                    .columns
                    .iter()
                    .map(|c| (c.name.to_owned(), c.kind))
                    .collect(),
                stored: Some(relation),
                origin: None,
            });
        }
        catalog
    }
    /// Write the inferred kinds into the derived relations' columns; a
    /// column no rule gives a kind refuses at the relation's first head.
    fn settle_kinds(&mut self) -> Result<(), Error> {
        for (&rel, kinds) in &self.kinds {
            let info = &mut self.rels[rel];
            for ((name, kind), inferred) in info.columns.iter_mut().zip(kinds) {
                let Some(inferred) = inferred else {
                    let (program, at) = self.defined_at[&rel];
                    return Err(program.error(
                        ErrorKind::Schema,
                        at,
                        format!(
                            "cannot tell whether field `{name}` of `{}` holds text, a number or a boolean: no rule binds it from a stored relation or a value",
                            info.name
                        ),
                    ));
                };
                *kind = *inferred;
            }
        }
        Ok(())
    }
    fn declare(
        &mut self,
        program: &'p Program,
        at: Location,
        relation: &str,
        head: &[Field<HeadTerm>],
    ) -> Result<(), Error> {
        let names = head_fields(program, head)?;
        if let Some(&rel) = self.by_name.get(relation) {
            return self.redeclare(rel, program, at, relation, &names);
        }
        let rel = self.rels.len();
        self.by_name.insert(relation.to_owned(), rel);
        self.rels.push(RelInfo {
            name: relation.to_owned(),
            columns: names
                .iter()
                .map(|n| ((*n).to_owned(), Kind::Text))
                .collect(),
            stored: None,
            origin: Some(program.origin),
        });
        self.defined_at.insert(rel, (program, at));
        self.kinds.insert(rel, vec![None; names.len()]);
        Ok(())
    }
    /// Another head for a known relation: refused for a stored relation, one
    /// another program defines, or one whose rules give different fields.
    fn redeclare(
        &self,
        rel: RelId,
        program: &Program,
        at: Location,
        relation: &str,
        names: &[&str],
    ) -> Result<(), Error> {
        let info = &self.rels[rel];
        if info.stored.is_some() {
            return Err(program.error(
                ErrorKind::Schema,
                at,
                format!(
                    "`{relation}` is a stored relation of the facts; a rule cannot add rows to it. Name the derived relation differently"
                ),
            ));
        }
        if info.origin != Some(program.origin) {
            return Err(program.error(
                ErrorKind::Schema,
                at,
                format!(
                    "`{relation}` is defined by the prelude; a program reads it but cannot add to it. Name your relation differently"
                ),
            ));
        }
        let expected: BTreeSet<&str> = info.columns.iter().map(|(c, _)| c.as_str()).collect();
        let given: BTreeSet<&str> = names.iter().copied().collect();
        if expected != given {
            let (_, first) = self.defined_at[&rel];
            return Err(program.error(
                ErrorKind::Schema,
                at,
                format!(
                    "every rule for `{relation}` gives the same fields; the rule at line {} gives {}, this one {}",
                    first.line,
                    list(expected.into_iter()),
                    list(given.into_iter())
                ),
            ));
        }
        Ok(())
    }
    fn column_kind(&self, rel: RelId, position: usize) -> Option<Kind> {
        match self.kinds.get(&rel) {
            Some(kinds) => kinds[position],
            None => Some(self.rels[rel].columns[position].1),
        }
    }
    /// The relation `name`, or an error that says what exists.
    fn resolve(
        &self,
        program: &Program,
        name: &str,
        at: Location,
        families: &[Family],
    ) -> Result<RelId, Error> {
        let Some(&rel) = self.by_name.get(name) else {
            let mut known: Vec<&str> = self
                .rels
                .iter()
                .filter(|r| !r.name.starts_with('?'))
                .map(|r| r.name.as_str())
                .collect();
            known.sort_by_key(|k| (distance(k, name), *k));
            known.truncate(3);
            return Err(program.error(
                ErrorKind::Schema,
                at,
                format!(
                    "no relation `{name}`; closest: {}. `cargo atlas facts --schema` lists the stored relations and `cargo atlas query --prelude` the prelude's",
                    list(known.into_iter())
                ),
            ));
        };
        if let Some(stored) = self.rels[rel].stored
            && !families.contains(&stored.family)
        {
            return Err(program.error(
                ErrorKind::Schema,
                at,
                format!(
                    "`{name}` is in the {} family, which these facts do not include",
                    stored.family.name()
                ),
            ));
        }
        Ok(rel)
    }
}

/// A rule head's field names in written order: each given once, and each
/// with a value or a variable.
fn head_fields<'h>(program: &Program, head: &'h [Field<HeadTerm>]) -> Result<Vec<&'h str>, Error> {
    let mut names: Vec<&str> = Vec::new();
    for field in head {
        if names.contains(&field.name.as_str()) {
            return Err(program.error(
                ErrorKind::Schema,
                field.at,
                format!("field `{}` is given twice", field.name),
            ));
        }
        if let HeadTerm::Term(Term::Wild(at)) = &field.term {
            return Err(program.error(
                ErrorKind::Safety,
                *at,
                format!(
                    "field `{}` of a rule head needs a value or a variable, not `_`",
                    field.name
                ),
            ));
        }
        names.push(&field.name);
    }
    Ok(names)
}

/// Derived column kinds, to a fixed point: a head field takes the kind of
/// the value or variable written there, and a variable the kind of the
/// stored or already inferred column it is read from.
fn infer_kinds(
    catalog: &Catalog<'_>,
    sources: &[Source<'_>],
) -> Result<HashMap<RelId, Vec<Option<Kind>>>, Error> {
    let mut kinds = catalog.kinds.clone();
    loop {
        let mut changed = false;
        for source in sources {
            let clause = &source.program.clauses[source.index];
            let ClauseKind::Rule {
                relation,
                head,
                body,
            } = &clause.kind
            else {
                continue;
            };
            let vars = body_kinds(catalog, &kinds, body);
            let rel = catalog.by_name[relation.as_str()];
            changed |= head_kinds(catalog, &mut kinds, source.program, rel, head, &vars)?;
        }
        if !changed {
            return Ok(kinds);
        }
    }
}
/// The position of field `name` among `rel`'s columns.
fn column_position(catalog: &Catalog<'_>, rel: RelId, name: &str) -> Option<usize> {
    catalog.rels[rel]
        .columns
        .iter()
        .position(|(c, _)| *c == name)
}
/// Each body variable's kind, from the first place that gives one: a stored
/// or inferred column, a builtin (text), a compared value, then a variable
/// compared with one that has a kind.
fn body_kinds<'b>(
    catalog: &Catalog<'_>,
    kinds: &HashMap<RelId, Vec<Option<Kind>>>,
    body: &'b [Literal],
) -> HashMap<&'b str, Kind> {
    let lookup = |rel: RelId, position: usize| {
        kinds.get(&rel).map_or_else(
            || Some(catalog.rels[rel].columns[position].1),
            |k| k[position],
        )
    };
    let mut vars: HashMap<&str, Kind> = HashMap::new();
    for literal in body {
        match literal {
            Literal::Atom { atom, .. } => {
                let Some(&rel) = catalog.by_name.get(&atom.relation) else {
                    continue;
                };
                for field in &atom.fields {
                    if let (Term::Var(v, _), Some(p)) =
                        (&field.term, column_position(catalog, rel, &field.name))
                        && let Some(kind) = lookup(rel, p)
                    {
                        vars.entry(v).or_insert(kind);
                    }
                }
            }
            Literal::Builtin { args, .. } => {
                for arg in args {
                    if let Term::Var(v, _) = arg {
                        vars.entry(v).or_insert(Kind::Text);
                    }
                }
            }
            Literal::Compare { left, right, .. } => {
                for (a, b) in [(left, right), (right, left)] {
                    if let (Term::Var(v, _), Term::Const(c, _)) = (a, b) {
                        vars.entry(v).or_insert(const_kind(c));
                    }
                }
            }
        }
    }
    for literal in body {
        if let Literal::Compare {
            left: Term::Var(a, _),
            right: Term::Var(b, _),
            ..
        } = literal
        {
            if let Some(&k) = vars.get(a.as_str()) {
                vars.entry(b).or_insert(k);
            } else if let Some(&k) = vars.get(b.as_str()) {
                vars.entry(a).or_insert(k);
            }
        }
    }
    vars
}
/// Record the kinds one head gives `rel`'s columns; returns whether any
/// column gained a kind. A kind that differs from another rule's refuses.
fn head_kinds(
    catalog: &Catalog<'_>,
    kinds: &mut HashMap<RelId, Vec<Option<Kind>>>,
    program: &Program,
    rel: RelId,
    head: &[Field<HeadTerm>],
    vars: &HashMap<&str, Kind>,
) -> Result<bool, Error> {
    let mut changed = false;
    for field in head {
        let Some(p) = column_position(catalog, rel, &field.name) else {
            continue;
        };
        let kind = match &field.term {
            HeadTerm::Count(_) => Some(Kind::Integer),
            HeadTerm::Term(Term::Const(c, _)) => Some(const_kind(c)),
            HeadTerm::Term(Term::Var(v, _)) => vars.get(v.as_str()).copied(),
            HeadTerm::Term(Term::Wild(_)) => None,
        };
        let (Some(kind), Some(slot)) = (kind, kinds.get_mut(&rel).map(|k| &mut k[p])) else {
            continue;
        };
        match slot {
            None => {
                *slot = Some(kind);
                changed = true;
            }
            Some(existing) if *existing != kind => {
                return Err(program.error(
                    ErrorKind::Schema,
                    field.at,
                    format!(
                        "field `{}` of `{}` is {} here but {} in another rule",
                        field.name,
                        catalog.rels[rel].name,
                        kind_name(kind),
                        kind_name(*existing)
                    ),
                ));
            }
            Some(_) => {}
        }
    }
    Ok(changed)
}

/// Variables of one clause, numbered by first appearance.
#[derive(Default)]
struct Vars {
    names: Vec<String>,
    uses: Vec<usize>,
    first_at: Vec<Location>,
}
impl Vars {
    fn get(&mut self, name: &str, at: Location) -> usize {
        if let Some(i) = self.names.iter().position(|v| v == name) {
            self.uses[i] += 1;
            i
        } else {
            self.names.push(name.to_owned());
            self.uses.push(1);
            self.first_at.push(at);
            self.names.len() - 1
        }
    }
    fn term(&mut self, term: &Term) -> CTerm {
        match term {
            Term::Var(name, at) => CTerm::Var(self.get(name, *at)),
            Term::Wild(_) => CTerm::Wild,
            Term::Const(c, _) => CTerm::Const(c.clone()),
        }
    }
}

/// A variable's kind as one place in the clause gives it.
struct Typed {
    var: usize,
    kind: Kind,
    at: Location,
    from: String,
}

/// A clause being compiled, with its program for error messages.
#[derive(Clone, Copy)]
struct Clause<'a> {
    program: &'a Program,
    at: Location,
    text: &'a str,
}

/// The positions and kinds of one body atom's fields.
fn compile_atom(
    catalog: &Catalog<'_>,
    program: &Program,
    atom: &Atom,
    families: &[Family],
    vars: &mut Vars,
    typed: &mut Vec<Typed>,
) -> Result<(RelId, Vec<CTerm>), Error> {
    let rel = catalog.resolve(program, &atom.relation, atom.at, families)?;
    let info = &catalog.rels[rel];
    let mut terms = vec![CTerm::Wild; info.columns.len()];
    let mut given: Vec<&str> = Vec::new();
    for field in &atom.fields {
        let Some(position) = info.columns.iter().position(|(c, _)| *c == field.name) else {
            let mut columns: Vec<&str> = info.columns.iter().map(|(c, _)| c.as_str()).collect();
            columns.sort_by_key(|c| distance(c, &field.name));
            return Err(program.error(
                ErrorKind::Schema,
                field.at,
                format!(
                    "`{}` has no field `{}`; its fields, closest first: {}",
                    info.name,
                    field.name,
                    list(columns.into_iter())
                ),
            ));
        };
        if given.contains(&field.name.as_str()) {
            return Err(program.error(
                ErrorKind::Schema,
                field.at,
                format!("field `{}` is given twice", field.name),
            ));
        }
        given.push(&field.name);
        let term = vars.term(&field.term);
        match (&term, catalog.column_kind(rel, position)) {
            (CTerm::Var(v), Some(kind)) => typed.push(Typed {
                var: *v,
                kind,
                at: field.at,
                from: format!("`{}.{}`", info.name, field.name),
            }),
            (CTerm::Const(c), Some(kind)) if const_kind(c) != kind => {
                return Err(program.error(
                    ErrorKind::Schema,
                    field.term.at(),
                    format!(
                        "`{}.{}` holds {}, given {}",
                        info.name,
                        field.name,
                        kind_name(kind),
                        kind_name(const_kind(c))
                    ),
                ));
            }
            _ => {}
        }
        terms[position] = term;
    }
    Ok((rel, terms))
}

/// The body literals of a rule or query, resolved, with each variable's
/// kind as the body gives it.
fn compile_body(
    catalog: &Catalog<'_>,
    program: &Program,
    body: &[Literal],
    families: &[Family],
    vars: &mut Vars,
    typed: &mut Vec<Typed>,
) -> Result<Vec<CLit>, Error> {
    let mut compiled = Vec::new();
    for literal in body {
        match literal {
            Literal::Atom { negated, atom } => {
                let (rel, terms) = compile_atom(catalog, program, atom, families, vars, typed)?;
                compiled.push(if *negated {
                    CLit::Neg(rel, terms)
                } else {
                    CLit::Pos(rel, terms)
                });
            }
            Literal::Compare {
                op,
                left,
                right,
                at,
            } => {
                let (l, r) = (vars.term(left), vars.term(right));
                if matches!(l, CTerm::Wild) || matches!(r, CTerm::Wild) {
                    return Err(program.error(
                        ErrorKind::Safety,
                        *at,
                        "`_` cannot be compared; name a variable".into(),
                    ));
                }
                for (side, other) in [(&l, &r), (&r, &l)] {
                    if let (CTerm::Var(v), CTerm::Const(c)) = (side, other) {
                        typed.push(Typed {
                            var: *v,
                            kind: const_kind(c),
                            at: *at,
                            from: "the value it is compared with".into(),
                        });
                    }
                }
                compiled.push(CLit::Cmp(*op, l, r));
            }
            Literal::Builtin { builtin, args, at } => {
                let pair = [vars.term(&args[0]), vars.term(&args[1])];
                for t in &pair {
                    match t {
                        CTerm::Var(v) => typed.push(Typed {
                            var: *v,
                            kind: Kind::Text,
                            at: *at,
                            from: format!("`{}`", builtin.name()),
                        }),
                        CTerm::Const(c) if const_kind(c) != Kind::Text => {
                            return Err(program.error(
                                ErrorKind::Schema,
                                *at,
                                format!("`{}` takes text", builtin.name()),
                            ));
                        }
                        CTerm::Wild => {
                            return Err(program.error(
                                ErrorKind::Safety,
                                *at,
                                format!("`{}` needs values, not `_`", builtin.name()),
                            ));
                        }
                        CTerm::Const(_) => {}
                    }
                }
                compiled.push(CLit::Builtin(*builtin, pair));
            }
        }
    }
    Ok(compiled)
}

fn compile(
    catalog: &Catalog<'_>,
    clause: Clause<'_>,
    head_rel: RelId,
    head: Option<&Vec<Field<HeadTerm>>>,
    body: &[Literal],
    families: &[Family],
) -> Result<CRule, Error> {
    let mut vars = Vars::default();
    let mut typed = Vec::new();
    let compiled = compile_body(
        catalog,
        clause.program,
        body,
        families,
        &mut vars,
        &mut typed,
    )?;
    let mut terms = Vec::new();
    if let Some(head) = head {
        let info = &catalog.rels[head_rel];
        let mut by_position: Vec<Option<CHead>> = (0..info.columns.len()).map(|_| None).collect();
        for field in head {
            let Some(position) = info.columns.iter().position(|(c, _)| *c == field.name) else {
                continue;
            };
            by_position[position] = Some(match &field.term {
                HeadTerm::Term(t) => CHead::Term(vars.term(t)),
                HeadTerm::Count(vs) => {
                    CHead::Count(vs.iter().map(|(v, at)| vars.get(v, *at)).collect())
                }
            });
        }
        terms = by_position.into_iter().flatten().collect();
    }
    var_kinds(clause, &vars, typed, &compiled)?;
    let rule = CRule {
        head: head_rel,
        terms,
        body: compiled,
        vars: vars.names.clone(),
        origin: clause.program.origin,
        at: clause.at,
        text: clause.text.to_owned(),
    };
    safety(clause.program, &rule, &vars, head.is_none())?;
    Ok(rule)
}

/// A query compiles to a rule whose head lists its named variables, in the
/// order they first appear; a variable that starts with `_` is not shown.
fn compile_query(
    catalog: &mut Catalog<'_>,
    clause: Clause<'_>,
    body: &[Literal],
    order: &[super::syntax::Order],
    limit: Option<usize>,
    families: &[Family],
    number: usize,
) -> Result<(CRule, CQuery), Error> {
    let mut vars = Vars::default();
    let mut typed = Vec::new();
    let compiled = compile_body(
        catalog,
        clause.program,
        body,
        families,
        &mut vars,
        &mut typed,
    )?;
    let kinds = var_kinds(clause, &vars, typed, &compiled)?;
    let shown: Vec<usize> = (0..vars.names.len())
        .filter(|&v| !vars.names[v].starts_with('_'))
        .collect();
    let columns: Vec<(String, Kind)> = shown
        .iter()
        .map(|&v| (vars.names[v].clone(), kinds[v].unwrap_or(Kind::Text)))
        .collect();
    let mut positions = Vec::new();
    for o in order {
        let Some(position) = columns.iter().position(|(c, _)| *c == o.var) else {
            return Err(clause.program.error(
                ErrorKind::Schema,
                o.at,
                format!(
                    "`order by {}` names no column of this query; its columns: {}",
                    o.var,
                    list(columns.iter().map(|(c, _)| c.as_str()))
                ),
            ));
        };
        positions.push((position, o.descending));
    }
    let rel = catalog.rels.len();
    catalog.rels.push(RelInfo {
        name: format!("?{}", number + 1),
        columns,
        stored: None,
        origin: Some(clause.program.origin),
    });
    let rule = CRule {
        head: rel,
        terms: shown.iter().map(|&v| CHead::Term(CTerm::Var(v))).collect(),
        body: compiled,
        vars: vars.names.clone(),
        origin: clause.program.origin,
        at: clause.at,
        text: clause.text.to_owned(),
    };
    safety(clause.program, &rule, &vars, true)?;
    let query = CQuery {
        rel,
        text: clause.text.to_owned(),
        order: positions,
        limit,
    };
    Ok((rule, query))
}

/// One kind per variable: a variable read from columns of different kinds,
/// or compared across kinds, is refused.
fn var_kinds(
    clause: Clause<'_>,
    vars: &Vars,
    typed: Vec<Typed>,
    body: &[CLit],
) -> Result<Vec<Option<Kind>>, Error> {
    let mut seen: Vec<Option<(Kind, String)>> = vec![None; vars.names.len()];
    for t in typed {
        match &seen[t.var] {
            None => seen[t.var] = Some((t.kind, t.from)),
            Some((k, first)) if *k != t.kind => {
                return Err(clause.program.error(
                    ErrorKind::Schema,
                    t.at,
                    format!(
                        "variable `{}` is {} in {first} but {} in {}",
                        vars.names[t.var],
                        kind_name(*k),
                        kind_name(t.kind),
                        t.from
                    ),
                ));
            }
            Some(_) => {}
        }
    }
    let kinds: Vec<Option<Kind>> = seen.into_iter().map(|s| s.map(|(k, _)| k)).collect();
    for literal in body {
        let CLit::Cmp(op, l, r) = literal else {
            continue;
        };
        let kind = |t: &CTerm| match t {
            CTerm::Var(v) => kinds[*v],
            CTerm::Const(c) => Some(const_kind(c)),
            CTerm::Wild => None,
        };
        if let (Some(a), Some(b)) = (kind(l), kind(r))
            && a != b
        {
            return Err(clause.program.error(
                ErrorKind::Schema,
                clause.at,
                format!(
                    "a comparison sets {} against {}",
                    kind_name(a),
                    kind_name(b)
                ),
            ));
        }
        if matches!(op, Op::Lt | Op::Le | Op::Gt | Op::Ge)
            && (kind(l) == Some(Kind::Boolean) || kind(r) == Some(Kind::Boolean))
        {
            return Err(clause.program.error(
                ErrorKind::Schema,
                clause.at,
                "booleans are compared with `=` or `!=` only".into(),
            ));
        }
    }
    Ok(kinds)
}

fn vars_of(terms: &[CTerm]) -> impl Iterator<Item = usize> + '_ {
    terms.iter().filter_map(|t| {
        if let CTerm::Var(v) = t {
            Some(*v)
        } else {
            None
        }
    })
}

/// Every variable is bound by a positive relation (directly, or through `=`
/// or `last_segment` from one that is) before a negation, comparison, test
/// or the head reads it; and a rule variable used once is a typo or `_`.
fn safety(program: &Program, rule: &CRule, vars: &Vars, query: bool) -> Result<(), Error> {
    let bound = binding_order(rule);
    let check = |v: usize, role: &str| -> Result<(), Error> {
        if bound[v] {
            Ok(())
        } else {
            Err(program.error(
                ErrorKind::Safety,
                vars.first_at[v],
                format!(
                    "variable `{}` is not bound: {role} reads it, but no positive relation in this clause gives it a value",
                    rule.vars[v]
                ),
            ))
        }
    };
    for literal in &rule.body {
        let (found, role): (Vec<usize>, &str) = match literal {
            CLit::Neg(_, terms) => (vars_of(terms).collect(), "a negation"),
            CLit::Cmp(_, a, b) => (vars_of(&[a.clone(), b.clone()]).collect(), "a comparison"),
            CLit::Builtin(builtin, args) => {
                let inputs = if *builtin == Builtin::LastSegment {
                    &args[..1]
                } else {
                    &args[..]
                };
                (vars_of(inputs).collect(), "a test")
            }
            CLit::Pos(..) => continue,
        };
        for v in found {
            check(v, role)?;
        }
    }
    for term in &rule.terms {
        match term {
            CHead::Term(CTerm::Var(v)) => check(*v, "the head")?,
            CHead::Count(vs) => {
                for v in vs {
                    check(*v, "`count`")?;
                }
            }
            CHead::Term(_) => {}
        }
    }
    if !query {
        for (v, name) in rule.vars.iter().enumerate() {
            if vars.uses[v] == 1 && !name.starts_with('_') {
                return Err(program.error(
                    ErrorKind::Safety,
                    vars.first_at[v],
                    format!(
                        "variable `{name}` appears only once in this rule; write `_` if any value will do, or check its spelling"
                    ),
                ));
            }
        }
    }
    if rule.body.is_empty() && !query {
        for term in &rule.terms {
            if !matches!(term, CHead::Term(CTerm::Const(_))) {
                return Err(program.error(
                    ErrorKind::Safety,
                    rule.at,
                    "a fact (a rule with no body) gives values only".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Which variables the body can bind, as the evaluator binds them.
pub(crate) fn binding_order(rule: &CRule) -> Vec<bool> {
    let mut bound = vec![false; rule.vars.len()];
    let is_bound = |t: &CTerm, bound: &[bool]| match t {
        CTerm::Var(v) => bound[*v],
        _ => true,
    };
    loop {
        let mut changed = false;
        let mut bind = |t: &CTerm, bound: &mut Vec<bool>| {
            if let CTerm::Var(v) = t
                && !bound[*v]
            {
                bound[*v] = true;
                changed = true;
            }
        };
        for literal in &rule.body {
            match literal {
                CLit::Pos(_, terms) => {
                    for t in terms {
                        bind(t, &mut bound);
                    }
                }
                CLit::Cmp(Op::Eq, a, b) => {
                    if is_bound(a, &bound) {
                        bind(b, &mut bound);
                    } else if is_bound(b, &bound) {
                        bind(a, &mut bound);
                    }
                }
                CLit::Builtin(Builtin::LastSegment, [a, b]) if is_bound(a, &bound) => {
                    bind(b, &mut bound);
                }
                _ => {}
            }
        }
        if !changed {
            return bound;
        }
    }
}

/// The rules the queries need, by position.
fn demanded(rels: &[RelInfo], rules: &[CRule], queries: &[CQuery]) -> BTreeSet<usize> {
    let mut by_head: BTreeMap<RelId, Vec<usize>> = BTreeMap::new();
    for (i, r) in rules.iter().enumerate() {
        by_head.entry(r.head).or_default().push(i);
    }
    let mut needed = BTreeSet::new();
    let mut todo: Vec<RelId> = queries.iter().map(|q| q.rel).collect();
    let mut seen = BTreeSet::new();
    while let Some(rel) = todo.pop() {
        if !seen.insert(rel) || rels[rel].stored.is_some() {
            continue;
        }
        for &i in by_head.get(&rel).into_iter().flatten() {
            needed.insert(i);
            for literal in &rules[i].body {
                if let CLit::Pos(r, _) | CLit::Neg(r, _) = literal {
                    todo.push(*r);
                }
            }
        }
    }
    needed
}

/// Strata: a relation sits above every relation it negates or counts over,
/// and no lower than those it reads. A cycle through negation or `count`
/// has no single answer and is refused.
fn stratify(
    rels: &[RelInfo],
    user: &Program,
    prelude: &Program,
    rules: &[CRule],
) -> Result<Vec<Vec<usize>>, Error> {
    let mut level = vec![0usize; rels.len()];
    let ceiling = rels.len();
    loop {
        let mut changed = false;
        for rule in rules {
            let strict_all = rule.aggregates();
            for literal in &rule.body {
                let (rel, strict) = match literal {
                    CLit::Pos(rel, _) => (*rel, strict_all),
                    CLit::Neg(rel, _) => (*rel, true),
                    _ => continue,
                };
                let need = level[rel] + usize::from(strict);
                if level[rule.head] < need {
                    if need > ceiling {
                        let program = if rule.origin == Origin::Prelude {
                            prelude
                        } else {
                            user
                        };
                        return Err(program.error(
                            ErrorKind::Stratification,
                            rule.at,
                            format!(
                                "`{}` depends on itself through a negation or `count`; such a program has no single answer",
                                rels[rule.head].name
                            ),
                        ));
                    }
                    level[rule.head] = need;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Queries run once, after every relation they read is complete; no rule
    // reads a query, so they share a last stratum.
    let query = |rule: &CRule| rels[rule.head].name.starts_with('?');
    let top = rules
        .iter()
        .filter(|r| !query(r))
        .map(|r| level[r.head])
        .max()
        .unwrap_or(0);
    let mut strata = vec![Vec::new(); top + 2];
    for (i, rule) in rules.iter().enumerate() {
        strata[if query(rule) {
            top + 1
        } else {
            level[rule.head]
        }]
        .push(i);
    }
    Ok(strata)
}
