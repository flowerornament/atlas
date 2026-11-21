//! A method on a receiver typed through a module path resolves when the type
//! is re-exported with a restricted visibility (`pub(crate) use`), the shape
//! of `fn placed(standing: &mut select::Standing<'_, Entry>)` with
//! `use crate::backend::select;` and `pub(crate) use standing::Standing;`.
mod support;
use std::process::Command;
use support::Fixture;

fn callees(f: &Fixture, function: &str) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(f.path())
        .env("ATLAS_CACHE", "off")
        .env("ATLAS_NOTICES", "off")
        .args(["callees", function, "--as", "json"])
        .output()
        .expect("run Atlas");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON")
}

#[test]
fn restricted_reexports_carry_a_receivers_methods() {
    for visibility in ["pub", "pub(crate)", "pub(super)", "pub(in crate::backend)"] {
        let f = Fixture::new(&[
            ("src/lib.rs", "pub mod backend;\npub mod search;\n"),
            (
                "src/backend/mod.rs",
                "pub mod select;\npub(crate) fn search_here(s: &mut select::Standing<'_, u8>) -> bool { s.could_place() }\n",
            ),
            (
                "src/backend/select/mod.rs",
                &format!("mod standing;\n{visibility} use standing::Standing;\n"),
            ),
            (
                "src/backend/select/standing.rs",
                "pub(crate) struct Standing<'m, T> {\n    pub(crate) items: &'m [T],\n}\nimpl<T> Standing<'_, T> {\n    pub(crate) fn could_place(&self) -> bool {\n        self.items.is_empty()\n    }\n}\n",
            ),
            (
                "src/search.rs",
                "use crate::backend::select;\npub struct Entry;\npub(crate) fn placed(standing: &mut select::Standing<'_, Entry>) -> bool {\n    standing.could_place()\n}\n",
            ),
        ]);
        // `pub(super)` reaches `backend` only, so `search` cannot use it.
        let callers: &[&str] = if visibility == "pub(super)" {
            &["search_here"]
        } else {
            &["placed", "search_here"]
        };
        for function in callers {
            let report = callees(&f, function);
            let resolved = &report["answers"][0]["resolved"];
            assert_eq!(
                resolved[0]["to_name"], "Standing<'_, T>::could_place",
                "{visibility} {function}: {report}"
            );
        }
    }
}
