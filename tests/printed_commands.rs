//! Every command Atlas prints for someone to run is built by
//! `atlas_lib::suggestion`, which adds the run's project and quotes its
//! words: no other code writes `cargo atlas` into its output. Static help
//! (the command line's help text and help topics), help commands, which read
//! no project, and `Example:` lines are documentation, not built commands.
use std::path::Path;

/// Files whose `cargo atlas` text is documentation or the renderer itself.
const DOCUMENTATION: [&str; 3] = [
    "src/cli/args.rs",
    "src/cli/help.rs",
    "crates/atlas-lib/src/suggestion.rs",
];

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && path.file_name().is_some_and(|name| name != "tests.rs")
        {
            out.push(path);
        }
    }
}

/// A line's `cargo atlas` that is not documentation.
fn built_here(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") {
        return false;
    }
    line.match_indices("cargo atlas").any(|(at, _)| {
        !line[at..].starts_with("cargo atlas help") && !line[..at].ends_with("Example: ")
    })
}

#[test]
fn every_printed_command_is_built_by_the_one_renderer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("crates/atlas-lib/src"), &mut files);
    let mut found = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .expect("under the workspace")
            .to_string_lossy()
            .replace('\\', "/");
        if DOCUMENTATION.contains(&relative.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("source");
        // A file's unit tests state expected output; they print nothing.
        let code = text
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or(&text);
        for (number, line) in code.lines().enumerate() {
            if built_here(line) {
                found.push(format!("{relative}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "build these with atlas_lib::suggestion::Command (or name the program with suggestion::PROGRAM):\n{}",
        found.join("\n")
    );
}
