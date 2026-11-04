//! Construct correlated function/name tables before exposing the immutable index.
//! Mount order fixes positions; imported impl owners are rebound after exports exist.
use super::{
    Function, Index,
    mounts::Mounts,
    resolve::{normalize, path, ty},
};
use crate::{
    Result,
    analysis::sources::Sources,
    source::{model::SourceGraph, population::TestSelection},
};

impl Index {
    pub(super) fn read_functions(
        &mut self,
        sources: &Sources,
        graph: &SourceGraph,
        mounts: &Mounts,
        tests: TestSelection,
    ) -> Result<()> {
        let packages = &mounts.crates;
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
                    let values: Vec<_> = target
                        .iter()
                        .map(|target| normalize(scope, target, &mounts.crates))
                        .collect();
                    self.exports.entry(key).or_default().extend(values);
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
                    }
                }
                self.names.entry(key).or_default().push(i);
            }
        }
        Ok(())
    }
}
