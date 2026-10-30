//! Native policy configuration rules; source and project authority are separate inputs.
use super::{ConcentrationPolicy, CrateGraphPolicy, Policy, Ruling};
use crate::Result;
use std::collections::BTreeSet;
const POLICY_PATH: &str = "policy";
const SECTION: &str = "policy concentration";

impl Policy {
    /// Validate generic authored structure against known Cargo workspace members.
    /// This neither measures source nor resolves project authority.
    pub fn validate_configuration(&self, members: &BTreeSet<&str>) -> Result<()> {
        self.validate_header()?;
        require_unique_nonempty("population.core", &self.population.core)?;
        for name in &self.population.core {
            require_member(members, "core population", name)?;
        }
        let core: BTreeSet<_> = self.population.core.iter().collect();
        let ceilings: BTreeSet<_> = self.size.crates.keys().collect();
        if core != ceilings {
            return Err("policy: size.crate keys must equal population.core".into());
        }
        let sum = self
            .size
            .crates
            .values()
            .try_fold(0usize, |sum, value| sum.checked_add(*value))
            .ok_or("policy: per-crate ceiling sum overflows usize")?;
        if sum != self.size.core_token_lines {
            return Err(format!(
                "policy: core_token_lines {} differs from per-crate sum {sum}",
                self.size.core_token_lines
            )
            .into());
        }
        for (name, ceiling) in &self.surface {
            require_member(members, "surface ceiling", name)?;
            if *ceiling == 0 {
                return Err(
                    format!("policy: surface ceiling for `{name}` must be positive").into(),
                );
            }
        }
        self.crate_graph.validate_configuration(members)?;
        require_unique("typed_errors", &self.typed_errors)?;
        for name in &self.typed_errors {
            require_member(members, "typed error crate", name)?;
        }
        self.concentration
            .as_ref()
            .ok_or("policy: [concentration] is missing")?
            .validate_configuration(members)?;
        self.validate_authority_configuration()
    }
    fn validate_authority_configuration(&self) -> Result<()> {
        if self.authority.is_empty() {
            return Err("policy: authority must not be empty".into());
        }
        let mut facts = BTreeSet::new();
        for row in &self.authority {
            if row.fact.trim().is_empty() || !facts.insert(row.fact.as_str()) {
                return Err(format!(
                    "policy: authority facts must be nonempty and unique; found `{}`",
                    row.fact
                )
                .into());
            }
            super::anchor(&row.home, "authority home")?;
            for item in &row.forbid_item {
                super::anchor(item, "forbid_item")?;
            }
            for pattern in &row.forbid_pattern {
                super::anchor(pattern, "forbid_pattern")?;
            }
            for path in &row.forbid_path {
                crate::architecture::relative_path(path)
                    .map_err(|error| format!("policy: forbid_path for `{}`: {error}", row.fact))?;
            }
        }
        Ok(())
    }
}
impl CrateGraphPolicy {
    fn validate_configuration(&self, members: &BTreeSet<&str>) -> Result<()> {
        require_unique("crate_graph.vendored", &self.vendored)?;
        require_unique("crate_graph.terminal", &self.terminal)?;
        require_unique("crate_graph.transparent", &self.transparent)?;
        require_unique("crate_graph.forbidden_members", &self.forbidden_members)?;
        let vendored = self
            .vendored
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut names = BTreeSet::new();
        for rule in &self.rule {
            require_member(members, "crate rule", &rule.name)?;
            if !names.insert(rule.name.as_str()) {
                return Err(format!("{POLICY_PATH}: duplicate crate rule `{}`", rule.name).into());
            }
            if let Some(allowed) = &rule.allowed {
                require_unique("crate rule allowed set", allowed)?;
                for target in allowed {
                    if !members.contains(target.as_str()) && !vendored.contains(target.as_str()) {
                        return Err(format!(
                            "{POLICY_PATH}: crate rule `{}` allows unknown target `{target}`",
                            rule.name
                        )
                        .into());
                    }
                }
            }
        }
        for transparent in &self.transparent {
            if !names.contains(transparent.as_str()) {
                return Err(format!(
                    "{POLICY_PATH}: transparent crate `{transparent}` requires a crate rule"
                )
                .into());
            }
        }
        for terminal in &self.terminal {
            require_member(members, "terminal crate", terminal)?;
        }
        for forbidden in &self.forbidden_members {
            if members.contains(forbidden.as_str()) {
                return Err(format!(
                    "{POLICY_PATH}: forbidden workspace member `{forbidden}` is present"
                )
                .into());
            }
        }
        Ok(())
    }
}
fn require_member(members: &BTreeSet<&str>, field: &str, value: &str) -> Result<()> {
    if members.contains(value) {
        Ok(())
    } else {
        Err(format!("{POLICY_PATH}: {field} names non-workspace crate `{value}`").into())
    }
}

fn require_unique(field: &str, values: &[String]) -> Result<()> {
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() == values.len() {
        Ok(())
    } else {
        Err(format!("{POLICY_PATH}: {field} contains a duplicate").into())
    }
}

fn require_unique_nonempty(field: &str, values: &[String]) -> Result<()> {
    if values.is_empty() {
        return Err(format!("{POLICY_PATH}: {field} must not be empty").into());
    }
    require_unique(field, values)
}

impl ConcentrationPolicy {
    /// Structure the section must have before any tree is measured.
    pub(crate) fn validate_configuration(&self, members: &BTreeSet<&str>) -> Result<()> {
        let fail = |why: String| -> Result<()> { Err(format!("{SECTION}: {why}").into()) };
        if self.population.is_empty() {
            return fail("population must not be empty".into());
        }
        let mut seen = BTreeSet::new();
        for name in &self.population {
            if !members.contains(name.as_str()) {
                return fail(format!("population names non-workspace crate `{name}`"));
            }
            if !seen.insert(name.as_str()) {
                return fail(format!("population names `{name}` twice"));
            }
        }
        if self.entry_lines == 0 || self.file_lines == 0 {
            return fail("entry_lines and file_lines must be positive".into());
        }
        for (rule, rows) in [("entry", &self.entry), ("file", &self.file)] {
            let mut paths = BTreeSet::new();
            if let Some(row) = rows.iter().find(|row| !paths.insert(row.path.as_str())) {
                return fail(format!("{rule} row `{}` appears twice", row.path));
            }
        }
        let mut names = BTreeSet::new();
        for row in &self.name {
            if !names.insert(row.name.as_str()) {
                return fail(format!("name row `{}` appears twice", row.name));
            }
            if row.reruled_by.is_some() && row.ruling != Ruling::Facet {
                return fail(format!(
                    "name row `{}`: reruled_by belongs on a facet row",
                    row.name
                ));
            }
            if row.why.trim().is_empty() {
                return fail(format!("name row `{}` must say why", row.name));
            }
            if self.conventional_names.contains(&row.name) {
                return fail(format!(
                    "name `{}` is conventional and takes no ruling",
                    row.name
                ));
            }
        }
        if self
            .amended_by
            .as_ref()
            .is_some_and(|amendment| amendment.covers.is_empty())
        {
            return fail("amended_by must name what it covers".into());
        }
        let aliases = self.alias.iter().collect::<BTreeSet<_>>();
        if aliases.len() != self.alias.len() {
            return fail("an alias row appears twice".into());
        }
        if let Some(name) = self
            .header_uncited
            .keys()
            .find(|name| !self.population.contains(name))
        {
            return fail(format!(
                "header_uncited names `{name}`, outside the population"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Policy;
    use std::collections::{BTreeMap, BTreeSet};
    fn policy() -> Policy {
        Policy::parse("schema=1\nprovenance='authored'\nsurface={core=1}\nauthority=[{fact='one home',home='src/lib.rs::Home'}]\n[population]\ncore=['core']\n[size]\ncore_token_lines=2\ncrate={core=2}\n[crate_graph]\nrule=[{name='core',allowed=[],law='leaf'}]\n[concentration]\npopulation=['core']\nentry_lines=300\nfile_lines=2000\nconventional_names=[]\n").expect("native policy fixture")
    }
    #[test]
    fn native_configuration_accepts_its_declared_population_and_rejects_guard_mismatches() {
        let members = BTreeSet::from(["core"]);
        let good = policy();
        assert!(good.validate_configuration(&members).is_ok());
        let mut bad = good.clone();
        bad.population.core.push("core".into());
        assert!(
            bad.validate_configuration(&members)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let mut bad = good.clone();
        bad.size.crates.clear();
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = good.clone();
        bad.size.core_token_lines = 3;
        assert!(
            bad.validate_configuration(&members)
                .unwrap_err()
                .to_string()
                .contains("per-crate sum")
        );
        let mut bad = good.clone();
        bad.surface.insert("core".into(), 0);
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = good.clone();
        bad.typed_errors.push("missing".into());
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = good;
        bad.provenance = "inferred".into();
        assert!(bad.validate_configuration(&members).is_err());
    }
    #[test]
    fn native_crate_rule_membership_and_unique_lists_are_validated() {
        let members = BTreeSet::from(["core"]);
        let mut bad = policy();
        bad.crate_graph.rule.push(bad.crate_graph.rule[0].clone());
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = policy();
        bad.crate_graph.rule[0].allowed = Some(vec!["unknown".into()]);
        assert!(bad.validate_configuration(&members).is_err());
        bad.crate_graph.vendored.push("unknown".into());
        assert!(bad.validate_configuration(&members).is_ok());
        let mut bad = policy();
        bad.crate_graph.transparent.push("absent".into());
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = policy();
        bad.crate_graph.forbidden_members.push("core".into());
        assert!(bad.validate_configuration(&members).is_err());
    }
    #[test]
    fn native_concentration_shapes_and_authority_addresses_are_validated() {
        let members = BTreeSet::from(["core"]);
        let mut bad = policy();
        bad.authority.clear();
        assert!(
            bad.validate_configuration(&members)
                .unwrap_err()
                .to_string()
                .contains("authority must not be empty")
        );
        let mut bad = policy();
        bad.concentration = None;
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = policy();
        bad.concentration.as_mut().unwrap().file_lines = 0;
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = policy();
        bad.concentration
            .as_mut()
            .unwrap()
            .header_uncited
            .insert("outside".into(), 0);
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = policy();
        bad.authority[0].home = "not-an-anchor".into();
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = policy();
        bad.authority.push(bad.authority[0].clone());
        assert!(bad.validate_configuration(&members).is_err());
        let mut bad = policy();
        bad.authority[0].forbid_pattern.push("src::".into());
        assert!(bad.validate_configuration(&members).is_err());
    }
    #[test]
    fn authored_ceiling_overflow_refuses_without_panicking() {
        let members = BTreeSet::from(["core", "other"]);
        let mut bad = policy();
        bad.population.core.push("other".into());
        bad.size.crates = BTreeMap::from([("core".into(), usize::MAX), ("other".into(), 1)]);
        assert!(
            bad.validate_configuration(&members)
                .unwrap_err()
                .to_string()
                .contains("overflows")
        );
    }
}
