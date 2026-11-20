//! The bare argument is the command's subject (spec §3.2): a scope the user
//! typed is never dropped, a positional ROOT is read only where it can only
//! mean a project, and every refusal names what to run instead.
mod support;
use std::{
    path::Path,
    process::{Command, Output},
};
use support::Fixture;

const NOTE: &str = "(the old spellings work until 1.0; ATLAS_NOTICES=off hides this)";

/// A workspace whose root package has `src/`, with member `a/`, a member
/// package `util` at `crates/helpers`, and a root file named `util.rs`.
fn workspace() -> Fixture {
    let f = Fixture::new(&[
        ("src/lib.rs", "pub mod util;\npub fn root() {}\n"),
        ("src/util.rs", "pub fn shared() {}\n"),
        ("src/cli/mod.rs", "pub fn cli() {}\n"),
        ("tests/cli.rs", "#[test]\nfn cli() {}\n"),
        (
            "a/src/lib.rs",
            "pub struct Alpha;\npub fn alpha() -> Alpha { Alpha }\n",
        ),
        ("crates/helpers/src/lib.rs", "pub fn help() {}\n"),
    ]);
    f.write(
        "Cargo.toml",
        "[package]\nname='root'\nversion='0.1.0'\nedition='2024'\n[workspace]\nmembers=['a','crates/helpers']\n",
    );
    f.write(
        "a/Cargo.toml",
        "[package]\nname='a'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write(
        "crates/helpers/Cargo.toml",
        "[package]\nname='util'\nversion='0.1.0'\nedition='2024'\n",
    );
    f
}
/// A separate project, outside `workspace()`.
fn outside() -> Fixture {
    Fixture::new(&[("src/lib.rs", "pub struct Elsewhere;\n")])
}
fn atlas(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-atlas"))
        .current_dir(dir)
        .env_remove("ATLAS_NOTICES")
        .env("ATLAS_CACHE", "off")
        .args(args)
        .output()
        .expect("run Atlas")
}
fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
fn refusal(dir: &Path, args: &[&str]) -> String {
    let output = atlas(dir, args);
    assert!(!output.status.success(), "{args:?} should refuse");
    assert!(output.stdout.is_empty(), "{args:?}");
    stderr(&output)
}

#[test]
fn a_bare_scope_inside_the_workspace_is_that_scope() {
    let f = workspace();
    for (args, heading) in [
        (&["census", "a"][..], "a: "),
        (&["census", "src"], "src: "),
        (&["census", "a/src"], "a/src: "),
        (&["census", "helpers"], "crates/helpers: "),
        (&["census", "-p", "util"], "util: "),
        (&["census", "-p", "a"], "a: "),
        (&["census", "--in", "a"], "a: "),
        (&["census"], "workspace: "),
        (&["census", "."], "workspace: "),
    ] {
        let output = atlas(f.path(), args);
        assert!(
            stdout(&output).starts_with(heading),
            "{args:?}: {}",
            stdout(&output)
        );
        assert!(output.stderr.is_empty(), "{args:?}: {}", stderr(&output));
    }
    // The same reading on every scope command.
    for command in [
        "orient",
        "spans",
        "concentration",
        "nontest-loc",
        "comments",
        "dependencies",
    ] {
        let bare = atlas(f.path(), &[command, "a", "--json"]);
        let named = atlas(f.path(), &[command, "--in", "a", "--json"]);
        assert_eq!(stdout(&bare), stdout(&named), "{command}");
        assert!(bare.stderr.is_empty(), "{command}: {}", stderr(&bare));
    }
}

#[test]
fn a_project_outside_the_workspace_is_still_read_as_the_root_with_a_notice() {
    let f = workspace();
    let other = outside();
    let path = other.path().to_str().expect("UTF-8 fixture path");
    let root_note = format!("atlas: note: positional ROOT is now --root {NOTE}\n");
    // AT2: one bare word that is another project.
    let census = atlas(f.path(), &["census", path]);
    assert!(
        stdout(&census).starts_with("workspace: 1 files"),
        "{}",
        stdout(&census)
    );
    assert_eq!(stderr(&census), root_note);
    assert_eq!(
        stdout(&census),
        stdout(&atlas(f.path(), &["census", "--root", path]))
    );
    // AT3: two bare words are root and scope.
    let scoped = atlas(other.path(), &["census", ".", "src"]);
    assert!(stdout(&scoped).starts_with("src: "), "{}", stdout(&scoped));
    assert_eq!(stderr(&scoped), root_note);
    let conflict = refusal(
        other.path(),
        &[
            "census",
            ".",
            "src",
            "--root",
            f.path().to_str().expect("UTF-8"),
        ],
    );
    assert!(
        conflict.ends_with(
            "atlas: positional ROOT conflicts with --root; choose one project directory\n"
        ),
        "{conflict}"
    );
    // `name ROOT NAME` and `name NAME`.
    let name = atlas(f.path(), &["name", path, "Elsewhere"]);
    assert!(stdout(&name).starts_with("Elsewhere: 1 declaration site"));
    assert_eq!(stderr(&name), root_note);
    assert!(stdout(&atlas(f.path(), &["name", "Alpha"])).starts_with("Alpha: 1 declaration site"));
    // A command with no subject reads only a project.
    assert!(stdout(&atlas(f.path(), &["dump", path])).contains("Elsewhere"));
    assert_eq!(
        refusal(f.path(), &["dump", "src"]),
        "atlas: dump takes no subject; to read another project use --root PATH\n"
    );
}

#[test]
fn types_takes_its_types_as_the_bare_argument() {
    let f = workspace();
    let other = outside();
    let path = other.path().to_str().expect("UTF-8 fixture path");
    let bare = atlas(f.path(), &["types", "Alpha", "--json"]);
    let flagged = atlas(f.path(), &["types", "--type", "Alpha", "--json"]);
    assert_eq!(stdout(&bare), stdout(&flagged));
    assert!(bare.stderr.is_empty(), "{}", stderr(&bare));
    assert_eq!(
        stderr(&flagged),
        format!("atlas: note: --type is now the bare TYPES argument {NOTE}\n")
    );
    // 0.18 ended this in "failed to start `cargo metadata`".
    let elsewhere = atlas(f.path(), &["types", "Elsewhere", "--root", path, "--json"]);
    assert!(stdout(&elsewhere).contains("\"Elsewhere\""));
    // `types ROOT --type X`, the 0.18 spelling, still reads ROOT.
    let old = atlas(f.path(), &["types", path, "--type", "Elsewhere", "--json"]);
    assert_eq!(stdout(&old), stdout(&elsewhere));
    assert!(stderr(&old).contains("positional ROOT is now --root"));
    assert!(
        refusal(f.path(), &["types", "Alpha", "--type", "Alpha"])
            .contains("give the types once: a bare TYPES or --type/--query")
    );
}

#[test]
fn every_scope_refusal_says_what_to_run() {
    let f = workspace();
    let dir = f.path();
    // R2: a name that is both a path and a package elsewhere.
    assert_eq!(
        refusal(dir, &["census", "util"]),
        "atlas: \"util\" is both the path src/util.rs and the package util (at crates/helpers); use --in src/util.rs or -p util\n"
    );
    // R3: a name that matches several paths, with the run line.
    assert_eq!(
        refusal(dir, &["census", "cli"]),
        "atlas: \"cli\" matches 2 paths: src/cli, tests/cli.rs; give one with --in PATH; run: cargo atlas census --in src/cli\n"
    );
    assert!(
        refusal(dir, &["spans", "--in", "cli"])
            .ends_with("; run: cargo atlas spans --in src/cli\n")
    );
    // A file's whole name is a name too: unique, or refused with the choices.
    let unique = atlas(dir, &["census", "util.rs"]);
    assert!(
        stdout(&unique).starts_with("src/util.rs: "),
        "{}",
        stdout(&unique)
    );
    assert_eq!(
        refusal(dir, &["census", "lib.rs"]),
        "atlas: \"lib.rs\" matches 3 paths: a/src/lib.rs, crates/helpers/src/lib.rs, src/lib.rs; give one with --in PATH; run: cargo atlas census --in a/src/lib.rs\n"
    );
    assert!(stdout(&atlas(dir, &["spans", "--in", "mod.rs"])).starts_with("src/cli/mod.rs"));
    // R4: nothing by that name.
    assert_eq!(
        refusal(dir, &["census", "nowhere"]),
        "atlas: no path, file name or package \"nowhere\" in this project; cargo atlas orient lists them\n"
    );
    assert_eq!(
        refusal(dir, &["census", "-p", "nowhere"]),
        "atlas: no package \"nowhere\" in this workspace; its packages: a, root, util\n"
    );
    // The scope is given once.
    assert_eq!(
        refusal(dir, &["census", "a", "--in", "a"]),
        "atlas: give the scope once: a bare argument or --in/-p\n"
    );
    // A path command cannot read a package that holds another package.
    assert!(
        refusal(dir, &["spans", "-p", "root"]).contains(
            "spans reads paths, and package root's directory . also holds package a; give the part you mean with --in PATH"
        )
    );
    assert!(stdout(&atlas(dir, &["spans", "-p", "a"])).starts_with("a/src/lib.rs"));
}

#[test]
fn scope_and_its_old_spelling_read_the_same() {
    let f = workspace();
    let dir = f.path();
    for (old, new) in [
        (
            &["census", "--scope", "a"][..],
            &["census", "--in", "a"][..],
        ),
        (
            &["spans", "--scope", "src", "--json"],
            &["spans", "--in", "src", "--json"],
        ),
        (
            &["map", "--scope", "src", "--level", "files"],
            &["map", "--in", "src", "--level", "files"],
        ),
    ] {
        let before = atlas(dir, old);
        assert_eq!(stdout(&before), stdout(&atlas(dir, new)), "{old:?}");
        assert_eq!(
            stderr(&before),
            format!("atlas: note: --scope is now --in {NOTE}\n"),
            "{old:?}"
        );
    }
    // A tail names the spelling that shows the rest.
    let tail = stdout(&atlas(dir, &["spans", "src", "--rows", "1"]));
    assert!(
        tail.ends_with("to see them: cargo atlas spans --all --in src\n"),
        "{tail}"
    );
}
