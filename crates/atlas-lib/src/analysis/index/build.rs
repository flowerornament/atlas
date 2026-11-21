//! Construct correlated function/name tables before exposing the immutable index.
//! Mount order fixes positions; imported impl owners are rebound after exports exist.
use super::{
    Function, Index,
    mounts::Mounts,
    resolve::{path, ty},
};
use crate::{
    Result,
    analysis::sources::Sources,
    source::{model::SourceGraph, population::TestSelection},
};
use std::collections::BTreeMap;

impl Index {
    pub(super) fn read_functions(
        &mut self,
        sources: &Sources,
        graph: &SourceGraph,
        mounts: &Mounts,
        tests: TestSelection,
    ) -> Result<()> {
        let packages = &mounts.crates;
        // Re-export targets as written, with the module they are written
        // in: made absolute once every module is known, so a local module
        // named like a crate is read first (`relative`).
        let mut written_exports = Vec::new();
        for file in graph
            .physical_files()
            .filter(|f| tests.admits_file(f.test_only))
        {
            let Some(file_mounts) = mounts.files.get(&file.path) else {
                continue;
            };
            let parsed = sources[&file.path].workflow(&file.path, tests)?;
            for (scope, binary) in file_mounts {
                let namespace = scope[0].split('#').next().unwrap_or(&scope[0]);
                if !packages.contains(namespace) {
                    continue;
                }
                for (name, target) in &parsed.exports {
                    let mut key = scope.clone();
                    key.push(name.clone());
                    for target in target {
                        written_exports.push((key.clone(), scope.clone(), target.clone()));
                    }
                }
                for path in &parsed.types {
                    self.type_names.extend(path.last().cloned());
                    self.types
                        .insert(scope.iter().chain(path).cloned().collect());
                }
                for names in &parsed.modules {
                    let key: Vec<String> = scope.iter().chain(&names.path).cloned().collect();
                    self.modules.insert(key, names.clone());
                }
                for alias in &parsed.aliases {
                    let qualified: Vec<String> = scope.iter().chain(&alias.path).cloned().collect();
                    let module = qualified[..qualified.len() - 1].to_vec();
                    self.aliases.insert(qualified, (module, alias.clone()));
                }
                for flow in &parsed.functions {
                    let id = format!(
                        "{}::{} [{}]",
                        scope.join("::"),
                        flow.function,
                        flow.site.link()
                    );
                    if *binary && flow.function == "main" {
                        self.roots.push(self.functions.len());
                    }
                    let mut function_scope = scope.clone();
                    function_scope.extend(flow.module.clone());
                    self.functions.push(Function {
                        id,
                        scope: function_scope,
                        flow: flow.clone(),
                    });
                }
            }
        }
        for (key, scope, target) in written_exports {
            let value = self.relative(&scope, &target);
            self.exports.entry(key).or_default().push(value);
        }
        Ok(())
    }
    pub(super) fn name_functions(&mut self) {
        for (i, f) in self.functions.iter().enumerate() {
            if let Some(p) = path(&f.flow.function) {
                let mut key = f.scope.clone();
                key.extend(p.into_iter().skip(f.flow.module.len()));
                self.names.entry(key).or_default().push(i);
            }
        }
    }
    pub(super) fn bind_method_owners(&mut self) -> Result<()> {
        // Resolve an impl owner imported from another physical file only once.
        for (i, f) in self.functions.iter().enumerate() {
            if let Some(owner) = f.flow.owner.as_deref().and_then(ty)
                && let Some(mut key) = self.canonical(i, &owner).and_then(|p| self.exported(&p))
            {
                // `impl From<X> for usize` names no workspace `usize::from`:
                // Rust picks the impl by argument type, or an inherent method.
                // Only a trait impl can have a foreign owner.
                let foreign = f.flow.trait_impl
                    && (self.imported_from_elsewhere(&f.flow.imports, &owner)
                        || !self.declares(&key));
                if foreign {
                    self.foreign.insert(key.clone());
                }
                key.push(
                    f.flow
                        .function
                        .rsplit("::")
                        .next()
                        .ok_or("method name missing")?
                        .into(),
                );
                if let Some(p) = path(&f.flow.function) {
                    let mut original = f.scope.clone();
                    original.extend(p.into_iter().skip(f.flow.module.len()));
                    if let Some(values) = self.names.get_mut(&original) {
                        values.retain(|v| *v != i);
                        if values.is_empty() {
                            self.names.remove(&original);
                        }
                    }
                }
                if !foreign {
                    self.names.entry(key).or_default().push(i);
                }
            }
        }
        Ok(())
    }
    /// Whether an impl owner's canonical path is a workspace type: declared
    /// there, or an alias whose aliased path, followed through the imports
    /// where it is written, is one (an alias of a foreign type, such as
    /// `type Rational = Ratio<i128>` with `use num_rational::Ratio`, is not).
    /// A workspace type reached through an import the index does not follow
    /// (a glob, or a private `use` named from outside) still declares its
    /// name somewhere, so an unknown path whose name the workspace declares
    /// is kept.
    fn declares(&self, key: &[String]) -> bool {
        self.declares_within(key, 0)
    }
    fn declares_within(&self, key: &[String], depth: usize) -> bool {
        if self.types.contains(key) {
            return true;
        }
        if let Some((module, alias)) = self.aliases.get(key) {
            // Alias chains are short; a cycle is not a workspace type.
            return depth < 8
                && alias.target.as_ref().is_some_and(|target| {
                    !self.imported_from_elsewhere(&alias.imports, target)
                        && self
                            .in_scope(module, &alias.imports, target)
                            .is_some_and(|path| self.declares_within(&path, depth + 1))
                });
        }
        key.last()
            .is_some_and(|name| self.type_names.contains(name))
    }
    /// Whether `path` starts with a name imported only from another crate,
    /// such as `Ratio` after `use num_rational::Ratio`. Such a type is never
    /// the workspace's, whatever names the workspace declares.
    fn imported_from_elsewhere(
        &self,
        imports: &BTreeMap<String, Vec<Vec<String>>>,
        path: &[String],
    ) -> bool {
        path.first()
            .and_then(|name| imports.get(name))
            .is_some_and(|sources| {
                !sources.is_empty()
                    && sources.iter().all(|source| {
                        source.first().is_some_and(|root| {
                            !matches!(root.as_str(), "crate" | "self" | "super")
                                && !self.crates.contains(root)
                        })
                    })
            })
    }
}
