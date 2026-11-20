//! Bottom-up evaluation of a checked program over captured relations:
//! stratum by stratum, each to a fixed point with semi-naive iteration (a
//! recursive rule re-runs only against the rows the last round added),
//! joining through hash indexes built on demand. Values are interned, rows
//! are sets, and `count` counts distinct bindings, so two identical
//! observations would merge; the facts carry a witness id on every row that
//! could otherwise repeat.
use super::{
    Answer, Error, ErrorKind, Limits, Table,
    check::{CHead, CLit, CQuery, CRule, CTerm, Checked, Kind, RelId},
    syntax::{Builtin, Const, Op},
};
use crate::relations::{Cell, Relations};
use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::{HashMap, HashSet},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum V {
    B(bool),
    I(u64),
    S(u32),
}

#[derive(Default)]
struct Interner {
    ids: HashMap<String, u32>,
    all: Vec<String>,
}
impl Interner {
    fn id(&mut self, text: &str) -> u32 {
        if let Some(&id) = self.ids.get(text) {
            return id;
        }
        let id = u32::try_from(self.all.len()).unwrap_or(u32::MAX);
        self.all.push(text.to_owned());
        self.ids.insert(text.to_owned(), id);
        id
    }
}

type Row = Box<[V]>;

#[derive(Default)]
struct Store {
    rows: Vec<Row>,
    set: HashSet<Row>,
}
impl Store {
    fn insert(&mut self, row: Row) -> bool {
        if self.set.contains(&row) {
            return false;
        }
        self.set.insert(row.clone());
        self.rows.push(row);
        true
    }
}

/// Rows of one relation keyed by the values of some of its columns.
#[derive(Default)]
struct Index {
    upto: usize,
    map: HashMap<Row, Vec<usize>>,
}

/// Which columns are bound when a literal runs, one bit per column.
type Mask = u64;

fn mask_of(columns: impl Iterator<Item = usize>) -> Mask {
    columns.fold(0, |m, c| m | (1 << c))
}

/// A term with its constant already interned.
#[derive(Clone, Copy, Debug)]
enum T {
    Var(usize),
    Val(V),
    Wild,
}
#[derive(Clone, Debug)]
enum Lit {
    Pos(RelId, Vec<T>),
    Neg(RelId, Vec<T>),
    Cmp(Op, T, T),
    Builtin(Builtin, [T; 2]),
}
#[derive(Clone, Debug)]
enum Head {
    Term(T),
    Count(Vec<usize>),
}
/// A rule as the engine runs it.
struct Rule {
    head: RelId,
    terms: Vec<Head>,
    body: Vec<Lit>,
    vars: usize,
}
impl Rule {
    fn aggregates(&self) -> bool {
        self.terms.iter().any(|t| matches!(t, Head::Count(_)))
    }
}

#[derive(Clone, Copy, Debug)]
struct Step {
    literal: usize,
    mask: Mask,
    /// Read the last round's new rows instead of the whole relation.
    delta: bool,
}

/// Why evaluation stopped before the fixed point.
#[derive(Debug)]
enum Stop {
    Time,
    Rows(RelId),
    Rounds(RelId),
}

/// The order a rule's literals run in: a test or negation as soon as its
/// variables are bound, otherwise the positive relation with the most
/// bound columns (written order on ties). A delta literal goes first.
fn plan(rule: &Rule, first: Option<usize>) -> Vec<Step> {
    let mut bound = vec![false; rule.vars];
    let is_bound = |t: &T, bound: &[bool]| match t {
        T::Var(v) => bound[*v],
        _ => true,
    };
    let bound_mask = |terms: &[T], bound: &[bool]| {
        mask_of(
            terms
                .iter()
                .enumerate()
                .filter(|(_, t)| !matches!(t, T::Wild) && is_bound(t, bound))
                .map(|(c, _)| c),
        )
    };
    let bind_all = |terms: &[T], bound: &mut [bool]| {
        for t in terms {
            if let T::Var(v) = t {
                bound[*v] = true;
            }
        }
    };
    let mut left: Vec<usize> = (0..rule.body.len()).collect();
    let mut steps = Vec::new();
    if let Some(first) = first {
        left.retain(|&l| l != first);
        if let Lit::Pos(_, terms) = &rule.body[first] {
            bind_all(terms, &mut bound);
        }
        steps.push(Step {
            literal: first,
            mask: 0,
            delta: true,
        });
    }
    while !left.is_empty() {
        let ready = left.iter().position(|&l| match &rule.body[l] {
            Lit::Neg(_, terms) => terms
                .iter()
                .all(|t| matches!(t, T::Wild) || is_bound(t, &bound)),
            Lit::Cmp(Op::Eq, a, b) => is_bound(a, &bound) || is_bound(b, &bound),
            Lit::Builtin(Builtin::LastSegment, [a, _]) => is_bound(a, &bound),
            Lit::Cmp(_, a, b) | Lit::Builtin(_, [a, b]) => {
                is_bound(a, &bound) && is_bound(b, &bound)
            }
            Lit::Pos(..) => false,
        });
        let pick = ready.unwrap_or_else(|| {
            left.iter()
                .enumerate()
                .filter_map(|(p, &l)| match &rule.body[l] {
                    Lit::Pos(_, terms) => Some((p, bound_mask(terms, &bound).count_ones())),
                    _ => None,
                })
                .max_by(|x, y| x.1.cmp(&y.1).then(y.0.cmp(&x.0)))
                .map_or(0, |(p, _)| p)
        });
        let literal = left.remove(pick);
        let mut mask = 0;
        match &rule.body[literal] {
            Lit::Pos(_, terms) => {
                mask = bound_mask(terms, &bound);
                bind_all(terms, &mut bound);
            }
            Lit::Neg(_, terms) => mask = bound_mask(terms, &bound),
            Lit::Cmp(Op::Eq, a, b) => bind_all(&[*a, *b], &mut bound),
            Lit::Builtin(Builtin::LastSegment, [_, b]) => bind_all(&[*b], &mut bound),
            _ => {}
        }
        steps.push(Step {
            literal,
            mask,
            delta: false,
        });
    }
    steps
}

fn under(path: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    dir.is_empty()
        || path == dir
        || (path.starts_with(dir) && path.as_bytes().get(dir.len()) == Some(&b'/'))
}

struct Engine<'c> {
    checked: &'c Checked,
    rules: Vec<Rule>,
    strings: RefCell<Interner>,
    stores: Vec<Store>,
    indexes: HashMap<(RelId, Mask), Index>,
    limits: Limits,
    started: Instant,
    derived: usize,
    work: std::cell::Cell<u64>,
}

/// One rule's run: its steps, and the last round's rows for a delta step.
struct Pass<'a> {
    rule: &'a Rule,
    steps: &'a [Step],
    delta: &'a [Row],
}

/// Where a join emits its bindings.
struct Sink<'a> {
    project: &'a dyn Fn(&[Option<V>]) -> Row,
    out: Vec<Row>,
}

impl Engine<'_> {
    fn intern(strings: &mut Interner, c: &Const) -> V {
        match c {
            Const::Text(t) => V::S(strings.id(t)),
            Const::Integer(n) => V::I(*n),
            Const::Boolean(b) => V::B(*b),
        }
    }
    fn lower(strings: &mut Interner, rule: &CRule) -> Rule {
        let mut term = |t: &CTerm| match t {
            CTerm::Var(v) => T::Var(*v),
            CTerm::Const(c) => T::Val(Self::intern(strings, c)),
            CTerm::Wild => T::Wild,
        };
        let body = rule
            .body
            .iter()
            .map(|l| match l {
                CLit::Pos(rel, terms) => Lit::Pos(*rel, terms.iter().map(&mut term).collect()),
                CLit::Neg(rel, terms) => Lit::Neg(*rel, terms.iter().map(&mut term).collect()),
                CLit::Cmp(op, a, b) => Lit::Cmp(*op, term(a), term(b)),
                CLit::Builtin(builtin, [a, b]) => Lit::Builtin(*builtin, [term(a), term(b)]),
            })
            .collect();
        let terms = rule
            .terms
            .iter()
            .map(|h| match h {
                CHead::Term(t) => Head::Term(term(t)),
                CHead::Count(vs) => Head::Count(vs.clone()),
            })
            .collect();
        Rule {
            head: rule.head,
            terms,
            body,
            vars: rule.vars.len(),
        }
    }
    fn text(&self, v: V) -> String {
        match v {
            V::S(id) => self.strings.borrow().all[id as usize].clone(),
            V::I(n) => n.to_string(),
            V::B(b) => b.to_string(),
        }
    }
    fn compare(&self, a: V, b: V) -> Ordering {
        match (a, b) {
            (V::I(x), V::I(y)) => x.cmp(&y),
            (V::B(x), V::B(y)) => x.cmp(&y),
            (V::S(x), V::S(y)) => {
                let strings = self.strings.borrow();
                strings.all[x as usize].cmp(&strings.all[y as usize])
            }
            (V::B(_), _) | (V::I(_), V::S(_)) => Ordering::Less,
            _ => Ordering::Greater,
        }
    }
    fn cell(&self, v: V) -> Cell {
        match v {
            V::B(b) => Cell::Boolean(b),
            V::I(n) => Cell::Integer(n),
            V::S(id) => Cell::Text(self.strings.borrow().all[id as usize].clone()),
        }
    }
    fn load(&mut self, relations: &Relations) -> Result<(), Error> {
        for &rel in &self.checked.stored {
            let info = &self.checked.rels[rel];
            let Some(relation) = relations.get(&info.name) else {
                return Err(Error::plain(
                    ErrorKind::Facts,
                    format!(
                        "the facts do not include `{}` (the {} family)",
                        info.name,
                        info.stored.map_or("", |s| s.family.name())
                    ),
                ));
            };
            let strings = self.strings.get_mut();
            let mut store = Store::default();
            for row in &relation.rows {
                let values: Row = row
                    .iter()
                    .map(|cell| match cell {
                        Cell::Boolean(b) => V::B(*b),
                        Cell::Integer(n) => V::I(*n),
                        Cell::Text(t) => V::S(strings.id(t)),
                    })
                    .collect();
                store.insert(values);
            }
            self.stores[rel] = store;
        }
        Ok(())
    }
    fn ensure_indexes(&mut self, rule: usize, steps: &[Step]) {
        for step in steps {
            let (Lit::Pos(rel, terms) | Lit::Neg(rel, terms)) =
                &self.rules[rule].body[step.literal]
            else {
                continue;
            };
            let full = mask_of(0..terms.len());
            if step.delta || step.mask == 0 || step.mask == full {
                continue;
            }
            let rows = &self.stores[*rel].rows;
            let index = self.indexes.entry((*rel, step.mask)).or_default();
            while index.upto < rows.len() {
                let row = &rows[index.upto];
                let key: Row = (0..row.len())
                    .filter(|c| step.mask & (1 << c) != 0)
                    .map(|c| row[c])
                    .collect();
                index.map.entry(key).or_default().push(index.upto);
                index.upto += 1;
            }
        }
    }
    /// Count work, and stop at the time limit.
    fn tick(&self) -> Result<(), Stop> {
        let n = self.work.get() + 1;
        self.work.set(n);
        if n.is_multiple_of(4096) && self.started.elapsed() > self.limits.time {
            return Err(Stop::Time);
        }
        Ok(())
    }
    fn get(t: T, env: &[Option<V>]) -> Option<V> {
        match t {
            T::Var(v) => env[v],
            T::Val(v) => Some(v),
            T::Wild => None,
        }
    }
    /// Match `row` against `terms`, binding unbound variables; the caller
    /// undoes the bindings listed in `set`.
    fn unify(terms: &[T], row: &[V], env: &mut [Option<V>], set: &mut Vec<usize>) -> bool {
        for (column, term) in terms.iter().enumerate() {
            match *term {
                T::Wild => {}
                T::Val(v) => {
                    if row[column] != v {
                        return false;
                    }
                }
                T::Var(var) => match env[var] {
                    Some(x) if x != row[column] => return false,
                    Some(_) => {}
                    None => {
                        env[var] = Some(row[column]);
                        set.push(var);
                    }
                },
            }
        }
        true
    }
    fn key(terms: &[T], mask: Mask, env: &[Option<V>]) -> Row {
        terms
            .iter()
            .enumerate()
            .filter(|(c, _)| mask & (1 << c) != 0)
            .filter_map(|(_, t)| Self::get(*t, env))
            .collect()
    }
    /// Continue the join with `var` bound to `value`.
    fn bind_then(
        &self,
        pass: &Pass<'_>,
        at: usize,
        (var, value): (usize, V),
        env: &mut Vec<Option<V>>,
        sink: &mut Sink<'_>,
    ) -> Result<(), Stop> {
        env[var] = Some(value);
        let result = self.join(pass, at + 1, env, sink);
        env[var] = None;
        result
    }
    fn join(
        &self,
        pass: &Pass<'_>,
        at: usize,
        env: &mut Vec<Option<V>>,
        sink: &mut Sink<'_>,
    ) -> Result<(), Stop> {
        self.tick()?;
        let Some(step) = pass.steps.get(at) else {
            sink.out.push((sink.project)(env));
            if sink.out.len() > self.limits.rows {
                return Err(Stop::Rows(pass.rule.head));
            }
            return Ok(());
        };
        match &pass.rule.body[step.literal] {
            Lit::Pos(rel, terms) => {
                let store = &self.stores[*rel];
                let mut visit = |row: &[V], env: &mut Vec<Option<V>>| -> Result<(), Stop> {
                    let mut set = Vec::new();
                    let result = if Self::unify(terms, row, env, &mut set) {
                        self.join(pass, at + 1, env, sink)
                    } else {
                        Ok(())
                    };
                    for v in set {
                        env[v] = None;
                    }
                    result
                };
                if step.delta {
                    for row in pass.delta {
                        visit(row, env)?;
                    }
                } else if step.mask == 0 {
                    for row in &store.rows {
                        visit(row, env)?;
                    }
                } else if step.mask == mask_of(0..terms.len()) {
                    let row: Row = terms.iter().filter_map(|t| Self::get(*t, env)).collect();
                    if let Some(row) = store.set.get(&row) {
                        visit(row, env)?;
                    }
                } else {
                    let key = Self::key(terms, step.mask, env);
                    if let Some(positions) = self
                        .indexes
                        .get(&(*rel, step.mask))
                        .and_then(|index| index.map.get(&key))
                    {
                        for &p in positions {
                            visit(&store.rows[p], env)?;
                        }
                    }
                }
                Ok(())
            }
            Lit::Neg(rel, terms) => {
                let store = &self.stores[*rel];
                let exists = if step.mask == 0 {
                    !store.rows.is_empty()
                } else if step.mask == mask_of(0..terms.len()) {
                    let row: Row = terms.iter().filter_map(|t| Self::get(*t, env)).collect();
                    store.set.contains(&row)
                } else {
                    let key = Self::key(terms, step.mask, env);
                    self.indexes
                        .get(&(*rel, step.mask))
                        .is_some_and(|index| index.map.contains_key(&key))
                };
                if exists {
                    Ok(())
                } else {
                    self.join(pass, at + 1, env, sink)
                }
            }
            Lit::Cmp(op, left, right) => match (Self::get(*left, env), Self::get(*right, env)) {
                (Some(lv), Some(rv)) => {
                    let ordering = self.compare(lv, rv);
                    let holds = match op {
                        Op::Eq => lv == rv,
                        Op::Ne => lv != rv,
                        Op::Lt => ordering.is_lt(),
                        Op::Le => ordering.is_le(),
                        Op::Gt => ordering.is_gt(),
                        Op::Ge => ordering.is_ge(),
                    };
                    if holds {
                        self.join(pass, at + 1, env, sink)
                    } else {
                        Ok(())
                    }
                }
                (Some(value), None) => match right {
                    T::Var(v) => self.bind_then(pass, at, (*v, value), env, sink),
                    _ => Ok(()),
                },
                (None, Some(value)) => match left {
                    T::Var(v) => self.bind_then(pass, at, (*v, value), env, sink),
                    _ => Ok(()),
                },
                (None, None) => Ok(()),
            },
            Lit::Builtin(builtin, [input, other]) => {
                let Some(value) = Self::get(*input, env) else {
                    return Ok(());
                };
                let text = self.text(value);
                if *builtin == Builtin::LastSegment {
                    let segment = text.rsplit("::").next().unwrap_or_default();
                    let segment = V::S(self.strings.borrow_mut().id(segment));
                    return match (other, Self::get(*other, env)) {
                        (_, Some(given)) if given == segment => self.join(pass, at + 1, env, sink),
                        (T::Var(v), None) => self.bind_then(pass, at, (*v, segment), env, sink),
                        _ => Ok(()),
                    };
                }
                let Some(second) = Self::get(*other, env) else {
                    return Ok(());
                };
                let second = self.text(second);
                let holds = match builtin {
                    Builtin::Under => under(&text, &second),
                    Builtin::StartsWith => text.starts_with(&second),
                    Builtin::EndsWith => text.ends_with(&second),
                    Builtin::Contains => text.contains(&second),
                    Builtin::LastSegment => false,
                };
                if holds {
                    self.join(pass, at + 1, env, sink)
                } else {
                    Ok(())
                }
            }
        }
    }
    /// Every row one rule derives, against the whole of each relation or,
    /// with `delta`, with its first step reading only the last round's rows.
    fn derive(
        &mut self,
        rule: usize,
        first: Option<usize>,
        delta: &[Row],
    ) -> Result<Vec<Row>, Stop> {
        let steps = plan(&self.rules[rule], first);
        self.ensure_indexes(rule, &steps);
        let rule = &self.rules[rule];
        let pass = Pass {
            rule,
            steps: &steps,
            delta,
        };
        let mut env = vec![None; rule.vars];
        if rule.aggregates() {
            // Project the plain head terms, then each count's variables;
            // `group` turns the bindings into one row per group.
            let project = |env: &[Option<V>]| -> Row {
                let mut row = Vec::new();
                for term in &rule.terms {
                    match term {
                        Head::Term(t) => row.extend(Self::get(*t, env)),
                        Head::Count(vs) => row.extend(vs.iter().filter_map(|v| env[*v])),
                    }
                }
                row.into()
            };
            let mut sink = Sink {
                project: &project,
                out: Vec::new(),
            };
            self.join(&pass, 0, &mut env, &mut sink)?;
            return Ok(group(rule, sink.out));
        }
        let project = |env: &[Option<V>]| -> Row {
            rule.terms
                .iter()
                .filter_map(|term| match term {
                    Head::Term(t) => Self::get(*t, env),
                    Head::Count(_) => None,
                })
                .collect()
        };
        let mut sink = Sink {
            project: &project,
            out: Vec::new(),
        };
        self.join(&pass, 0, &mut env, &mut sink)?;
        Ok(sink.out)
    }
    fn insert(&mut self, rel: RelId, rows: Vec<Row>, fresh: &mut Vec<Row>) -> Result<(), Stop> {
        for row in rows {
            if self.stores[rel].insert(row.clone()) {
                fresh.push(row);
                self.derived += 1;
                if self.derived > self.limits.rows {
                    return Err(Stop::Rows(rel));
                }
            }
        }
        Ok(())
    }
    fn stratum(&mut self, rules: &[usize]) -> Result<(), Stop> {
        let in_stratum: HashSet<RelId> = rules.iter().map(|&r| self.rules[r].head).collect();
        let mut delta: HashMap<RelId, Vec<Row>> = HashMap::new();
        // Counts first: their bodies read only lower strata.
        let mut ordered: Vec<usize> = rules.to_vec();
        ordered.sort_by_key(|&r| !self.rules[r].aggregates());
        for &r in &ordered {
            let rows = self.derive(r, None, &[])?;
            let mut fresh = Vec::new();
            self.insert(self.rules[r].head, rows, &mut fresh)?;
            delta.entry(self.rules[r].head).or_default().extend(fresh);
        }
        let mut rounds = 0usize;
        while delta.values().any(|rows| !rows.is_empty()) {
            rounds += 1;
            if rounds > self.limits.rounds {
                let grew = delta
                    .iter()
                    .max_by_key(|(_, rows)| rows.len())
                    .map_or(0, |(rel, _)| *rel);
                return Err(Stop::Rounds(grew));
            }
            let mut next: HashMap<RelId, Vec<Row>> = HashMap::new();
            for &r in &ordered {
                if self.rules[r].aggregates() {
                    continue;
                }
                let recursive: Vec<(usize, RelId)> = self.rules[r]
                    .body
                    .iter()
                    .enumerate()
                    .filter_map(|(l, lit)| match lit {
                        Lit::Pos(rel, _) if in_stratum.contains(rel) => Some((l, *rel)),
                        _ => None,
                    })
                    .collect();
                for (literal, rel) in recursive {
                    let Some(rows) = delta.get(&rel).filter(|rows| !rows.is_empty()) else {
                        continue;
                    };
                    let rows = rows.clone();
                    let derived = self.derive(r, Some(literal), &rows)?;
                    let mut fresh = Vec::new();
                    self.insert(self.rules[r].head, derived, &mut fresh)?;
                    next.entry(self.rules[r].head).or_default().extend(fresh);
                }
            }
            delta = next;
        }
        Ok(())
    }
    fn table(&self, query: &CQuery) -> Table {
        let info = &self.checked.rels[query.rel];
        let mut rows: Vec<&Row> = self.stores[query.rel].rows.iter().collect();
        rows.sort_by(|a, b| {
            for &(column, descending) in &query.order {
                let o = self.compare(a[column], b[column]);
                let o = if descending { o.reverse() } else { o };
                if o.is_ne() {
                    return o;
                }
            }
            a.iter()
                .zip(b.iter())
                .map(|(x, y)| self.compare(*x, *y))
                .find(|o| o.is_ne())
                .unwrap_or(Ordering::Equal)
        });
        let total = rows.len();
        let shown = query.limit.unwrap_or(usize::MAX).min(total);
        Table {
            query: query.text.clone(),
            columns: info.columns.iter().map(|(c, _)| c.clone()).collect(),
            kinds: info
                .columns
                .iter()
                .map(|(_, k)| match k {
                    Kind::Integer => "integer",
                    Kind::Boolean => "boolean",
                    _ => "text",
                })
                .collect(),
            rows: rows[..shown]
                .iter()
                .map(|row| row.iter().map(|v| self.cell(*v)).collect())
                .collect(),
            total,
        }
    }
}

/// One row per group: the plain head values, then each count of distinct
/// bindings, in head order. A group with no bindings has no row; a count is
/// never 0.
fn group(rule: &Rule, bindings: Vec<Row>) -> Vec<Row> {
    let counts = rule
        .terms
        .iter()
        .filter(|t| matches!(t, Head::Count(_)))
        .count();
    let mut groups: HashMap<Vec<V>, Vec<HashSet<Vec<V>>>> = HashMap::new();
    for binding in bindings {
        let mut values = binding.iter().copied();
        let mut key = Vec::new();
        let mut counted: Vec<Vec<V>> = Vec::new();
        for term in &rule.terms {
            match term {
                Head::Term(_) => key.extend(values.next()),
                Head::Count(vs) => counted.push(values.by_ref().take(vs.len()).collect()),
            }
        }
        let sets = groups
            .entry(key)
            .or_insert_with(|| vec![HashSet::new(); counts]);
        for (set, tuple) in sets.iter_mut().zip(counted) {
            set.insert(tuple);
        }
    }
    groups
        .into_iter()
        .map(|(key, sets)| {
            let mut key = key.into_iter();
            let mut sets = sets.into_iter();
            rule.terms
                .iter()
                .filter_map(|term| match term {
                    Head::Term(_) => key.next(),
                    Head::Count(_) => sets
                        .next()
                        .map(|s| V::I(u64::try_from(s.len()).unwrap_or(u64::MAX))),
                })
                .collect()
        })
        .collect()
}

pub(crate) fn run(
    checked: &Checked,
    relations: &Relations,
    limits: Limits,
) -> Result<Answer, Error> {
    let mut strings = Interner::default();
    let rules = checked
        .rules
        .iter()
        .map(|r| Engine::lower(&mut strings, r))
        .collect();
    let mut engine = Engine {
        checked,
        rules,
        strings: RefCell::new(strings),
        stores: (0..checked.rels.len()).map(|_| Store::default()).collect(),
        indexes: HashMap::new(),
        limits,
        started: Instant::now(),
        derived: 0,
        work: std::cell::Cell::new(0),
    };
    engine.load(relations)?;
    engine.started = Instant::now();
    for stratum in &checked.strata {
        engine.stratum(stratum).map_err(|stop| {
            let message = match stop {
                Stop::Time => format!(
                    "evaluation passed its time limit of {} s with {} rows derived; root the rules more narrowly, or raise the limit",
                    limits.time.as_secs(),
                    engine.derived
                ),
                Stop::Rows(rel) => format!(
                    "`{}` grew past the limit of {} derived rows; root the rules more narrowly (an unrooted recursion derives every pair), or raise the limit",
                    checked.rels[rel].name, limits.rows
                ),
                Stop::Rounds(rel) => format!(
                    "`{}` was still growing after {} rounds of recursion; root the rules more narrowly, or raise the limit",
                    checked.rels[rel].name, limits.rounds
                ),
            };
            Error::plain(ErrorKind::Limit, message)
        })?;
    }
    let tables = checked.queries.iter().map(|q| engine.table(q)).collect();
    Ok(Answer {
        tables,
        derived: engine.derived,
        elapsed: engine.started.elapsed(),
        accepted: Vec::new(),
        notes: Vec::new(),
        work: engine.work.get(),
    })
}
