//! Native workspace classification, allowed-edge and cycle laws.
use super::super::policy::CrateGraphPolicy;
use std::collections::{BTreeMap, BTreeSet};
const POLICY_PATH: &str = "authored architecture policy";
impl CrateGraphPolicy {
    pub(super) fn violations(
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
        for (source, rule) in &rules {
            let Some(targets) = dag.get(*source) else {
                continue;
            };
            for target in targets {
                if terminal.contains(target.as_str()) {
                    violations.push(format!(
                        "TERMINAL LEAK: `{source}` depends on terminal `{target}`"
                    ));
                } else if let Some(allowed) = &rule.allowed
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
        }
        if let Some(cycle) = find_cycle(dag) {
            violations.push(format!("CYCLE: {}", cycle.join(" -> ")));
        }
        violations
    }
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
