//! One syn UseTree flattening for source facts and qualified projections.
/// One local name introduced by a captured `use` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBinding {
    pub source: String,
    pub source_path: String,
    pub local_name: String,
    /// A braced `self` (`use a::b::{self};`) binds only the module `b`, never
    /// a same-named value or macro.
    pub module_only: bool,
}

/// Extract bindings from a scanner-owned `use` block.
///
/// Nested groups are flattened; globs remain explicit unresolved entries
/// instead of disappearing from downstream inventories.
pub fn import_bindings(declaration: &str) -> Result<Vec<ImportBinding>, syn::Error> {
    syn::parse_str::<syn::ItemUse>(declaration).map(|item| item_use_bindings(&item))
}
pub(crate) fn item_use_bindings(item: &syn::ItemUse) -> Vec<ImportBinding> {
    let mut bindings = use_tree_bindings(&item.tree);
    if item.leading_colon.is_some() {
        for binding in &mut bindings {
            binding.source_path.insert_str(0, "::");
        }
    }
    bindings
}

/// One use-tree grammar for lexical bindings and qualified import projections.
pub(crate) fn use_tree_bindings(tree: &syn::UseTree) -> Vec<ImportBinding> {
    fn walk(tree: &syn::UseTree, prefix: &[String], out: &mut Vec<ImportBinding>) {
        match tree {
            syn::UseTree::Path(path) => {
                let mut prefix = prefix.to_vec();
                prefix.push(path.ident.to_string());
                walk(&path.tree, &prefix, out);
            }
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    walk(tree, prefix, out);
                }
            }
            leaf => {
                let (source, alias, module_only) = match leaf {
                    syn::UseTree::Name(name) => {
                        (name.ident.to_string(), None, name.ident == "self")
                    }
                    syn::UseTree::Rename(name) => (
                        name.ident.to_string(),
                        Some(name.rename.to_string()),
                        name.ident == "self",
                    ),
                    syn::UseTree::Glob(_) => ("*".into(), None, false),
                    _ => return,
                };
                let mut path = prefix.to_vec();
                if source != "self" {
                    path.push(source);
                }
                if let Some(source) = path.last() {
                    out.push(ImportBinding {
                        source: source.clone(),
                        source_path: path.join("::"),
                        local_name: alias.unwrap_or_else(|| source.clone()),
                        module_only,
                    });
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, &[], &mut out);
    out.sort_by(|a, b| (&a.local_name, &a.source_path).cmp(&(&b.local_name, &b.source_path)));
    out.dedup();
    out
}
