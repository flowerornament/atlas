//! Semantic call evidence: a compiler front end's name and type resolution for
//! one build configuration, read from a SCIP index an external indexer wrote
//! (rust-analyzer's `scip` command). A fourth evidence family beside lexical,
//! qualified and native.
//!
//! The reading sits beside each qualified [`Target`]; it never changes one.
//! Agreement between the two is reported per call, never resolved, and no
//! candidate is ever picked. Limits of the result:
//! - One configuration: the indexer's host target, features and `cfg(test)`,
//!   which the index does not record. Code inactive there has no definitions.
//! - Currency: Atlas runs no indexer here. Each site is checked against the
//!   captured source text instead; a mismatch reads as not indexed.
//! - An index naming no standard-library symbol is unavailable as a whole: an
//!   indexer that could not load the sysroot writes one that looks complete.
//! - Calls Atlas does not record (inside macro payloads) are not read; a
//!   closure's or `async` block's calls are its enclosing function's.
//! - A stale index is caught per call, not trusted: the index's symbol must
//!   name what Atlas found written at the site, and a document whose text the
//!   index carries must be the captured text.
//!
//! This module reads the bytes it is given and does no I/O.
mod locate;
mod wire;

use crate::{
    analysis::index::{Index, Target},
    workflow::{Call, Site},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Version of the semantic evidence JSON; lexical and qualified schemas do
/// not move when it changes.
pub const SCHEMA: u32 = 1;

/// Where a semantic reading came from and what it can claim.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct Provenance {
    /// `supplied`: the caller gave Atlas an index file it did not produce.
    /// `indexed`: the caller ran an indexer on the inspected workspace for
    /// this reading (the command line's `--semantic`).
    pub origin: &'static str,
    /// SHA-256 of the index bytes, in hex.
    pub index_digest: String,
    pub producer: String,
    pub producer_version: String,
    pub project_root: String,
    pub position_encoding: &'static str,
    pub configuration: &'static str,
    pub currency: &'static str,
}

/// What the index covers, so a partial reading never backs a complete claim.
#[derive(Debug, Clone, Default, Serialize)]
#[non_exhaustive]
pub struct Coverage {
    pub documents: usize,
    /// Core, alloc or std symbols occur in the index.
    pub sysroot_observed: bool,
    /// Symbols the index defines at more than one site.
    pub symbols_with_several_definitions: usize,
    /// Files with read calls that the index has no document for.
    pub files_not_indexed: Vec<String>,
}

/// A definition site in the index: workspace-relative path, one-based line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct DefinitionSite {
    pub path: String,
    pub line: usize,
}

/// What the index says about one callee name.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "reading", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Semantic {
    /// A workspace item; dispatch is fixed.
    Definition {
        site: DefinitionSite,
        kind: String,
        symbol: String,
    },
    /// A trait method declaration: dispatch is open (generic or dyn), and the
    /// index lists no implementors.
    TraitMethod {
        declaration: DefinitionSite,
        symbol: String,
    },
    /// A workspace symbol the index defines nowhere: derive- or
    /// macro-generated, inactive under the host cfg, or lost by the indexer.
    NoDefinition { symbol: String },
    /// An item of another package, including the standard library.
    External {
        package: String,
        version: String,
        item: String,
    },
    /// A local binding: a closure or function value.
    Local,
    /// The index does not identify one definition.
    Ambiguous { reason: String },
    /// The index names a symbol whose name is not the one written here: the
    /// index was made from other source. Never read as an edge.
    Mismatch { symbol: String },
    /// The index says nothing usable at this site.
    NotIndexed { reason: String },
}

/// The qualified target, as compared.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "target", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Qualified {
    Function { function: String, site: String },
    Boundary,
    Summarized { reason: String },
}

/// Qualified and semantic readings of one call, compared by site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Agreement {
    /// Both name the same workspace function.
    Agrees,
    /// Qualified names no workspace function; the index names a target.
    Extends,
    /// Both name targets and they differ. A finding: a stale index, a cfg
    /// difference or a resolver bug; reported, never resolved.
    Disagrees,
    /// The index does not identify one definition.
    Ambiguous,
    /// The index says nothing usable at this site.
    Silent,
}

/// One recorded call with its readings.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct CallReading {
    /// The calling function's id.
    pub caller: String,
    pub site: Site,
    pub spelling: String,
    pub receiver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified: Option<Qualified>,
    pub semantic: Semantic,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agreement: Option<Agreement>,
    /// The calling function's file and line, where the reading was joined
    /// with a qualified index: how relations name the caller.
    #[serde(skip)]
    pub caller_site: Option<(String, usize)>,
}

/// One recorded call's semantic reading, with its caller and, when the index
/// defines the callee at a captured function, its target, by `function.id`.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Named<'a> {
    pub call: &'a CallReading,
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
    /// The index defines the callee where several captured functions start
    /// (a macro writing several on one line): `to` is none, and the call is
    /// not ruled out for any of them.
    pub shared: bool,
}
impl Reading {
    /// Where the reading came from and what it covers, one sentence each:
    /// the limits a report built on it states.
    #[must_use]
    pub fn describe(&self) -> Vec<String> {
        let provenance = &self.provenance;
        let mut lines = vec![format!(
            "Semantic reading ({}): {} {}, index sha256 {}; {}; {}.",
            provenance.origin,
            provenance.producer,
            provenance.producer_version,
            provenance.index_digest,
            provenance.configuration,
            provenance.currency
        )];
        if let Some(coverage) = &self.coverage {
            lines.push(format!(
                "Semantic coverage: {} indexed documents; {} files with recorded calls not in the index; {} symbols defined at several sites.",
                coverage.documents,
                coverage.files_not_indexed.len(),
                coverage.symbols_with_several_definitions
            ));
        }
        lines
    }
    /// Each call named by `facts`'s function ids: a function is its file and
    /// line, as the index and the facts both record it; a site several
    /// captured functions share names none.
    pub fn named<'a>(&'a self, facts: &'a crate::representations::Facts) -> Vec<Named<'a>> {
        let mut at: BTreeMap<(&str, usize), Vec<&str>> = BTreeMap::new();
        for function in &facts.functions {
            at.entry((function.file.as_str(), function.line))
                .or_default()
                .push(function.id.as_str());
        }
        let function_at = |path: &str, line: usize| match at.get(&(path, line)).map(Vec::as_slice) {
            Some([one]) => Some(*one),
            _ => None,
        };
        let shared =
            |path: &str, line: usize| at.get(&(path, line)).is_some_and(|ids| ids.len() > 1);
        self.calls
            .iter()
            .map(|call| Named {
                call,
                from: call
                    .caller_site
                    .as_ref()
                    .and_then(|(path, line)| function_at(path, *line)),
                to: match &call.semantic {
                    Semantic::Definition { site, .. } => function_at(&site.path, site.line),
                    _ => None,
                },
                shared: matches!(&call.semantic, Semantic::Definition { site, .. } if shared(&site.path, site.line)),
            })
            .collect()
    }
}

/// A semantic reading of a population of calls.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct Reading {
    /// Always `semantic`: this evidence family.
    pub evidence: &'static str,
    pub schema: u32,
    /// `available` or `unavailable`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
    pub provenance: Provenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// Calls by semantic reading kind.
    pub readings: BTreeMap<String, usize>,
    /// Calls by agreement, when a qualified target was compared.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub agreement: BTreeMap<Agreement, usize>,
    pub calls: Vec<CallReading>,
}

/// One supplied SCIP index, checked and prepared for lookup.
pub struct SemanticIndex {
    provenance: Provenance,
    tables: Result<Tables, Unusable>,
}

/// Why an index gives no reading: it is not valid SCIP, does not describe
/// a workspace Atlas can read, or could not be obtained.
#[derive(Debug, Clone)]
struct Unusable(String);
impl From<String> for Unusable {
    fn from(reason: String) -> Self {
        Self(reason)
    }
}
impl From<&str> for Unusable {
    fn from(reason: &str) -> Self {
        Self(reason.to_owned())
    }
}

/// A reference occurrence's start: path, zero-based line and column.
type Start = (String, u32, u32);
/// A reference occurrence's end (line, column) and symbol.
type Ending = ([u32; 2], String);

struct Tables {
    references: HashMap<Start, Vec<Ending>>,
    definitions: HashMap<String, Vec<DefinitionSite>>,
    kinds: HashMap<String, u64>,
    /// `manager package version` of every package the index defines items of.
    packages: BTreeSet<String>,
    documents: BTreeSet<String>,
    /// The text of each document the index carries it for.
    texts: HashMap<String, String>,
    coverage: Coverage,
}

impl Provenance {
    /// A supplied index's provenance before its header is read.
    fn supplied(index_digest: String) -> Self {
        Self {
            origin: "supplied",
            index_digest,
            producer: String::new(),
            producer_version: String::new(),
            project_root: String::new(),
            position_encoding: "UTF-8 code units from line start",
            configuration: "the indexer's host target, default features and cfg(test); the index does not record them",
            currency: "per-site text match against the captured source",
        }
    }
}

impl SemanticIndex {
    /// Check and prepare an index. A malformed or unusable index is kept as
    /// unavailable with its reason; it never fails the caller's command.
    pub fn read(bytes: &[u8]) -> Self {
        let mut provenance = Provenance::supplied(crate::source::hex(&Sha256::digest(bytes)));
        let tables = wire::parse(bytes)
            .map_err(|error| Unusable(format!("the index is not valid SCIP: {error}")))
            .and_then(|index| {
                provenance.producer.clone_from(&index.tool);
                provenance.producer_version.clone_from(&index.version);
                provenance.project_root.clone_from(&index.project_root);
                Tables::new(&index)
            });
        Self { provenance, tables }
    }
    /// [`SemanticIndex::read`] for an index the caller produced from the
    /// inspected workspace itself, so its provenance says `indexed`. The
    /// caller is responsible for that claim: Atlas cannot tell from the bytes.
    pub fn read_indexed(bytes: &[u8]) -> Self {
        let mut index = Self::read(bytes);
        index.provenance.origin = "indexed";
        index
    }
    /// An index that could not be obtained, such as an unreadable file: every
    /// reading from it is unavailable with `reason`.
    pub fn unavailable_because(reason: String) -> Self {
        Self {
            provenance: Provenance::supplied(String::new()),
            tables: Err(Unusable(reason)),
        }
    }
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
    /// Why no reading can be made from this index.
    pub fn unavailable(&self) -> Option<&str> {
        self.tables.as_ref().err().map(|reason| reason.0.as_str())
    }

    /// Read calls without a qualified comparison. `text` gives each file's
    /// captured source by workspace-relative path.
    pub fn read_calls<'a>(
        &self,
        calls: impl IntoIterator<Item = (&'a str, &'a Call)>,
        text: &dyn Fn(&str) -> Option<&'a str>,
    ) -> Reading {
        self.reading(
            calls
                .into_iter()
                .map(|(caller, call)| (caller, call, None, None)),
            text,
        )
    }

    /// Read every call of a qualified index and compare it with its target.
    pub fn join<'a>(&self, index: &'a Index, text: &dyn Fn(&str) -> Option<&'a str>) -> Reading {
        let functions = index.functions();
        let outside = |t: usize| Qualified::Summarized {
            reason: format!("function position {t} outside the index"),
        };
        let calls = functions.iter().enumerate().flat_map(|(i, f)| {
            f.flow.calls.iter().map(move |call| {
                let (qualified, target) = match index.resolve(i, call) {
                    Target::Function(t) => functions.get(t).map_or_else(
                        || (outside(t), None),
                        |target| {
                            let qualified = Qualified::Function {
                                function: target.id.clone(),
                                site: format!(
                                    "{}:{}",
                                    target.flow.site.path, target.flow.site.line
                                ),
                            };
                            (qualified, Some(target))
                        },
                    ),
                    Target::Boundary(..) => (Qualified::Boundary, None),
                    Target::Summarized(reason) => (
                        Qualified::Summarized {
                            reason: reason.message,
                        },
                        None,
                    ),
                    Target::InvalidFunction(t) => (outside(t), None),
                };
                (f.id.as_str(), call, Some((qualified, target)), Some(f))
            })
        });
        self.reading(calls, text)
    }

    fn reading<'a>(
        &self,
        calls: impl Iterator<
            Item = (
                &'a str,
                &'a Call,
                Option<(Qualified, Option<&'a crate::analysis::index::Function>)>,
                Option<&'a crate::analysis::index::Function>,
            ),
        >,
        text: &dyn Fn(&str) -> Option<&'a str>,
    ) -> Reading {
        let mut reading = Reading {
            evidence: "semantic",
            schema: SCHEMA,
            status: "unavailable",
            unavailable: None,
            provenance: self.provenance.clone(),
            coverage: None,
            readings: BTreeMap::new(),
            agreement: BTreeMap::new(),
            calls: Vec::new(),
        };
        let tables = match &self.tables {
            Ok(tables) => tables,
            Err(reason) => {
                reading.unavailable = Some(reason.0.clone());
                return reading;
            }
        };
        reading.status = "available";
        let mut coverage = tables.coverage.clone();
        let mut missing = BTreeSet::new();
        let mut lines = HashMap::new();
        for (caller, call, qualified, function) in calls {
            let target = qualified.as_ref().and_then(|(_, target)| *target);
            let semantic =
                tables.reading_of(call, function, target, text, (&mut missing, &mut lines));
            *reading
                .readings
                .entry(kind(&semantic).to_owned())
                .or_default() += 1;
            let (qualified, agreement) = match qualified {
                Some((qualified, target)) => {
                    let agreement = compare(target, &semantic);
                    *reading.agreement.entry(agreement).or_default() += 1;
                    (Some(qualified), Some(agreement))
                }
                None => (None, None),
            };
            reading.calls.push(CallReading {
                caller: caller.to_owned(),
                site: call.site.clone(),
                spelling: call.spelling.clone(),
                receiver: call.receiver.clone(),
                qualified,
                semantic,
                agreement,
                caller_site: function
                    .map(|function| (function.flow.site.path.clone(), function.flow.site.line)),
            });
        }
        coverage.files_not_indexed = missing.into_iter().collect();
        reading.coverage = Some(coverage);
        reading
    }
}

fn kind(semantic: &Semantic) -> &'static str {
    match semantic {
        Semantic::Definition { .. } => "definition",
        Semantic::TraitMethod { .. } => "trait_method",
        Semantic::NoDefinition { .. } => "no_definition",
        Semantic::External { .. } => "external",
        Semantic::Local => "local",
        Semantic::Ambiguous { .. } => "ambiguous",
        Semantic::Mismatch { .. } => "mismatch",
        Semantic::NotIndexed { .. } => "not_indexed",
    }
}

/// Compare by site, never by symbol. `target` is the qualified function the
/// call resolved to, if it resolved to one.
fn compare(target: Option<&crate::analysis::index::Function>, semantic: &Semantic) -> Agreement {
    match semantic {
        Semantic::Ambiguous { .. } => return Agreement::Ambiguous,
        Semantic::NotIndexed { .. } | Semantic::NoDefinition { .. } | Semantic::Mismatch { .. } => {
            return Agreement::Silent;
        }
        _ => {}
    }
    let Some(target) = target else {
        return Agreement::Extends;
    };
    let same = match semantic {
        Semantic::Definition { site, .. } => {
            target.flow.site.path == site.path
                && (target.flow.site.line..=target.flow.site.end_line).contains(&site.line)
        }
        _ => false,
    };
    if same {
        Agreement::Agrees
    } else {
        Agreement::Disagrees
    }
}

impl Tables {
    fn new(index: &wire::Index) -> Result<Self, Unusable> {
        if index.documents.is_empty() {
            return Err("the index has no documents".into());
        }
        let mut tables = Self {
            references: HashMap::new(),
            definitions: HashMap::new(),
            kinds: HashMap::new(),
            packages: BTreeSet::new(),
            documents: BTreeSet::new(),
            texts: HashMap::new(),
            coverage: Coverage {
                documents: index.documents.len(),
                ..Coverage::default()
            },
        };
        tables.read_documents(index)?;
        if !tables.read_occurrences(index) {
            return Err("the index names no core, alloc or std symbol; an indexer that cannot load the standard library (rust-src) writes such an index, so its external readings cannot be trusted".into());
        }
        tables.coverage.sysroot_observed = true;
        tables.coverage.symbols_with_several_definitions = tables
            .definitions
            .iter()
            .filter(|(symbol, sites)| sites.len() > 1 && !symbol.starts_with("local "))
            .count();
        Ok(tables)
    }
    /// Each document once, in UTF-8 positions: its symbols' kinds and text.
    fn read_documents(&mut self, index: &wire::Index) -> Result<(), Unusable> {
        let tables = self;
        for document in &index.documents {
            if !tables.documents.insert(document.relative_path.clone()) {
                return Err(format!("document {} appears twice", document.relative_path).into());
            }
            if document.position_encoding != wire::UTF8_FROM_LINE_START {
                return Err(format!(
                    "document {} uses position encoding {}; Atlas reads only UTF-8 code units from line start (1)",
                    document.relative_path, document.position_encoding
                )
                .into());
            }
            for symbol in &document.symbols {
                tables.kinds.insert(symbol.symbol.clone(), symbol.kind);
            }
            if let Some(text) = &document.text {
                tables
                    .texts
                    .insert(document.relative_path.clone(), text.clone());
            }
        }
        Ok(())
    }
    /// Each occurrence as a definition or a reference; whether any names a
    /// standard-library symbol.
    fn read_occurrences(&mut self, index: &wire::Index) -> bool {
        let tables = self;
        let mut sysroot = false;
        for document in &index.documents {
            for o in &document.occurrences {
                sysroot |= matches!(
                    package(&o.symbol).map(|p| p.1),
                    Some("core" | "alloc" | "std")
                );
                if o.roles & wire::DEFINITION != 0 {
                    if let Some((key, ..)) = package(&o.symbol) {
                        tables.packages.insert(key.to_owned());
                    }
                    tables
                        .definitions
                        .entry(o.symbol.clone())
                        .or_default()
                        .push(DefinitionSite {
                            path: document.relative_path.clone(),
                            line: o.range[0] as usize + 1,
                        });
                } else {
                    tables
                        .references
                        .entry((document.relative_path.clone(), o.range[0], o.range[1]))
                        .or_default()
                        .push(([o.range[2], o.range[3]], o.symbol.clone()));
                }
            }
        }
        sysroot
    }

    /// One call's semantic reading, in the scope of the function that makes
    /// it. A name the qualified reading followed to the very definition the
    /// index names (through a re-export chain the name check cannot see,
    /// `crate::{synchronize_render}` for `synchronize`) is the same symbol,
    /// not a stale one.
    fn reading_of<'a>(
        &self,
        call: &Call,
        function: Option<&crate::analysis::index::Function>,
        target: Option<&crate::analysis::index::Function>,
        text: &dyn Fn(&str) -> Option<&'a str>,
        (missing, lines): (&mut BTreeSet<String>, &mut HashMap<String, locate::Lines>),
    ) -> Semantic {
        // Whether `written` may stand for a symbol named `named`: an import
        // of that name in scope brings it in (`use crate::base as label`),
        // or a body-local `use` or item rebinds the name here.
        let renames = |written: &str, named: &str| {
            function.is_some_and(|function| {
                function
                    .flow
                    .local_names
                    .iter()
                    .any(|local| local == written)
                    || function.flow.imports.get(written).is_some_and(|paths| {
                        paths
                            .iter()
                            .any(|path| path.last().is_some_and(|last| last == named))
                    })
            })
        };
        let semantic = self.call(call, text, &renames, missing, lines);
        if let Semantic::Mismatch { symbol } = &semantic
            && target.is_some()
        {
            let classified = self.classify(symbol);
            if compare(target, &classified) == Agreement::Agrees {
                return classified;
            }
        }
        semantic
    }

    fn call<'a>(
        &self,
        call: &Call,
        text: &dyn Fn(&str) -> Option<&'a str>,
        renames: &dyn Fn(&str, &str) -> bool,
        missing: &mut BTreeSet<String>,
        lines: &mut HashMap<String, locate::Lines>,
    ) -> Semantic {
        let not_indexed = |reason: &str| Semantic::NotIndexed {
            reason: reason.to_owned(),
        };
        if !self.documents.contains(&call.site.path) {
            missing.insert(call.site.path.clone());
            return not_indexed("the file is not in the index");
        }
        let Some(source) = text(&call.site.path) else {
            return not_indexed("no captured source for the file");
        };
        if self
            .texts
            .get(&call.site.path)
            .is_some_and(|indexed| indexed != source)
        {
            return not_indexed("index is for other source");
        }
        let lines = lines
            .entry(call.site.path.clone())
            .or_insert_with(|| locate::Lines::of(source));
        let Some(callee) = locate::callee(source, lines, call) else {
            return not_indexed("the callee name is not written at the call site");
        };
        let key = (call.site.path.clone(), callee.line, callee.column);
        let Some(found) = self.references.get(&key) else {
            return not_indexed("no index occurrence at the callee name");
        };
        let width = u32::try_from(callee.name.len()).unwrap_or(u32::MAX);
        let expected = [callee.line, callee.column.saturating_add(width)];
        let symbols: BTreeSet<&str> = found
            .iter()
            .filter(|(end, _)| *end == expected)
            .map(|(_, symbol)| symbol.as_str())
            .collect();
        let symbol = match symbols.len() {
            0 => return not_indexed("index text differs from captured source"),
            1 => symbols.into_iter().next().unwrap_or_default(),
            n => {
                return Semantic::Ambiguous {
                    reason: format!(
                        "{n} index occurrences at the callee name name different symbols"
                    ),
                };
            }
        };
        // An occurrence at the right place can still be another name, when
        // the index was made from other source: never read it as an edge.
        let named = descriptor_name(symbol);
        // `Self` stands for the impl's type, whose symbol names the type.
        if !symbol.starts_with("local ")
            && callee.name != "Self"
            && named != Some(callee.name.as_str())
            && !named.is_some_and(|named| renames(&callee.name, named))
        {
            return Semantic::Mismatch {
                symbol: symbol.to_owned(),
            };
        }
        self.classify(symbol)
    }

    fn classify(&self, symbol: &str) -> Semantic {
        if symbol.starts_with("local ") {
            return Semantic::Local;
        }
        match self.definitions.get(symbol).map(Vec::as_slice) {
            Some([site]) => {
                let kind = self.kinds.get(symbol).copied().unwrap_or_default();
                if kind == TRAIT_METHOD {
                    Semantic::TraitMethod {
                        declaration: site.clone(),
                        symbol: symbol.to_owned(),
                    }
                } else {
                    Semantic::Definition {
                        site: site.clone(),
                        kind: kind_name(kind),
                        symbol: symbol.to_owned(),
                    }
                }
            }
            Some(sites) if sites.len() > 1 => Semantic::Ambiguous {
                reason: format!("the index defines this symbol at {} sites", sites.len()),
            },
            _ => match package(symbol) {
                Some((key, ..)) if self.packages.contains(key) => Semantic::NoDefinition {
                    symbol: symbol.to_owned(),
                },
                Some((_, name, version, item)) => Semantic::External {
                    package: name.to_owned(),
                    version: version.to_owned(),
                    item: item.to_owned(),
                },
                None => Semantic::NotIndexed {
                    reason: format!("unreadable symbol {symbol:?}"),
                },
            },
        }
    }
}

const TRAIT_METHOD: u64 = 70;

/// The name a global symbol's last descriptor gives: `graph` for
/// `.../Snapshot#graph().`, `Snapshot` for `.../Snapshot#`, without a
/// disambiguator such as `(+1)` or backtick escapes.
fn descriptor_name(symbol: &str) -> Option<&str> {
    let (_, _, _, descriptors) = package(symbol)?;
    let trimmed = descriptors.trim_end_matches(['.', '#', '/', ':', '!']);
    let trimmed = trimmed
        .strip_suffix(')')
        .and_then(|rest| rest.rfind('(').map(|open| &rest[..open]))
        .unwrap_or(trimmed);
    let last = trimmed
        .rsplit(['/', '#', '.', '[', ']'])
        .next()
        .unwrap_or(trimmed);
    Some(last.trim_matches('`'))
}

fn kind_name(kind: u64) -> String {
    match kind {
        9 => "constructor".into(),
        12 => "enum member".into(),
        17 => "function".into(),
        25 => "macro".into(),
        26 => "method".into(),
        49 => "struct".into(),
        80 => "static method".into(),
        0 => "unspecified".into(),
        other => format!("SCIP kind {other}"),
    }
}

/// A global symbol's `manager package version` key, then package name,
/// version and descriptors: `<scheme> <manager> <package> <version> <descriptors>`.
fn package(symbol: &str) -> Option<(&str, &str, &str, &str)> {
    let mut parts = symbol.splitn(5, ' ');
    let scheme = parts.next()?;
    if scheme == "local" {
        return None;
    }
    let manager = parts.next()?;
    let name = parts.next()?;
    let version = parts.next()?;
    let item = parts.next()?;
    let key_start = scheme.len() + 1;
    let key = &symbol[key_start..key_start + manager.len() + name.len() + version.len() + 2];
    Some((key, name, version, item))
}
