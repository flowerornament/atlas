//! The two re-export routes plain path resolution does not follow, for
//! readers that must see every way a path can reach an item: a glob
//! re-export (`pub use self::parse::*;`), stored under `*` where
//! `Index::exported` never looks, and a type alias (`type T =
//! crate::parse::Tree;`). Kept apart from `exported`, which `callers`
//! shares, until following globs there is measured.
use super::{Index, resolve::Resolved};

/// How far alias and glob chains are followed; a longer chain is a cycle
/// or an unusual layout, read as unresolved.
const CHAIN: usize = 8;

impl Index {
    /// `path`, already resolved and exported, followed through glob
    /// re-exports and type aliases to what it names. `None` when a glob
    /// door or an alias is ambiguous, or the chain does not end.
    pub(crate) fn through_doors(&self, path: Vec<String>) -> Option<Vec<String>> {
        let mut path = path;
        for _ in 0..CHAIN {
            match self.alias_step(&path).or_else(|| self.glob_step(&path)) {
                None => return Some(path),
                Some(Resolved::Ambiguous) => return None,
                Some(Resolved::To(next)) => path = next,
            }
        }
        None
    }
    /// One step through a type alias that is a prefix of `path`.
    fn alias_step(&self, path: &[String]) -> Option<Resolved> {
        let (len, (module, alias)) = (1..=path.len())
            .rev()
            .find_map(|len| self.aliases.get(&path[..len]).map(|found| (len, found)))?;
        let target = alias.target.as_ref()?;
        Some(match self.in_scope(module, &alias.imports, target) {
            Some(mut target) => {
                target.extend_from_slice(&path[len..]);
                Resolved::To(target)
            }
            None => Resolved::Ambiguous,
        })
    }
    /// One step through a glob re-export: `path` is `M::name::...` where `M`
    /// re-exports globs and does not declare `name` itself.
    fn glob_step(&self, path: &[String]) -> Option<Resolved> {
        (1..path.len()).rev().find_map(|len| {
            let mut key = path[..len].to_vec();
            key.push("*".into());
            let sources = self.exports.get(&key)?;
            if self.declares_name(&path[..len], &path[len]) {
                return None;
            }
            let mut found = sources.iter().filter_map(|source| {
                let mut candidate = source.clone();
                candidate.extend_from_slice(&path[len..]);
                self.declares_name(source, &path[len]).then_some(candidate)
            });
            let first = found.next()?;
            Some(if found.next().is_some() {
                Resolved::Ambiguous
            } else {
                Resolved::To(self.exported(&first).unwrap_or(first))
            })
        })
    }
}
