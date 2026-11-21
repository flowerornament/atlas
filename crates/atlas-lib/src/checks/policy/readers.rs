//! `allowed_readers`: the only files whose production code may read an
//! authority's home, each with its exact count of reading sites.
//!
//! A home is an item (`FILE::ITEM`) or a module (a file, or a directory
//! for the module rooted there), resolved to one `crate::module` path
//! through the production index; its own files are not readers. A site
//! reads the home when it resolves into it in some production scope of
//! its file, unless what binds its first segment already leads into the
//! home (the import is the reading, not each use of it). A site Atlas
//! cannot pin, with a candidate inside the home, or a macro's spelled path
//! that reaches it, makes the rule unavailable, never a pass.
use super::{AuthorityRule, Reader, Report, homes::Home};
use crate::analysis::{
    Analysis,
    index::sites::{PathSite, SiteForm},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// What one authority's readers were found to be.
#[derive(Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ReaderReading {
    /// The home as a module path (`crate::front::parse`).
    pub home: String,
    /// The home's own files, which are not readers.
    pub home_files: Vec<String>,
    /// Each reading file's sites: line and written path.
    pub reads: BTreeMap<String, Vec<(usize, String)>>,
    /// Sites counted from macro arguments read as code (a syntax
    /// assumption).
    pub macro_arguments: usize,
}

/// Judge every authority that writes `allowed_readers` and whose home
/// resolved (`resolved` names their facts).
pub(super) fn judge(
    analysis: &Analysis<'_>,
    authorities: &[AuthorityRule],
    resolved: &BTreeSet<String>,
    report: &mut Report,
) -> crate::Result<()> {
    let wanted: Vec<&AuthorityRule> = authorities
        .iter()
        .filter(|a| a.allowed_readers.is_some() && resolved.contains(&a.fact))
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let reading = analysis.path_reading()?;
    for authority in wanted {
        let field = format!("authority.{}.allowed_readers", authority.fact);
        let home =
            match super::homes::resolve(analysis.snapshot(), &reading.scopes, &authority.home) {
                Ok(home) => home,
                Err(reason) => {
                    report.missing(field, reason.to_string());
                    continue;
                }
            };
        let found = read(&home, &reading.sites, &authority.home, &field, report);
        unreadable(analysis, &home, &authority.home, &field, report);
        let rows = authority.allowed_readers.as_deref().unwrap_or_default();
        judge_rows(
            &authority.fact,
            &authority.home,
            rows,
            &home,
            &found.reads,
            report,
        );
        report
            .authority_readers
            .insert(authority.fact.clone(), found);
    }
    Ok(())
}

/// The sites outside the home that read it, and the unavailable ones.
fn read(
    home: &Home,
    sites: &[PathSite],
    written_home: &str,
    field: &str,
    report: &mut Report,
) -> ReaderReading {
    let mut found = ReaderReading {
        home: home.path.join("::"),
        home_files: home.own_files().into_iter().collect(),
        ..ReaderReading::default()
    };
    let into = |path: &Vec<String>| path.starts_with(&home.path);
    let own = home.own_files();
    for site in sites.iter().filter(|site| !own.contains(&site.file)) {
        let reaches = site.targets.iter().flatten().any(into);
        let repeats = site.through.as_ref().is_some_and(into);
        let at = format!("{}:{}", site.file, site.line);
        if reaches && site.form == SiteForm::MacroText {
            report.missing(field, format!(
                "{at} spells `{}` in a macro's input, which reaches the home `{written_home}`; Atlas does not expand macros: write the path outside the macro, or as an argument expression",
                site.written
            ));
        } else if reaches && !repeats {
            found.macro_arguments += usize::from(site.form == SiteForm::MacroArgument);
            found
                .reads
                .entry(site.file.clone())
                .or_default()
                .push((site.line, site.written.clone()));
        } else if !reaches && site.candidates.iter().any(into) {
            report.missing(field, format!(
                "{at}: `{}` may read the home `{written_home}`, but its first name has competing bindings; write the full `crate::` path, import the names instead of a glob, or split the competing cfg bindings",
                site.written
            ));
        }
    }
    found
}

/// A production file Atlas cannot read whose text names the home's last
/// segment (a syntax error, or a `use` it cannot read) is unavailable.
fn unreadable(
    analysis: &Analysis<'_>,
    home: &Home,
    written_home: &str,
    field: &str,
    report: &mut Report,
) {
    let Some(name) = home.path.last() else {
        return;
    };
    let snapshot = analysis.snapshot();
    for file in snapshot.graph().files.iter().filter(|file| !file.test_only) {
        let text = snapshot.source(&file.path).unwrap_or_default();
        let unreadable_use = snapshot.source_facts(&file.path).is_some_and(|source| {
            source
                .imports()
                .any(|import| import.bindings.is_err() && import.statement.contains(name.as_str()))
        });
        if (snapshot.syntax_error(&file.path).is_some() && text.contains(name.as_str()))
            || unreadable_use
        {
            report.missing(field, format!(
                "`{}` names `{name}` in code Atlas cannot read, so its reads of the home `{written_home}` are unknown; fix the syntax Atlas reports there",
                file.path
            ));
        }
    }
}

/// The table: an unlisted reader, or more reads than its row, refuses; a
/// row its file has fallen below is a hint, or for an owned row a refusal;
/// a row with no reads, or naming a file of the home, refuses.
fn judge_rows(
    fact: &str,
    home: &str,
    rows: &[Reader],
    owned_home: &Home,
    reads: &BTreeMap<String, Vec<(usize, String)>>,
    report: &mut Report,
) {
    let listed: BTreeMap<&str, &Reader> = rows.iter().map(|row| (row.file.as_str(), row)).collect();
    for (file, sites) in reads {
        let n = sites.len();
        let lines = sites
            .iter()
            .map(|(line, _)| line.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        match listed.get(file.as_str()) {
            None => {
                let (line, written) = &sites[0];
                report.violations.push(format!(
                    "authority `{fact}`: {file}:{line} reads the home `{home}` through `{written}`; {file} is not an allowed reader; read it through a listed reader, or add {{ file = \"{file}\", reads = {n}, reason = \"...\" }}"
                ));
            }
            Some(row) if n > row.reads => report.violations.push(format!(
                "authority `{fact}`: {file} reads the home `{home}` at {n} sites ({lines}); its row allows {}; remove the new reads or set reads = {n}",
                row.reads
            )),
            Some(row) if n < row.reads => match &row.owner {
                Some(owner) => report.violations.push(format!(
                    "authority `{fact}`: {file} reads the home at {n} sites, below its row's {}; it is owned by `{owner}`, and an owned row must equal its measure: set reads = {n}",
                    row.reads
                )),
                None => report.hints.push(format!(
                    "authority `{fact}`: the row for {file} can ratchet to reads = {n} (row {})",
                    row.reads
                )),
            },
            Some(_) => {}
        }
    }
    let own = owned_home.own_files();
    for row in rows {
        if own.contains(&row.file) {
            report.violations.push(format!(
                "authority `{fact}`: allowed reader {} is a file of the home `{home}` itself; delete its row",
                row.file
            ));
        } else if !reads.contains_key(&row.file) {
            report.violations.push(format!(
                "authority `{fact}`: listed reader {} no longer reads the home `{home}`; delete its row (an unused allowance would allow a future read)",
                row.file
            ));
        }
    }
}

/// The shape of `allowed_readers`, before anything is read.
pub(super) fn validate(authority: &AuthorityRule) -> crate::Result<()> {
    let mut files = BTreeSet::new();
    for row in authority.allowed_readers.iter().flatten() {
        let at = format!(
            "policy: authority `{}` allowed reader `{}`",
            authority.fact, row.file
        );
        if !files.insert(row.file.as_str()) {
            return Err(format!("{at} is listed twice").into());
        }
        if row.reads == 0 {
            return Err(format!("{at} has reads = 0; delete the row instead").into());
        }
        if row.reason.trim().is_empty() {
            return Err(format!("{at} needs a reason").into());
        }
    }
    Ok(())
}
