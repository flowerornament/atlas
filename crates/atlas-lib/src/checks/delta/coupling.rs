//! Native workspace classification, allowed-edge, unused-allowance,
//! vendored-edge and cycle laws, judged by `policy` and `delta` alike.
use super::super::policy::{Allowed, CrateGraphPolicy};
use std::collections::{BTreeMap, BTreeSet};
const POLICY_PATH: &str = "authored architecture policy";
impl CrateGraphPolicy {
    /// The graph's violations over `members` and their normal dependencies
    /// on members (`dag`). The rules are checked for shape first
    /// (`validate_configuration`): every rule writes `allowed`.
    pub(in crate::checks) fn violations(
        &self,
        members: &BTreeSet<String>,
        dag: &BTreeMap<String, BTreeSet<String>>,
    ) -> Vec<String> {
        let rules = self
            .rule
            .iter()
            .map(|rule| (rule.name.as_str(), rule))
            .collect::<BTreeMap<_, _>>();
        let vendored = self
            .vendored
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let terminal = self
            .terminal
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let transparent = self
            .transparent
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut violations = Vec::new();
        for forbidden in &self.forbidden_members {
            if members.contains(forbidden) {
                violations.push(format!(
                    "FORBIDDEN MEMBER: `{forbidden}` is a workspace member"
                ));
            }
        }
        for member in members {
            if !rules.contains_key(member.as_str())
                && !vendored.contains(member.as_str())
                && !terminal.contains(member.as_str())
            {
                violations.push(format!(
                    "UNCLASSIFIED CRATE: `{member}`; classify it in {POLICY_PATH}"
                ));
            }
        }
        let none = BTreeSet::new();
        for (source, rule) in &rules {
            let targets = dag.get(*source).unwrap_or(&none);
            violations.extend(edge_violations(
                source,
                rule,
                targets,
                (&terminal, &transparent),
            ));
        }
        // Code copied in from elsewhere was written against no member.
        for source in &vendored {
            for target in dag.get(*source).into_iter().flatten() {
                if members.contains(target) {
                    violations.push(format!(
                        "VENDORED EDGE: vendored `{source}` depends on member `{target}`; code copied in from elsewhere depends on no member"
                    ));
                }
            }
        }
        if let Some(cycle) = find_cycle(dag) {
            violations.push(format!("CYCLE: {}", cycle.join(" -> ")));
        }
        violations
    }
}

/// One rule's edges: a dependency on a terminal package, one its
/// `allowed` list does not name (unless the target is transparent), and an
/// allowance it does not use.
fn edge_violations(
    source: &str,
    rule: &super::super::policy::CrateRule,
    targets: &BTreeSet<String>,
    (terminal, transparent): (&BTreeSet<&str>, &BTreeSet<&str>),
) -> Vec<String> {
    let mut violations = Vec::new();
    for target in targets {
        if terminal.contains(target.as_str()) {
            violations.push(format!(
                "TERMINAL LEAK: `{source}` depends on terminal `{target}`"
            ));
        } else if let Some(Allowed::Crates(allowed)) = &rule.allowed
            && !allowed.contains(target)
            && !transparent.contains(target.as_str())
        {
            violations.push(format!(
                "ILLEGAL EDGE: `{source}` -> `{target}`; allowed: {allowed:?}{}",
                rule.law
                    .as_ref()
                    .map_or_else(String::new, |law| format!(" [{law}]"))
            ));
        }
    }
    // An allowance the crate does not use is slack a later edge
    // would spend without anyone deciding it.
    if let Some(Allowed::Crates(allowed)) = &rule.allowed {
        for unused in allowed.iter().filter(|target| !targets.contains(*target)) {
            violations.push(format!(
                "UNUSED ALLOWANCE: `{source}` allows `{unused}` but does not depend on it; remove it from the allowed set"
            ));
        }
    }
    violations
}

fn find_cycle(dag: &BTreeMap<String, BTreeSet<String>>) -> Option<Vec<String>> {
    fn visit(
        node: &str,
        dag: &BTreeMap<String, BTreeSet<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if active.contains(node) {
            let start = path.iter().position(|value| value == node).unwrap_or(0);
            let mut cycle = path[start..].to_vec();
            cycle.push(node.to_owned());
            return Some(cycle);
        }
        if done.contains(node) {
            return None;
        }
        active.insert(node.to_owned());
        path.push(node.to_owned());
        if let Some(targets) = dag.get(node) {
            for target in targets {
                if let Some(cycle) = visit(target, dag, active, done, path) {
                    return Some(cycle);
                }
            }
        }
        path.pop();
        active.remove(node);
        done.insert(node.to_owned());
        None
    }
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();
    let mut path = Vec::new();
    for node in dag.keys() {
        if let Some(cycle) = visit(node, dag, &mut active, &mut done, &mut path) {
            return Some(cycle);
        }
    }
    None
}
