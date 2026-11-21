//! Source control and matched call occurrences, grouped by physical file.
use super::{Detail, ExitKind, LoopKind, Site, Step, Workflow, compact::Folds};
use crate::{
    Result,
    analysis::index::{Index, Target},
    diagram::{Coverage, Diagram, Edge, EdgeEvidence, Level, Node, View},
};
use std::fmt::Write as _;

pub(super) fn diagram(flow: &Workflow) -> Diagram {
    let mut b = Builder::new(None, usize::MAX, Detail::Expressions);
    b.function(flow, None, 0);
    b.finish(flow)
}
pub(super) fn with_detail(flow: &Workflow, detail: Detail) -> Result<Diagram> {
    let budget = if detail == Detail::Control {
        600
    } else {
        usize::MAX
    };
    let mut b = Builder::new(None, budget, detail);
    b.function(flow, None, 0);
    b.checked(flow)
}
pub(super) fn expanded(index: &Index, entry: usize, depth: u8, detail: Detail) -> Result<Diagram> {
    let flow = &index
        .functions()
        .get(entry)
        .ok_or_else(|| format!("invalid indexed function position {entry}"))?
        .flow;
    let mut b = Builder::new(Some(index), 600, detail);
    b.function(flow, Some(entry), depth);
    b.checked(flow)
}
struct Builder<'a> {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    group: String,
    loops: Vec<LoopTarget>,
    index: Option<&'a Index>,
    stack: Vec<usize>,
    function: Option<usize>,
    exit: String,
    depth: u8,
    budget: usize,
    exhausted: bool,
    folds: Option<Folds>,
    targets: std::collections::BTreeMap<(usize, Site), Target>,
    /// One note per body-local declaration of every drawn body, once each.
    declared: Vec<String>,
}
struct LoopTarget {
    name: Option<String>,
    header: String,
    exit: String,
    kind: TargetKind,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    /// A loop body: any `break` or `continue` may reach it.
    Loop,
    /// A `while` condition: only a labelled transfer may name this loop, and
    /// an unlabelled one there is not valid Rust, so it reaches no target.
    WhileHeader,
    /// A labelled block: only `break 'label` leaves it; it has no `continue`.
    Block,
}
impl<'a> Builder<'a> {
    fn new(index: Option<&'a Index>, budget: usize, detail: Detail) -> Self {
        Self {
            nodes: vec![],
            edges: vec![],
            group: String::new(),
            loops: vec![],
            index,
            stack: vec![],
            function: None,
            exit: String::new(),
            depth: 0,
            budget,
            exhausted: false,
            folds: (detail == Detail::Control).then(Folds::default),
            targets: std::collections::BTreeMap::new(),
            declared: Vec::new(),
        }
    }
    fn checked(self, flow: &Workflow) -> Result<Diagram> {
        if let Some(position) = self.targets.values().find_map(|target| match target {
            Target::InvalidFunction(position) => Some(position),
            _ => None,
        }) {
            return Err(format!("invalid indexed function position {position}").into());
        }
        if self.exhausted {
            return Err("flow exceeds 600 source-control nodes; lower --depth or select a smaller function (nothing emitted)".into());
        }
        Ok(self.finish(flow))
    }
    fn finish(self, flow: &Workflow) -> Diagram {
        let files: std::collections::BTreeSet<_> =
            self.nodes.iter().map(|n| n.group.clone()).collect();
        let diagram = Diagram {
            title: format!("{} — source control and call occurrences", flow.function),
            level: Level::Declarations,
            view: View::Workflow,
            nodes: self.nodes,
            edges: self.edges,
            inventory: files.iter().cloned().collect(),
            coverage: Coverage {
                represented_files: files.into_iter().collect(),
                ..Coverage::default()
            },
            notes: vec![
                "Source-control possibilities, not path feasibility or a proof of termination. Panics, unwinding, destructors, overloaded operations and macro effects are not inferred.".into(),
                "Call occurrences have distinct continuations. Unexpanded calls continue only if they return; recursion and unresolved bodies are visible boundaries.".into(),
                "Pattern failure skips its guard; guard failure tries the next pattern. Each ? exits the current function on its residual branch.".into(),
            ]
            .into_iter()
            .chain(self.declared)
            .collect(),
            evidence_links: vec![],
        };
        if let Some(folds) = self.folds {
            folds.apply(diagram)
        } else {
            diagram
        }
    }
    fn node(&mut self, label: &str, site: &Site) -> String {
        if self.nodes.len() >= self.budget {
            self.exhausted = true;
        }
        let id = format!("flow-{}", self.nodes.len());
        self.nodes.push(Node {
            id: id.clone(),
            label: label.into(),
            group: self.group.clone(),
            source_sites: vec![site.link()],
            context: false,
            internal_relationships: vec![],
            surface: None,
        });
        id
    }
    fn connect(&mut self, from: Vec<String>, to: &str) {
        for from in from {
            self.edge(&from, to, None);
        }
    }
    fn edge(&mut self, from: &str, to: &str, label: Option<&str>) {
        self.edges.push(Edge {
            from: from.into(),
            to: to.into(),
            label: label.map(str::to_owned),
            evidence: EdgeEvidence::SourceOrder,
            relationships: vec![],
            declarations: vec![],
        });
    }
    fn function(
        &mut self,
        flow: &Workflow,
        function: Option<usize>,
        depth: u8,
    ) -> (String, String) {
        let old_group = std::mem::replace(&mut self.group, flow.site.path.clone());
        let old_loops = std::mem::take(&mut self.loops);
        let old_function = std::mem::replace(&mut self.function, function);
        let old_depth = std::mem::replace(&mut self.depth, depth);
        if let Some(i) = function {
            if let Some(index) = self.index {
                for call in &flow.calls {
                    self.targets
                        .entry((i, call.site.clone()))
                        .or_insert_with(|| index.resolve(i, call));
                }
            }
            self.stack.push(i);
        }
        if let Some(folds) = &mut self.folds {
            folds
                .call_sites
                .extend(flow.calls.iter().map(|c| c.site.clone()));
        }
        for declared in &flow.declarations {
            let note = format!(
                "{} declares {} at {}; it runs nothing, so it is not a step.",
                flow.function,
                declared.label,
                declared.site.link()
            );
            if !self.declared.contains(&note) {
                self.declared.push(note);
            }
        }
        let entry = self.node(&format!("fn {}", flow.function), &flow.site);
        let end = self.node(
            &format!("return from {} → {}", flow.function, flow.output),
            &flow.site,
        );
        let old_exit = std::mem::replace(&mut self.exit, end.clone());
        let tails = self.steps(&flow.steps, vec![entry.clone()]);
        self.connect(tails, &end);
        self.exit = old_exit;
        self.depth = old_depth;
        self.function = old_function;
        self.loops = old_loops;
        self.group = old_group;
        if function.is_some() {
            self.stack.pop();
        }
        (entry, end)
    }
    fn call(&mut self, id: &str, site: &Site) -> Option<String> {
        let index = self.index?;
        let f = self.function?;
        let target = self.targets.get(&(f, site.clone()))?.clone();
        let (target, reason) = match target {
            Target::Function(j) if self.stack.contains(&j) => (None, "recursion boundary".into()),
            Target::Function(_) if self.depth == 0 => (None, "depth boundary".into()),
            Target::Function(j) => (Some(j), String::new()),
            Target::Summarized(s) => (None, s.message),
            Target::Boundary(..) => (None, "declared boundary".into()),
            Target::InvalidFunction(position) => {
                (None, format!("invalid function position {position}"))
            }
        };
        let Some(j) = target else {
            if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                write!(
                    node.label,
                    "\n[not expanded: {reason}; continues if it returns]"
                )
                .expect("formatting into a String cannot fail");
            }
            return Some(id.to_owned());
        };
        let continuation = self.node(
            &format!(
                "after {} @ {}:{} (if returned)",
                index.functions()[j].flow.function,
                site.line,
                site.column
            ),
            site,
        );
        let flow = index.functions()[j].flow.clone();
        let (entry, exit) = self.function(&flow, Some(j), self.depth - 1);
        self.edge(id, &entry, Some("call"));
        self.edge(
            &exit,
            &continuation,
            Some(&format!("return to {}:{}", site.line, site.column)),
        );
        Some(continuation)
    }
    /// Each step as a node, connected from `tails`; the tails it leaves.
    fn steps(&mut self, steps: &[Step], mut tails: Vec<String>) -> Vec<String> {
        for step in steps {
            if self.nodes.len() >= self.budget {
                self.exhausted = true;
                return vec![];
            }
            let id = self.node(&label_of(step), step.site());
            self.connect(tails, &id);
            self.fold(step, &id);
            tails = match step {
                Step::Branch { arms, site, .. } => self.branch(&id, arms, site),
                Step::Match { arms, site, .. } => self.match_arms(id, arms, site),
                Step::Loop {
                    head,
                    body,
                    form,
                    site,
                    target,
                    ..
                } => self.loop_step(&id, (head, body), *form, site, target.as_ref()),
                Step::Block {
                    body, target, site, ..
                } => self.labelled_block(&id, body, target, site),
                Step::Exit { control, .. } => {
                    if let Some(target) = self.transfer_target(control) {
                        self.edge(&id, &target, Some("transfer"));
                    } else {
                        let unknown =
                            self.node("opaque control target (not resolved)", step.site());
                        self.connect(vec![id], &unknown);
                    }
                    vec![]
                }
                Step::Action {
                    fallible: true,
                    site,
                    ..
                }
                | Step::Opaque {
                    fallible: true,
                    site,
                    ..
                } => self.fallible(&id, step, site),
                Step::Action { site, .. } => vec![self.call(&id, site).unwrap_or(id)],
                Step::Borrow { .. } | Step::Opaque { .. } => vec![id],
            };
        }
        tails
    }
    /// What compact detail folds away later: borrows, and infallible calls
    /// at known call sites.
    fn fold(&mut self, step: &Step, id: &str) {
        let Some(folds) = &mut self.folds else {
            return;
        };
        if matches!(step, Step::Borrow { .. }) {
            folds.linear.push(id.to_owned());
        }
        if matches!(
            step,
            Step::Action {
                fallible: false,
                ..
            }
        ) && folds.call_sites.contains(step.site())
        {
            folds.calls.insert(id.to_owned());
        }
    }
    /// An `if`: each arm from its gate; the arms' tails.
    fn branch(&mut self, id: &str, arms: &[super::Arm], site: &Site) -> Vec<String> {
        let mut ends = vec![];
        for arm in arms {
            if self.exhausted {
                break;
            }
            let gate = self.node(&arm.label, site);
            self.edge(id, &gate, Some(&arm.label));
            ends.extend(self.steps(&arm.steps, vec![gate]));
        }
        ends
    }
    /// A `match`: patterns tried in order, each with its guard, then the
    /// arm body; coverage is not checked.
    fn match_arms(&mut self, id: String, arms: &[super::MatchArm], site: &Site) -> Vec<String> {
        let mut pending = vec![id];
        let mut ends = vec![];
        for arm in arms {
            if self.exhausted {
                break;
            }
            let pattern = self.node(&format!("pattern: {}", arm.pattern), site);
            self.connect(pending, &pattern);
            let next = self.node("try next pattern", site);
            if arm.pattern != "_" {
                self.edge(&pattern, &next, Some("does not match"));
            }
            let body_gate = self.node("matched arm body", site);
            if arm.guard.is_empty() {
                self.edge(&pattern, &body_gate, Some("matches"));
            } else {
                let guard = self.node("pattern matches: evaluate guard", site);
                self.edge(&pattern, &guard, Some("matches"));
                let guards = self.steps(&arm.guard, vec![guard]);
                let result = self.node("guard result", site);
                self.connect(guards, &result);
                self.edge(&result, &body_gate, Some("true"));
                self.edge(&result, &next, Some("false"));
            }
            ends.extend(self.steps(&arm.body, vec![body_gate]));
            pending = vec![next];
        }
        let unmatched = self.node("match coverage not checked (no fallthrough)", site);
        self.connect(pending, &unmatched);
        ends
    }
    /// A loop: its header, then its body back to the header; the exit.
    fn loop_step(
        &mut self,
        id: &str,
        (head, body): (&[Step], &[Step]),
        form: LoopKind,
        site: &Site,
        target: Option<&String>,
    ) -> Vec<String> {
        // A labelled transfer in a `while` condition names this loop, so
        // its target is registered before the condition.
        let exit = self.node("after loop (if it exits)", site);
        let header = form == LoopKind::While;
        if header {
            self.loops.push(LoopTarget {
                name: target.cloned(),
                header: id.to_owned(),
                exit: exit.clone(),
                kind: TargetKind::WhileHeader,
            });
        }
        let tested = self.steps(head, vec![id.to_owned()]);
        if header {
            self.loops.pop();
        }
        let test = self.node("loop header result", site);
        self.connect(tested, &test);
        let gate = self.node("loop body (when entered)", site);
        self.edge(&test, &gate, Some("enter"));
        if form != LoopKind::Infinite {
            let label = if form == LoopKind::For {
                "exhausted"
            } else {
                "false"
            };
            self.edge(&test, &exit, Some(label));
        }
        self.loops.push(LoopTarget {
            name: target.cloned(),
            header: id.to_owned(),
            exit: exit.clone(),
            kind: TargetKind::Loop,
        });
        let ends = self.steps(body, vec![gate]);
        self.loops.pop();
        self.connect(ends, id);
        vec![exit]
    }
    /// A labelled block: its body, then the node after it.
    fn labelled_block(
        &mut self,
        id: &str,
        body: &[Step],
        target: &str,
        site: &Site,
    ) -> Vec<String> {
        let exit = self.node("after labelled block", site);
        self.loops.push(LoopTarget {
            name: Some(target.to_owned()),
            header: id.to_owned(),
            exit: exit.clone(),
            kind: TargetKind::Block,
        });
        let ends = self.steps(body, vec![id.to_owned()]);
        self.loops.pop();
        self.connect(ends, &exit);
        vec![exit]
    }
    /// Where a `return`, `break` or `continue` goes: the function's exit, or
    /// the innermost target it can name (an unlabelled transfer never names
    /// a block; `continue 'label` cannot name a block).
    fn transfer_target(&self, control: &ExitKind) -> Option<String> {
        let (ExitKind::Break(name) | ExitKind::Continue(name)) = control else {
            return Some(self.exit.clone());
        };
        self.loops
            .iter()
            .rev()
            .find(|t| match name {
                Some(n) => t.name.as_ref() == Some(n),
                None => t.kind != TargetKind::Block,
            })
            .filter(|t| {
                !matches!(
                    (name, t.kind, control),
                    (None, TargetKind::WhileHeader, _)
                        | (_, TargetKind::Block, ExitKind::Continue(_))
                )
            })
            .map(|t| {
                if matches!(control, ExitKind::Break(_)) {
                    t.exit.clone()
                } else {
                    t.header.clone()
                }
            })
    }
    /// A `?`: its residual returns early to the exit, and success continues.
    fn fallible(&mut self, id: &str, step: &Step, site: &Site) -> Vec<String> {
        if let Some(folds) = &mut self.folds {
            let binding = if let Step::Action { binding, .. } = step {
                binding.clone()
            } else {
                None
            };
            folds.residuals.push((id.to_owned(), binding));
        }
        let early = self.node("? returns early (residual)", site);
        if let Some(folds) = &mut self.folds {
            folds.linear.push(early.clone());
        }
        self.edge(id, &early, Some("residual"));
        let exit = self.exit.clone();
        self.edge(&early, &exit, Some("return residual"));
        let success = self.node("? continues", site);
        if let Some(folds) = &mut self.folds {
            folds.linear.push(success.clone());
        }
        self.edge(id, &success, Some("success"));
        vec![success]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn return_arm_does_not_flow_into_following_statement() {
        let flow =
            crate::workflow::parse::parse("fn f(){if x {return early();} late();}", "lib.rs", "f")
                .unwrap();
        let diagram = flow.diagram();
        diagram.validate().unwrap();
        let ret = diagram
            .nodes
            .iter()
            .find(|n| n.label.starts_with("return"))
            .unwrap();
        assert!(diagram.edges.iter().filter(|e| e.from == ret.id).all(|e| {
            diagram
                .nodes
                .iter()
                .find(|n| n.id == e.to)
                .unwrap()
                .label
                .starts_with("return from")
        }));
        assert!(
            diagram
                .edges
                .iter()
                .all(|e| matches!(e.evidence, EdgeEvidence::SourceOrder))
        );
    }
    #[test]
    fn fallible_step_has_explicit_early_return_sink() {
        let flow =
            crate::workflow::parse::parse("fn f(){let x=read()?; consume(x);}", "lib.rs", "f")
                .unwrap();
        let diagram = flow.diagram();
        let early = diagram
            .nodes
            .iter()
            .find(|n| n.label.starts_with("? returns"))
            .unwrap();
        assert!(diagram.edges.iter().any(|e| e.to == early.id));
        assert!(
            diagram
                .edges
                .iter()
                .filter(|e| e.from == early.id)
                .all(|e| diagram
                    .nodes
                    .iter()
                    .find(|n| n.id == e.to)
                    .unwrap()
                    .label
                    .starts_with("return from"))
        );
    }
}

#[cfg(test)]
mod loop_tests {
    #[test]
    fn break_exits_its_loop_and_continue_repeats_it() {
        let flow = crate::workflow::parse::parse(
            "fn f(){'outer: loop { loop {if stop {break 'outer;} continue;} } after();}",
            "lib.rs",
            "f",
        )
        .unwrap();
        let diagram = flow.diagram();
        diagram.validate().unwrap();
        let jump = diagram
            .nodes
            .iter()
            .find(|n| n.label == "break 'outer")
            .unwrap();
        let edge = diagram.edges.iter().find(|e| e.from == jump.id).unwrap();
        let outer_exit = diagram.nodes.iter().find(|n| n.id == edge.to).unwrap();
        assert!(outer_exit.label.starts_with("after loop"));
        let after = diagram.nodes.iter().find(|n| n.label == "after()").unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == outer_exit.id && e.to == after.id)
        );
        let jump = diagram
            .nodes
            .iter()
            .find(|n| n.label == "continue")
            .unwrap();
        let edge = diagram.edges.iter().find(|e| e.from == jump.id).unwrap();
        let header = diagram.nodes.iter().find(|n| n.id == edge.to).unwrap();
        assert_eq!(header.label, "loop");
        assert_ne!(header.id, "flow-1");
    }

    /// The target of each transfer whose label starts with `jump`.
    fn targets(source: &str, jump: &str) -> Vec<String> {
        let diagram = crate::workflow::parse::parse(source, "lib.rs", "f")
            .unwrap()
            .diagram();
        diagram.validate().unwrap();
        diagram
            .nodes
            .iter()
            .filter(|n| n.label.starts_with(jump))
            .map(|n| {
                let edge = diagram.edges.iter().find(|e| e.from == n.id).unwrap();
                let to = diagram.nodes.iter().find(|t| t.id == edge.to).unwrap();
                to.label.clone()
            })
            .collect()
    }

    #[test]
    fn labelled_transfers_in_a_while_condition_reach_their_loop() {
        let source = "fn f(){'a: while { if p() { continue 'a; } q() } { 'b: while { if p() { break 'a; } if q() { continue 'b; } r() } { s(); } } after();}";
        // A loop's header node is labelled with its condition.
        let continues = targets(source, "continue");
        assert_eq!(continues.len(), 2, "{continues:?}");
        assert!(continues[0].starts_with("while { if p() { continue 'a"));
        assert!(continues[1].starts_with("while { if p() { break 'a"));
        assert_eq!(targets(source, "break 'a"), ["after loop (if it exits)"]);
    }

    #[test]
    fn break_label_leaves_a_labelled_block_and_plain_transfers_skip_it() {
        let source = "fn f(){ loop { 'found: { if p() { break 'found; } if q() { break; } if r() { continue; } work(); } after(); } }";
        assert_eq!(targets(source, "break 'found"), ["after labelled block"]);
        // The unlabelled break and continue name the loop, not the block.
        assert_eq!(
            targets(source, "break"),
            ["after labelled block", "after loop (if it exits)"]
        );
        assert_eq!(targets(source, "continue"), ["loop"]);
        let diagram = crate::workflow::parse::parse(source, "lib.rs", "f")
            .unwrap()
            .diagram();
        let after_block = diagram
            .nodes
            .iter()
            .find(|n| n.label == "after labelled block")
            .unwrap();
        let after = diagram.nodes.iter().find(|n| n.label == "after()").unwrap();
        assert!(
            diagram
                .edges
                .iter()
                .any(|e| e.from == after_block.id && e.to == after.id)
        );
    }

    #[test]
    fn continue_cannot_name_a_labelled_block() {
        let source = "fn f(){ 'b: { if p() { continue 'b; } } }";
        assert_eq!(
            targets(source, "continue"),
            ["opaque control target (not resolved)"]
        );
    }

    #[test]
    fn an_unlabelled_transfer_in_a_while_condition_is_not_resolved() {
        // Rust refuses it; Atlas must not pick the enclosing loop instead.
        let source = "fn f(){ loop { while { if p() { continue; } q() } { s(); } } }";
        assert_eq!(
            targets(source, "continue"),
            ["opaque control target (not resolved)"]
        );
    }
}

/// A step's node label: a fallible call or opaque step is marked `?`.
fn label_of(step: &Step) -> String {
    let question = |fallible: bool| if fallible { " ?" } else { "" };
    match step {
        Step::Action { fallible, .. } => format!("{}{}", step.label(), question(*fallible)),
        Step::Opaque { fallible, .. } => {
            format!("[opaque] {}{}", step.label(), question(*fallible))
        }
        _ => step.label().to_owned(),
    }
}
