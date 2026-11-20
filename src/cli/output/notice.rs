//! One stderr line naming the superseded spellings an invocation used. A
//! notice is never part of the answer: it is not written to stdout or JSON and
//! never changes the exit code. `ATLAS_NOTICES=off` hides it.

#[derive(Default)]
pub(crate) struct Notices {
    /// (old spelling, its replacement), in the order first used.
    superseded: Vec<(&'static str, &'static str)>,
}
impl Notices {
    pub(crate) fn superseded(&mut self, old: &'static str, new: &'static str) {
        if !self.superseded.contains(&(old, new)) {
            self.superseded.push((old, new));
        }
    }
    /// Print the line, before any other output, unless there is nothing to
    /// say or the reader turned notices off.
    pub(crate) fn print(&self) {
        if std::env::var_os("ATLAS_NOTICES").is_some_and(|value| value == "off") {
            return;
        }
        if let Some(line) = self.line() {
            eprintln!("{line}");
        }
    }
    fn line(&self) -> Option<String> {
        if self.superseded.is_empty() {
            return None;
        }
        // Group the old spellings by their replacement, in first-use order:
        // "--backend and --format are now --as".
        let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
        for (old, new) in &self.superseded {
            match groups.iter_mut().find(|(target, _)| target == new) {
                Some((_, olds)) => olds.push(old),
                None => groups.push((new, vec![old])),
            }
        }
        let clauses: Vec<String> = groups
            .iter()
            .map(|(new, olds)| match olds.as_slice() {
                [old] => format!("{old} is now {new}"),
                [rest @ .., last] => format!("{} and {last} are now {new}", rest.join(", ")),
                [] => unreachable!("a group has at least one spelling"),
            })
            .collect();
        Some(format!(
            "atlas: note: {} (the old spellings work until 1.0; ATLAS_NOTICES=off hides this)",
            clauses.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::Notices;

    #[test]
    fn one_line_groups_old_spellings_by_their_replacement() {
        let mut notices = Notices::default();
        assert_eq!(notices.line(), None);
        notices.superseded("--backend", "--as");
        notices.superseded("representations", "types");
        notices.superseded("--format", "--as");
        notices.superseded("--backend", "--as");
        assert_eq!(
            notices.line().as_deref(),
            Some(
                "atlas: note: --backend and --format are now --as, representations is now types (the old spellings work until 1.0; ATLAS_NOTICES=off hides this)"
            )
        );
    }
}
