//! Flat relations over a captured workspace: the lexical source graph and the
//! qualified analysis facts as named tables with documented columns, for
//! tools that compose their own questions instead of reading each command's
//! report shape.
//!
//! The export flattens but never collapses. An ambiguous lexical mention is
//! one `mention_target` row per candidate with a `basis` column; an
//! unresolved one is a `mention_unresolved` row. A relation of resolved
//! observations names its unknowns companion (`call` and `unknown_call`), so a
//! consumer can tell "no row" from "no evidence either way". Observations that
//! could otherwise be told apart only by position carry a witness id.
//!
//! The two families keep their own identity schemes (`path:line:name` and
//! `path:line:column`). A qualified function or type also names the lexical
//! declaration at its file, line and name, so the families can be joined
//! without guessing. Nothing here resolves a name anew: every row is an
//! observation the capture or the analysis already made.
use crate::{
    model::{Resolution, SourceGraph},
    representations::Facts,
};
use serde::{Serialize, ser::SerializeMap};
use std::collections::{BTreeMap, BTreeSet};

pub mod schema;
pub use schema::{Family, VERSION};

/// One value. The column's [`schema::Kind`] says which variant it holds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Cell {
    Boolean(bool),
    Integer(u64),
    Text(String),
}
impl Cell {
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        if let Self::Text(text) = self {
            Some(text)
        } else {
            None
        }
    }
    #[must_use]
    pub fn integer(&self) -> Option<u64> {
        if let Self::Integer(n) = self {
            Some(*n)
        } else {
            None
        }
    }
    #[must_use]
    pub fn boolean(&self) -> Option<bool> {
        if let Self::Boolean(b) = self {
            Some(*b)
        } else {
            None
        }
    }
}
impl From<&str> for Cell {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}
impl From<String> for Cell {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}
impl From<&String> for Cell {
    fn from(text: &String) -> Self {
        Self::Text(text.clone())
    }
}
impl From<bool> for Cell {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl From<usize> for Cell {
    fn from(n: usize) -> Self {
        Self::Integer(u64::try_from(n).unwrap_or(u64::MAX))
    }
}

/// The rows of one relation, in the order its schema lists the columns.
#[derive(Debug)]
#[non_exhaustive]
pub struct Relation {
    pub schema: &'static schema::Relation,
    pub rows: Vec<Vec<Cell>>,
}
impl Relation {
    fn new(name: &str) -> Self {
        Self {
            schema: schema::relation(name).expect("relation declared in schema v1"),
            rows: Vec::new(),
        }
    }
    fn push<const N: usize>(&mut self, row: [Cell; N]) {
        debug_assert_eq!(N, self.schema.columns.len(), "{}", self.schema.name);
        self.rows.push(row.into());
    }
    /// Each row as column name and value pairs.
    pub fn records(&self) -> impl Iterator<Item = impl Iterator<Item = (&'static str, &Cell)>> {
        self.rows.iter().map(|row| {
            self.schema
                .columns
                .iter()
                .map(|column| column.name)
                .zip(row.iter())
        })
    }
}

/// Every relation of the selected families, with the analysis limitations
/// that apply to all of them.
#[derive(Debug)]
#[non_exhaustive]
pub struct Relations {
    pub schema: u32,
    pub families: Vec<Family>,
    /// Limits of the qualified population, as the analysis reports them.
    pub limitations: Vec<String>,
    pub relations: Vec<Relation>,
}
impl Relations {
    /// Flatten `graph` (the lexical family) and, when given, `facts` (the
    /// qualified family). Pass facts built from the same snapshot as the
    /// graph: the bridge from qualified to lexical rows matches by file and
    /// line and cannot detect two different captures.
    #[must_use]
    pub fn capture(graph: &SourceGraph, facts: Option<&Facts>) -> Self {
        let mut relations = lexical(graph);
        let mut families = vec![Family::Lexical];
        let mut limitations = Vec::new();
        if let Some(facts) = facts {
            relations.extend(qualified(graph, facts));
            families.push(Family::Qualified);
            limitations.clone_from(&facts.limitations);
        }
        Self {
            schema: VERSION,
            families,
            limitations,
            relations,
        }
    }
    /// The relation called `name`, if its family was captured.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Relation> {
        self.relations.iter().find(|r| r.schema.name == name)
    }
    /// One JSON document: the header, then each relation's columns and rows
    /// (rows are arrays in column order).
    pub fn write_json(&self, out: impl std::io::Write) -> serde_json::Result<()> {
        serde_json::to_writer(out, self)
    }
    /// JSON Lines: a header object (`schema`, `families`, `limitations`),
    /// then one object per row with a `relation` key and its named columns.
    pub fn write_jsonl(&self, mut out: impl std::io::Write) -> std::io::Result<()> {
        #[derive(Serialize)]
        struct Header<'a> {
            schema: u32,
            families: &'a [Family],
            limitations: &'a [String],
        }
        struct Row<'a>(&'static str, &'a Relation, &'a [Cell]);
        impl Serialize for Row<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let mut map = s.serialize_map(Some(self.2.len() + 1))?;
                map.serialize_entry("relation", self.0)?;
                for (column, cell) in self.1.schema.columns.iter().zip(self.2) {
                    map.serialize_entry(column.name, cell)?;
                }
                map.end()
            }
        }
        serde_json::to_writer(
            &mut out,
            &Header {
                schema: self.schema,
                families: &self.families,
                limitations: &self.limitations,
            },
        )?;
        out.write_all(b"\n")?;
        for relation in &self.relations {
            for row in &relation.rows {
                serde_json::to_writer(&mut out, &Row(relation.schema.name, relation, row))?;
                out.write_all(b"\n")?;
            }
        }
        Ok(())
    }
}
impl Serialize for Relations {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Table<'a> {
            family: Family,
            columns: Vec<&'static str>,
            basis: Option<&'static str>,
            unknowns: Option<&'static str>,
            rows: &'a [Vec<Cell>],
        }
        struct Tables<'a>(&'a [Relation]);
        impl Serialize for Tables<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let mut map = s.serialize_map(Some(self.0.len()))?;
                for relation in self.0 {
                    let schema = relation.schema;
                    map.serialize_entry(
                        schema.name,
                        &Table {
                            family: schema.family,
                            columns: schema.columns.iter().map(|c| c.name).collect(),
                            basis: schema.basis,
                            unknowns: schema.unknowns,
                            rows: &relation.rows,
                        },
                    )?;
                }
                map.end()
            }
        }
        let mut map = s.serialize_map(Some(4))?;
        map.serialize_entry("schema", &self.schema)?;
        map.serialize_entry("families", &self.families)?;
        map.serialize_entry("limitations", &self.limitations)?;
        map.serialize_entry("relations", &Tables(&self.relations))?;
        map.end()
    }
}

fn witness(relation: &str, n: usize) -> Cell {
    Cell::Text(format!("{relation}:{n}"))
}
fn label(value: &impl Serialize) -> Cell {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => Cell::Text(text),
        Ok(other) => Cell::Text(other.to_string()),
        Err(_) => Cell::Text(String::new()),
    }
}

fn lexical(graph: &SourceGraph) -> Vec<Relation> {
    let mut file = Relation::new("file");
    let mut mount = Relation::new("mount");
    let mut declaration = Relation::new("declaration");
    let mut import = Relation::new("import");
    for f in &graph.files {
        file.push([
            (&f.path).into(),
            (&f.crate_name).into(),
            f.test_only.into(),
            (&f.content_fingerprint).into(),
        ]);
        for m in &f.mounts {
            mount.push([
                (&f.path).into(),
                (&f.crate_name).into(),
                (&m.root).into(),
                m.modules.join("::").into(),
                m.target.clone().unwrap_or_default().into(),
                m.test_only.into(),
            ]);
        }
        for d in &f.declarations {
            declaration.push([
                (&d.id).into(),
                (&f.crate_name).into(),
                (&f.path).into(),
                (&d.name).into(),
                (&d.kind).into(),
                (&d.visibility).into(),
                d.line.into(),
                d.end_line.into(),
                d.modules.join("::").into(),
                d.test_only.into(),
            ]);
        }
        for written in &f.imports {
            import.push([
                (&f.path).into(),
                (&f.crate_name).into(),
                (&**written).into(),
            ]);
        }
    }
    let mut mention = Relation::new("mention");
    let mut target = Relation::new("mention_target");
    let mut unresolved = Relation::new("mention_unresolved");
    for r in &graph.relationships {
        mention.push([
            (&r.id).into(),
            (&r.source).into(),
            (&r.spelling).into(),
            label(&r.evidence),
        ]);
        match &r.target {
            Resolution::Unique(id) => target.push([(&r.id).into(), id.into(), "unique".into()]),
            Resolution::Ambiguous(ids) => {
                for id in ids {
                    target.push([(&r.id).into(), id.into(), "candidate".into()]);
                }
            }
            Resolution::Unresolved(reason) => unresolved.push([(&r.id).into(), reason.into()]),
        }
    }
    let mut diagnostic = Relation::new("diagnostic");
    for (n, d) in graph.diagnostics.iter().enumerate() {
        diagnostic.push([
            witness("diagnostic", n),
            (&d.file).into(),
            d.line.unwrap_or(0).into(),
            d.package.clone().unwrap_or_default().into(),
            (&d.message).into(),
        ]);
    }
    vec![
        file,
        mount,
        declaration,
        import,
        mention,
        target,
        unresolved,
        diagnostic,
    ]
}

/// Lexical declaration ids by file, start line and written name, for the
/// bridge from a qualified item to the declaration Atlas read at its site.
struct Bridge<'a>(BTreeMap<(&'a str, usize, &'a str), BTreeSet<&'a str>>);
impl<'a> Bridge<'a> {
    fn new(graph: &'a SourceGraph, kinds: &[&str]) -> Self {
        let mut sites = BTreeMap::<_, BTreeSet<&str>>::new();
        for file in &graph.files {
            for d in file
                .declarations
                .iter()
                .filter(|d| kinds.contains(&d.kind.as_str()))
            {
                sites
                    .entry((file.path.as_str(), d.line, d.name.as_str()))
                    .or_default()
                    .insert(d.id.as_str());
            }
        }
        Self(sites)
    }
    /// The one declaration at this site, or empty when none or several match.
    fn find(&self, path: &str, line: usize, name: &str) -> Cell {
        let last = name.rsplit("::").next().unwrap_or(name);
        match self.0.get(&(path, line, last)) {
            Some(ids) if ids.len() == 1 => ids.iter().next().copied().unwrap_or_default().into(),
            _ => Cell::Text(String::new()),
        }
    }
}

fn qualified(graph: &SourceGraph, facts: &Facts) -> Vec<Relation> {
    let functions = Bridge::new(graph, &["Fn"]);
    let types = Bridge::new(graph, &["Struct", "Enum", "Union", "TypeAlias"]);
    let mut file = Relation::new("qualified_file");
    for (path, fingerprint) in &facts.fingerprints {
        file.push([path.into(), fingerprint.into()]);
    }
    let mut function = Relation::new("function");
    for f in &facts.functions {
        function.push([
            (&f.id).into(),
            (&f.file).into(),
            f.line.into(),
            (&f.name).into(),
            (&f.qualified).into(),
            (&f.signature).into(),
            f.public.into(),
            f.trait_impl.into(),
            functions.find(&f.file, f.line, &f.name),
        ]);
    }
    let mut ty = Relation::new("type");
    let mut field = Relation::new("field");
    for t in &facts.types {
        ty.push([
            (&t.id).into(),
            (&t.file).into(),
            t.line.into(),
            (&t.name).into(),
            (&t.qualified).into(),
            t.alias.clone().unwrap_or_default().into(),
            types.find(&t.file, t.line, &t.name),
        ]);
        for (position, (name, written)) in t.fields.iter().enumerate() {
            field.push([(&t.id).into(), position.into(), name.into(), written.into()]);
        }
    }
    let mut call = Relation::new("call");
    for (n, c) in facts.calls.iter().enumerate() {
        call.push([
            witness("call", n),
            (&c.from).into(),
            (&c.to).into(),
            (&c.site.path).into(),
            c.site.line.into(),
            c.site.column.into(),
        ]);
    }
    let mut unknown_call = Relation::new("unknown_call");
    for (n, u) in facts.unknown_calls.iter().enumerate() {
        unknown_call.push([
            witness("unknown_call", n),
            (&u.from).into(),
            (&u.call.spelling).into(),
            u.call.receiver.clone().unwrap_or_default().into(),
            (&u.reason).into(),
            (&u.call.site.path).into(),
            u.call.site.line.into(),
            u.call.site.column.into(),
        ]);
    }
    let mut reference = Relation::new("reference");
    for (n, r) in facts.references.iter().enumerate() {
        reference.push([
            witness("reference", n),
            (&r.from).into(),
            (&r.to).into(),
            label(&r.role),
            (&r.slot).into(),
            (&r.expression).into(),
            r.line.into(),
            r.borrow.into(),
        ]);
    }
    let mut unknown_type = Relation::new("unknown_type");
    for (n, u) in facts.unknown_types.iter().enumerate() {
        unknown_type.push([
            witness("unknown_type", n),
            (&u.from).into(),
            (&u.spelling).into(),
            label(&u.role),
            (&u.reason).into(),
            (&u.slot).into(),
            u.line.into(),
            u.column.into(),
        ]);
    }
    let built = |name: &str, list: &[crate::representations::Constructor]| {
        let mut relation = Relation::new(name);
        for (n, c) in list.iter().enumerate() {
            relation.push([
                witness(name, n),
                (&c.from).into(),
                (&c.to).into(),
                c.line.into(),
            ]);
        }
        relation
    };
    let construction = built("construction", &facts.constructors);
    let default_call = built("default_call", &facts.default_calls);
    let mut constant = Relation::new("constant");
    for c in &facts.constants {
        constant.push([
            c.id().into(),
            (&c.site.path).into(),
            c.site.line.into(),
            (&c.name).into(),
            c.module.join("::").into(),
            serde_json::to_string(&c.value).unwrap_or_default().into(),
        ]);
    }
    vec![
        file,
        function,
        ty,
        field,
        call,
        unknown_call,
        reference,
        unknown_type,
        construction,
        default_call,
        constant,
    ]
}
