//! Native schema-1 policy data; project authority references remain opaque.
mod configuration;
mod observations;
pub use observations::{observe, observe_with_helper};
mod validation;
pub use validation::{
    Inputs, Match, MissingInput, PathState, PatternState, ReferenceResolution, Report, Resolution,
    Status, validate,
};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Policy {
    pub schema: u32,
    pub provenance: String,
    pub population: CorePopulation,
    pub size: SizeCeilings,
    pub surface: BTreeMap<String, usize>,
    pub crate_graph: CrateGraphPolicy,
    pub authority: Vec<AuthorityRule>,
    /// Crates whose written production types must not return String errors.
    #[serde(default)]
    pub typed_errors: Vec<String>,
    #[serde(default)]
    pub stage_order: Vec<StageOrder>,
    /// Per crate, per stage: the earlier stages it may read.
    #[serde(default)]
    pub stage_reads: BTreeMap<String, BTreeMap<String, StageReads>>,
    /// Per crate whose stages meet at item-level seams: the floor every
    /// stage reads whole, and each test owner's coupling count.
    #[serde(default)]
    pub stage_seams: BTreeMap<String, StageSeams>,
    /// Absent only in a published policy older than the concentration gate.
    #[serde(default)]
    pub concentration: Option<ConcentrationPolicy>,
}

/// One crate's top-level modules in pipeline order, upstream first. `atlas
/// stages` reports against it, and `cargo atlas delta` refuses a candidate whose
/// crate holds a forward edge, an unjudged path or an unnamed module.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct StageOrder {
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub stages: Vec<String>,
    /// The function that runs the stages, and the calls it must make.
    #[serde(default)]
    pub pipeline: Option<Pipeline>,
}

/// The function that runs a crate's stages: straight-line, making exactly
/// `calls` as written; `loops` allows one `for` around them.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Pipeline {
    pub file: String,
    pub function: String,
    pub calls: Vec<String>,
    #[serde(default)]
    pub loops: bool,
}

/// One stage's interface: the earlier stages it reads. `reads` are stages
/// it reads whole; `input` are stages whose boundary items (what the stage's
/// root module declares or re-exports) it reads; `debt` are reads past both,
/// each row counting the distinct items it may name and the bead that
/// retires it. A read in none of them is a new coupling, and `cargo atlas delta`
/// refuses it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct StageReads {
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub debt: StageDebt,
}

/// A stage's debt rows. The published policy before item-level seams named
/// debt stages bare; `cargo atlas delta` still reads that baseline, and the
/// candidate's own policy must use rows.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum StageDebt {
    Rows(BTreeMap<String, DebtRow>),
    Bare(Vec<String>),
}

impl Default for StageDebt {
    fn default() -> Self {
        Self::Rows(BTreeMap::new())
    }
}

impl StageDebt {
    /// The stages owed, in either form.
    pub fn stages(&self) -> Vec<&str> {
        match self {
            Self::Rows(rows) => rows.keys().map(String::as_str).collect(),
            Self::Bare(stages) => stages.iter().map(String::as_str).collect(),
        }
    }

    /// Test membership without allocating the list of all owed stages.
    pub fn contains(&self, stage: &str) -> bool {
        match self {
            Self::Rows(rows) => rows.contains_key(stage),
            Self::Bare(stages) => stages.iter().any(|named| named == stage),
        }
    }

    /// The row for one stage, when the debt is written as rows.
    pub fn row(&self, stage: &str) -> Option<&DebtRow> {
        match self {
            Self::Rows(rows) => rows.get(stage),
            Self::Bare(_) => None,
        }
    }
}

/// One debt row: a count of distinct interior items (those of one earlier
/// stage that a stage reads past that stage's boundary, or a stage root's
/// exports outside its boundary list) and the bead whose work retires them.
/// The count only goes down.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DebtRow {
    pub items: usize,
    pub owner: String,
}

impl StageReads {
    /// Whether the row names `stage` at all, given the crate's floor.
    pub fn admits(&self, stage: &str, floor: &[String]) -> bool {
        floor
            .iter()
            .chain(&self.reads)
            .chain(&self.input)
            .any(|read| read == stage)
            || self.debt.contains(stage)
    }
}

/// One crate's item-level seams: `floor` are the shared stages every stage
/// reads whole (the shared names and rules); `exports` counts, per stage, the
/// root exports its header's boundary list does not name (interior, so they
/// only fall); and `test_coupling` holds, per test owner (a stage, or a
/// crate-level test module), how far its tests reach into other stages'
/// interiors.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct StageSeams {
    pub floor: Vec<String>,
    /// Stages no other stage reads, except the one that runs them all.
    #[serde(default)]
    pub final_stages: Vec<String>,
    #[serde(default)]
    pub exports: BTreeMap<String, DebtRow>,
    #[serde(default)]
    pub writers: BTreeMap<String, Writers>,
    #[serde(default)]
    pub test_coupling: BTreeMap<String, TestCoupling>,
}

/// Who may build or change one boundary type that later stages read whole
/// (a type listed whole, or a floor type). Its writer methods are derived
/// from the type itself (each `impl` method in `stage` taking `&mut self` or
/// returning the type) plus any `methods` named here; they and the writer
/// `functions` belong to its `producers`. For a producer the functions are
/// boundary items; any other stage or test owner calling either is hidden
/// debt, counted per owner in `sites`, which only falls. `reason` says why the
/// type is read whole, and `owner` is the bead that narrows it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Writers {
    pub stage: String,
    pub reason: String,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
    pub producers: Vec<String>,
    pub owner: String,
    #[serde(default)]
    pub sites: BTreeMap<String, usize>,
    /// Writer call sites outside the producing stage in test code, per
    /// owner; `sites` counts both populations together.
    #[serde(default)]
    pub test_sites: BTreeMap<String, usize>,
    /// The same, in production code.
    #[serde(default)]
    pub production_sites: BTreeMap<String, usize>,
}

/// One test owner's coupling: the distinct (test file, item) pairs its tests
/// name inside other stages, past their boundaries, and the bead whose work
/// retires them. The count only goes down.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TestCoupling {
    pub pairs: usize,
    pub owner: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CorePopulation {
    pub core: Vec<String>,
    /// The citation that lets a crate leave `core`, as concentration's.
    #[serde(default)]
    pub amended_by: Option<Amendment>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SizeCeilings {
    #[serde(default)]
    pub core_token_lines: usize,
    #[serde(rename = "crate", default)]
    pub crates: BTreeMap<String, usize>,
    /// No stored ceilings: each crate's growth is reported for the host to
    /// approve.
    #[serde(default)]
    pub approve_growth: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CrateGraphPolicy {
    #[serde(default)]
    pub vendored: Vec<String>,
    #[serde(default)]
    pub terminal: Vec<String>,
    #[serde(default)]
    pub transparent: Vec<String>,
    #[serde(default)]
    pub forbidden_members: Vec<String>,
    pub rule: Vec<CrateRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CrateRule {
    pub name: String,
    /// The crates this one may depend on; `"any"` (or absent, for now) for
    /// no limit.
    #[serde(default, deserialize_with = "allowed_crates")]
    pub allowed: Option<Vec<String>>,
    pub law: Option<String>,
}

/// `allowed = ["a", "b"]`, or `allowed = "any"`: no limit, which an absent
/// `allowed` also means until it is refused (0.21's crate-graph item).
fn allowed_crates<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        Crates(Vec<String>),
        Word(String),
    }
    match Written::deserialize(deserializer)? {
        Written::Crates(crates) => Ok(Some(crates)),
        Written::Word(word) if word == "any" => Ok(None),
        Written::Word(word) => Err(serde::de::Error::custom(format!(
            "allowed must be a list of crates or \"any\", not \"{word}\""
        ))),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct AuthorityRule {
    pub fact: String,
    pub home: String,
    #[serde(default)]
    pub forbid_item: Vec<String>,
    #[serde(default)]
    pub forbid_path: Vec<String>,
    #[serde(default)]
    pub forbid_pattern: Vec<String>,
    /// The only files whose production code may read the home, each with
    /// its exact count of reads; `[]` for none.
    #[serde(default)]
    pub allowed_readers: Option<Vec<Reader>>,
    /// Spellings (ast-grep patterns) that may not appear anew or grow
    /// outside the home; existing ones are counted from the baseline.
    #[serde(default)]
    pub forbid_new_pattern: Vec<NamedPattern>,
    /// The same, for spellings the host observes and supplies per item.
    #[serde(default)]
    pub forbid_new_observed: Vec<String>,
    /// Items that may hold up to these counts of a `forbid_new` spelling.
    #[serde(default)]
    pub allowed_in: Vec<Allowance>,
}

/// A file allowed to read an authority's home, and how many times.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Reader {
    pub file: String,
    pub reads: usize,
    #[serde(default)]
    pub owner: Option<String>,
    pub reason: String,
}

/// A named spelling, as an ast-grep pattern.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct NamedPattern {
    pub name: String,
    pub pattern: String,
}

/// An item allowed to hold a `forbid_new` spelling: `counts` maps a path
/// relative to the item (`""` for the item itself) to each spelling's
/// largest count there.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Allowance {
    pub file: String,
    pub item: String,
    pub counts: BTreeMap<String, BTreeMap<String, usize>>,
    #[serde(default)]
    pub owner: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ConcentrationPolicy {
    pub population: Vec<String>,
    pub entry_lines: usize,
    pub file_lines: usize,
    /// Names every crate may reuse (`error`, `tests`): never a ruling.
    pub conventional_names: Vec<String>,
    #[serde(default)]
    pub entry: Vec<LineRow>,
    #[serde(default)]
    pub file: Vec<LineRow>,
    #[serde(default)]
    pub name: Vec<NameRow>,
    #[serde(default)]
    pub alias: Vec<Alias>,
    /// Per population crate, its folder roots whose header cites nothing.
    #[serde(default)]
    pub header_uncited: BTreeMap<String, usize>,
    /// The ruling behind this landing's loosening of the section, and the
    /// knobs it covers.
    #[serde(default)]
    pub amended_by: Option<Amendment>,
    /// The longest a function body may be, in code lines.
    #[serde(default)]
    pub function_lines: Option<usize>,
    /// The deepest a function may nest (clippy's excessive_nesting).
    #[serde(default)]
    pub function_nesting: Option<usize>,
    /// Paths whose functions are not measured.
    #[serde(default)]
    pub generated: Vec<Generated>,
    /// Functions allowed over a function limit, at their measure.
    #[serde(default)]
    pub function: Vec<FunctionRow>,
}

/// A path (file or directory) whose functions are not measured, and why.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Generated {
    pub path: String,
    pub reason: String,
}

/// One function held above a function limit at its measure. `function` is
/// written as `callers` takes it (`Type::method`, `module::f`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct FunctionRow {
    pub file: String,
    pub function: String,
    #[serde(default)]
    pub lines: Option<usize>,
    #[serde(default)]
    pub nesting: Option<usize>,
    #[serde(default)]
    pub owner: Option<String>,
}

/// A bead that rules a loosening, and exactly what it loosens.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Amendment {
    pub bead: String,
    pub covers: BTreeSet<Knob>,
}

/// A knob of the section that a loosening moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Knob {
    EntryLines,
    FileLines,
    Population,
    ConventionalNames,
}

/// One file held above a limit at its measured body lines.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct LineRow {
    pub path: String,
    pub lines: usize,
    /// The reference that answers for the row.
    #[serde(default)]
    pub owner: Option<String>,
}

/// One module name with several homes, and what the repetition means.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct NameRow {
    pub name: String,
    pub ruling: Ruling,
    pub why: String,
    pub homes: BTreeSet<String>,
    /// The bead that re-ruled a published collision a facet.
    #[serde(default)]
    pub reruled_by: Option<String>,
}

/// Whether a repeated module name holds one meaning or two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Ruling {
    /// One concept seen from several stages or crates; declared, kept.
    Facet,
    /// Two concepts under one name; debt, owed a rename, never a new home.
    Collision,
}

/// One re-export that gives an item a second home or a second name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Alias {
    /// The module that holds the re-export (`demo-compiler`, its root, or
    /// `demo-session::workflow::status`).
    pub module: String,
    pub kind: AliasKind,
    /// The name the re-export binds.
    pub binding: String,
    /// The path it re-exports, as written.
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AliasKind {
    /// `pub use stage::sub;` at the crate root re-homes a nested module.
    ModuleRelay,
    /// A crate-root re-export named like a top-level module it is not.
    RootShadow,
    /// A `pub(crate)` (or narrower) `use` at the crate root: an internal alias.
    RootRestricted,
    /// `pub use path::X as Y;`: one item under two names.
    Rename,
}

impl Policy {
    /// Decode the native authored shape, without claiming project authority was resolved.
    fn validate_header(&self) -> crate::Result<()> {
        if self.schema != 1 || self.provenance != "authored" {
            return Err(
                "architecture policy requires schema = 1 and provenance = 'authored'".into(),
            );
        }
        Ok(())
    }
    pub fn parse(text: &str) -> crate::Result<Self> {
        let policy: Self = toml::from_str(text)?;
        policy.validate_header()?;
        Ok(policy)
    }
}

/// A written project authority and its exact authored field address.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct Reference {
    pub field: Vec<String>,
    pub written: String,
}
impl Policy {
    /// The project wrapper must resolve these; parsing them never grants authority.
    pub fn external_references(&self) -> Vec<Reference> {
        let mut out = Vec::new();
        let mut add = |field: Vec<String>, written: &str| {
            out.push(Reference {
                field,
                written: written.to_owned(),
            });
        };
        for (package, rows) in &self.stage_reads {
            for (reader, row) in rows {
                if let StageDebt::Rows(debt) = &row.debt {
                    for (read, row) in debt {
                        add(
                            vec![
                                "stage_reads".into(),
                                package.clone(),
                                reader.clone(),
                                "debt".into(),
                                read.clone(),
                                "owner".into(),
                            ],
                            &row.owner,
                        );
                    }
                }
            }
        }
        for (package, seams) in &self.stage_seams {
            for (stage, row) in &seams.exports {
                add(
                    vec![
                        "stage_seams".into(),
                        package.clone(),
                        "exports".into(),
                        stage.clone(),
                        "owner".into(),
                    ],
                    &row.owner,
                );
            }
            for (name, row) in &seams.writers {
                add(
                    vec![
                        "stage_seams".into(),
                        package.clone(),
                        "writers".into(),
                        name.clone(),
                        "owner".into(),
                    ],
                    &row.owner,
                );
            }
            for (owner, row) in &seams.test_coupling {
                add(
                    vec![
                        "stage_seams".into(),
                        package.clone(),
                        "test_coupling".into(),
                        owner.clone(),
                        "owner".into(),
                    ],
                    &row.owner,
                );
            }
        }
        if let Some(policy) = &self.concentration {
            if let Some(amendment) = &policy.amended_by {
                add(
                    vec!["concentration".into(), "amended_by".into(), "bead".into()],
                    &amendment.bead,
                );
            }
            for (index, row) in policy.name.iter().enumerate() {
                if let Some(bead) = &row.reruled_by {
                    add(
                        vec![
                            "concentration".into(),
                            "name".into(),
                            index.to_string(),
                            "reruled_by".into(),
                        ],
                        bead,
                    );
                }
            }
        }
        out
    }
}

impl Policy {
    /// The fields this policy writes whose rule this build of Atlas reads
    /// but does not judge yet, by address. A check over such a policy is
    /// unavailable, never a complete PASS.
    pub fn unjudged(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.size.approve_growth {
            out.push("size.approve_growth".to_owned());
        }
        if self.population.amended_by.is_some() {
            out.push("population.amended_by".to_owned());
        }
        if let Some(policy) = &self.concentration {
            for (written, field) in [
                (policy.function_lines.is_some(), "function_lines"),
                (policy.function_nesting.is_some(), "function_nesting"),
                (!policy.generated.is_empty(), "generated"),
                (!policy.function.is_empty(), "function"),
            ] {
                if written {
                    out.push(format!("concentration.{field}"));
                }
            }
            for (rows, name) in [(&policy.entry, "entry"), (&policy.file, "file")] {
                for (index, row) in rows.iter().enumerate() {
                    if row.owner.is_some() {
                        out.push(format!("concentration.{name}.{index}.owner"));
                    }
                }
            }
        }
        for (index, rule) in self.authority.iter().enumerate() {
            for (written, field) in [
                (rule.allowed_readers.is_some(), "allowed_readers"),
                (!rule.forbid_new_pattern.is_empty(), "forbid_new_pattern"),
                (!rule.forbid_new_observed.is_empty(), "forbid_new_observed"),
                (!rule.allowed_in.is_empty(), "allowed_in"),
            ] {
                if written {
                    out.push(format!("authority.{index}.{field}"));
                }
            }
        }
        for order in &self.stage_order {
            if order.pipeline.is_some() {
                out.push(format!("stage_order.{}.pipeline", order.crate_name));
            }
        }
        for (package, seams) in &self.stage_seams {
            if !seams.final_stages.is_empty() {
                out.push(format!("stage_seams.{package}.final_stages"));
            }
            for (name, writers) in &seams.writers {
                for (written, field) in [
                    (!writers.test_sites.is_empty(), "test_sites"),
                    (!writers.production_sites.is_empty(), "production_sites"),
                ] {
                    if written {
                        out.push(format!("stage_seams.{package}.writers.{name}.{field}"));
                    }
                }
            }
        }
        out
    }
    /// Project authority required by selected item-level seams.
    pub fn seam_references(&self, package: Option<&str>) -> Vec<Reference> {
        self.stage_references(package)
            .into_iter()
            .filter(|reference| {
                reference
                    .field
                    .get(1)
                    .is_some_and(|name| self.stage_seams.contains_key(name))
            })
            .collect()
    }
    /// Project authority used by this stage question, selected by package.
    pub fn stage_references(&self, package: Option<&str>) -> Vec<Reference> {
        self.external_references()
            .into_iter()
            .filter(|reference| {
                matches!(
                    reference.field.first().map(String::as_str),
                    Some("stage_reads" | "stage_seams")
                ) && package.is_none_or(|package| {
                    reference.field.get(1).is_some_and(|name| name == package)
                })
            })
            .collect()
    }
}

/// Stage declarations in the published baseline, for monotone seam ratchets.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct PublishedStages {
    pub orders: Vec<StageOrder>,
    pub reads: BTreeMap<String, BTreeMap<String, StageReads>>,
    pub seams: BTreeMap<String, StageSeams>,
}
impl From<&Policy> for PublishedStages {
    fn from(policy: &Policy) -> Self {
        Self {
            orders: policy.stage_order.clone(),
            reads: policy.stage_reads.clone(),
            seams: policy.stage_seams.clone(),
        }
    }
}

/// Split a native PATH::ITEM or ROOT::PATTERN field without interpreting it.
pub(super) fn anchor<'a>(value: &'a str, field: &str) -> crate::Result<(&'a str, &'a str)> {
    let (path, item) = value
        .split_once("::")
        .ok_or_else(|| format!("policy: {field} `{value}` must be PATH::ITEM"))?;
    if path.trim().is_empty() || item.trim().is_empty() {
        return Err(format!("policy: {field} `{value}` must be PATH::ITEM").into());
    }
    Ok((path, item))
}
