//! Source outlines and native non-test line accounting over a retained snapshot.
use crate::{Result, architecture::contains, discovery::Snapshot};
use serde::Serialize;
pub mod moves;

#[derive(Debug, Serialize)]
pub struct ItemSpan {
    pub name: String,
    pub kind: String,
    pub declaration: usize,
    pub first: usize,
    pub last: usize,
}
/// Murail's native block-depth unit: declaration through next top-level declaration
/// or test region, rather than the attached syntactic closing brace.
#[derive(Debug, Serialize)]
pub struct Block {
    pub name: String,
    pub line: usize,
    pub lines: usize,
}
#[derive(Debug, Serialize)]
pub struct FileOutline {
    pub file: String,
    pub fingerprint: String,
    pub non_test_lines: usize,
    pub items: Vec<ItemSpan>,
    pub top_level_items: usize,
    pub largest_block: Option<Block>,
    /// Code outside all attached item spans; a nonempty list exposes outline omissions.
    pub uncovered_code: Vec<usize>,
}
#[derive(Debug, Serialize)]
pub struct Outline {
    pub schema: u32,
    pub scope: String,
    pub files: Vec<FileOutline>,
}

/// Every lexical item, including private imports and macro invocations.
/// No filesystem read occurs here: source and fingerprints come from the same snapshot.
pub fn outline(snapshot: &Snapshot, scope: &str) -> Result<Outline> {
    let scope = crate::discovery::resolve_scope(snapshot.graph(), Some(scope))?;
    let mut files = Vec::new();
    for file in snapshot
        .graph()
        .physical_files()
        .filter(|f| contains(&scope, &f.path))
    {
        let source = snapshot
            .source_facts(&file.path)
            .ok_or_else(|| format!("snapshot lacks source facts for {}", file.path))?;
        let projection = source.projection();
        let outline = source.outline();
        let regions = source.test_regions();
        let tops: Vec<_> = outline
            .iter()
            .filter(|i| i.kind.declares() && i.top_level)
            .collect();
        let top_level_items = tops
            .iter()
            .filter(|i| !file.test_only && !regions.covers(i.line))
            .count();
        let end = source.text().lines().count() + 1;
        let mut largest_block: Option<Block> = None;
        for (index, item) in tops.iter().enumerate() {
            let next = tops.get(index + 1).map_or(end, |i| i.line);
            let stop = next.min(regions.next_start_after(item.line).unwrap_or(end));
            let lines = stop.saturating_sub(item.line);
            if !file.test_only
                && !regions.covers(item.line)
                && largest_block.as_ref().is_none_or(|b| lines > b.lines)
            {
                largest_block = Some(Block {
                    name: item.name.clone(),
                    line: item.line,
                    lines,
                });
            }
        }
        let mut covered = vec![false; source.text().lines().count()];
        let mut items = Vec::new();
        for item in outline {
            for line in item.span.clone() {
                if let Some(mark) = covered.get_mut(line - 1) {
                    *mark = true;
                }
            }
            items.push(ItemSpan {
                name: item.name.clone(),
                kind: item.kind.tag().to_owned(),
                declaration: item.line,
                first: *item.span.start(),
                last: *item.span.end(),
            });
        }
        let uncovered_code = projection
            .code_lines()
            .enumerate()
            .filter_map(|(index, line)| {
                (!covered[index]
                    && !line.trim().is_empty()
                    && !line.trim_start().starts_with("#!["))
                .then_some(index + 1)
            })
            .collect();
        files.push(FileOutline {
            file: file.path.clone(),
            fingerprint: file.content_fingerprint.clone(),
            non_test_lines: if file.test_only {
                0
            } else {
                regions.non_test_loc()
            },
            items,
            uncovered_code,
            top_level_items,
            largest_block,
        });
    }
    if files.is_empty() {
        return Err(format!("no source files in scope `{scope}`").into());
    }
    Ok(Outline {
        schema: 1,
        scope,
        files,
    })
}

/// Native module-shape units; none denotes no top-level production block.
#[derive(Debug, Serialize)]
pub struct Shape {
    pub fingerprint: String,
    pub non_test_lines: usize,
    pub top_level_items: usize,
    pub largest_block: Option<Block>,
}
#[derive(Debug, Serialize)]
pub struct ShapeChange {
    pub file: String,
    pub before: Option<Shape>,
    pub after: Option<Shape>,
}
#[derive(Debug, Serialize)]
pub struct ShapeDiff {
    pub schema: u32,
    pub scope: String,
    pub changes: Vec<ShapeChange>,
}

fn production_lines<'a>(
    snapshot: &'a Snapshot,
    path: &str,
) -> Option<impl Iterator<Item = &'a str>> {
    let file = snapshot.physical_file(path)?;
    let source = snapshot.source_facts(path)?;
    if file.test_only {
        return None;
    }
    Some(
        source
            .text()
            .lines()
            .enumerate()
            .filter(move |(index, _)| !source.test_regions().covers(index + 1))
            .map(|(_, line)| line),
    )
}

fn scoped_shapes(snapshot: &Snapshot, scope: &str) -> Result<Vec<FileOutline>> {
    if snapshot
        .graph()
        .files
        .iter()
        .any(|file| contains(scope, &file.path))
    {
        Ok(outline(snapshot, scope)?.files)
    } else {
        Ok(Vec::new())
    }
}

/// Prefer current membership; only absence permits falling back to the old tree.
pub(crate) fn comparison_scope(before: &Snapshot, after: &Snapshot, scope: &str) -> Result<String> {
    match crate::discovery::resolve_scope(after.graph(), Some(scope)) {
        Ok(scope) => Ok(scope),
        Err(error)
            if matches!(
                error.downcast_ref::<crate::discovery::ScopeError>(),
                Some(crate::discovery::ScopeError::Absent { .. })
            ) =>
        {
            crate::discovery::resolve_scope(before.graph(), Some(scope))
        }
        Err(error) => Err(error),
    }
}

/// Compare production source and its shape, without treating a changed metric as a verdict.
/// Match files across both snapshots before applying a report scope, so removed scopes remain visible.
pub fn diff(before: &Snapshot, after: &Snapshot, scope: &str) -> Result<ShapeDiff> {
    let scope = comparison_scope(before, after, scope)?;
    let before_shapes = scoped_shapes(before, &scope)?;
    let after_shapes = scoped_shapes(after, &scope)?;
    let mut pairs: std::collections::BTreeMap<String, (Option<Shape>, Option<Shape>)> =
        std::collections::BTreeMap::new();
    for file in before_shapes {
        if contains(&scope, &file.file) {
            pairs.entry(file.file).or_default().0 = Some(Shape {
                fingerprint: file.fingerprint,
                non_test_lines: file.non_test_lines,
                top_level_items: file.top_level_items,
                largest_block: file.largest_block,
            });
        }
    }
    for file in after_shapes {
        if contains(&scope, &file.file) {
            pairs.entry(file.file).or_default().1 = Some(Shape {
                fingerprint: file.fingerprint,
                non_test_lines: file.non_test_lines,
                top_level_items: file.top_level_items,
                largest_block: file.largest_block,
            });
        }
    }
    let changes = pairs
        .into_iter()
        .filter_map(|(file, (left, right))| {
            let unchanged = match (
                production_lines(before, &file),
                production_lines(after, &file),
            ) {
                (Some(left), Some(right)) => left.eq(right),
                (None, None) => true,
                _ => false,
            };
            (!unchanged).then_some(ShapeChange {
                file,
                before: left,
                after: right,
            })
        })
        .collect();
    Ok(ShapeDiff {
        schema: 1,
        scope,
        changes,
    })
}
