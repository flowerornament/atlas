//! The `name` selector grammar, shared by both report populations.
//!
//! A selector is `NAME`, `MODULE::…::NAME` or a source site `FILE:LINE[:COLUMN]`.
//! The modules match the end of a declaration's written module path: the
//! file's directories and stem (`mod`, `lib` and `main` add nothing), then the
//! inline `mod` blocks that enclose it. This is a lexical spelling, not Rust
//! path resolution: re-exports and `use` aliases do not move a declaration.

pub(crate) enum Selector<'a> {
    Path {
        modules: Vec<&'a str>,
        name: &'a str,
    },
    Site {
        file: &'a str,
        line: usize,
        column: Option<usize>,
    },
}

impl<'a> Selector<'a> {
    pub(crate) fn parse(query: &'a str) -> crate::Result<Self> {
        if !query.contains("::")
            && let Some(site) = site(query)
        {
            return Ok(site);
        }
        let mut segments: Vec<&str> = query.split("::").collect();
        let name = segments.pop().unwrap_or_default();
        if name.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
            return Err(format!(
                "name selector {query:?}: expected NAME, MODULE::NAME or FILE:LINE"
            )
            .into());
        }
        Ok(Self::Path {
            modules: segments,
            name,
        })
    }

    /// The identifier a path selector names; a site selector names none.
    pub(crate) fn name(&self) -> Option<&'a str> {
        match self {
            Self::Path { name, .. } => Some(name),
            Self::Site { .. } => None,
        }
    }

    /// Whether a declaration named `name` at `file:line:column`, inside the
    /// inline modules `inline` returns (outermost first), is selected.
    /// `inline` runs only for a declaration whose name matches.
    pub(crate) fn admits<'s>(
        &self,
        name: &str,
        file: &'s str,
        line: usize,
        column: Option<usize>,
        inline: impl FnOnce() -> Vec<&'s str>,
    ) -> bool {
        match self {
            Self::Path {
                modules,
                name: wanted,
            } => {
                if name != *wanted {
                    return false;
                }
                let path: Vec<&str> = file_modules(file).chain(inline()).collect();
                path.ends_with(modules)
            }
            Self::Site {
                file: wanted,
                line: at,
                column: col,
            } => {
                file == *wanted
                    && line == *at
                    && col.is_none_or(|col| column.is_none_or(|column| column == col))
            }
        }
    }
}

fn site(query: &str) -> Option<Selector<'_>> {
    let mut parts = query.rsplitn(3, ':');
    let last: usize = parts.next()?.parse().ok()?;
    let middle = parts.next()?;
    match (middle.parse::<usize>(), parts.next()) {
        (Ok(line), Some(file)) if !file.is_empty() => Some(Selector::Site {
            file,
            line,
            column: Some(last),
        }),
        _ if !middle.is_empty() => Some(Selector::Site {
            file: query.rsplit_once(':')?.0,
            line: last,
            column: None,
        }),
        _ => None,
    }
}

/// A file's directories and stem, without the stems that add no module.
fn file_modules(file: &str) -> impl Iterator<Item = &str> {
    let mut parts: Vec<&str> = file.split('/').collect();
    if let Some(last) = parts.last_mut() {
        *last = last.strip_suffix(".rs").unwrap_or(last);
    }
    if parts
        .last()
        .is_some_and(|stem| matches!(*stem, "mod" | "lib" | "main"))
    {
        parts.pop();
    }
    parts.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_match_the_end_of_the_module_path() {
        let s = Selector::parse("parse::Tree").expect("selector");
        assert!(s.admits("Tree", "src/parse.rs", 4, None, Vec::new));
        assert!(s.admits("Tree", "src/parse/mod.rs", 4, None, Vec::new));
        assert!(!s.admits("Tree", "src/check.rs", 4, None, Vec::new));
        let s = Selector::parse("duplicate::a::repeated").expect("selector");
        assert!(s.admits("repeated", "src/lib.rs", 7, None, || vec!["duplicate", "a"]));
        assert!(!s.admits("repeated", "src/lib.rs", 7, None, || vec!["duplicate", "b"]));
    }

    #[test]
    fn sites_and_bad_selectors() {
        let s = Selector::parse("src/s1.rs:1:11").expect("selector");
        assert!(s.admits("x", "src/s1.rs", 1, Some(11), Vec::new));
        assert!(!s.admits("x", "src/s1.rs", 1, Some(12), Vec::new));
        let s = Selector::parse("src/s1.rs:3").expect("selector");
        assert!(s.admits("x", "src/s1.rs", 3, Some(5), Vec::new));
        for bad in ["", "a::", "::a", "a::::b"] {
            assert!(Selector::parse(bad).is_err(), "{bad:?}");
        }
    }
}
