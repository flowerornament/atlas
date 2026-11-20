//! Relations over this workspace: the qualified-to-lexical bridge is an
//! invariant of capture, not an observation of one fixture.
use atlas_lib::{
    analysis::Analysis,
    discovery,
    relations::{Cell, Relations, schema},
};
use std::{collections::BTreeSet, path::Path};

#[test]
fn every_qualified_function_and_type_bridges_to_one_distinct_declaration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snapshot = discovery::snapshot_workspace(&root).expect("snapshot this workspace");
    let facts = Analysis::new(&snapshot)
        .and_then(|analysis| analysis.representations())
        .expect("qualified facts");
    let relations = Relations::capture(snapshot.graph(), Some(&facts));
    let declarations: BTreeSet<_> = relations
        .get("declaration")
        .expect("lexical family")
        .records()
        .filter_map(|mut row| row.find(|(c, _)| *c == "id").and_then(|(_, v)| v.text()))
        .collect();
    for name in ["function", "type"] {
        let relation = relations.get(name).expect("qualified family");
        let column = relation
            .schema
            .position("declaration")
            .expect("bridge column");
        let mut seen = BTreeSet::new();
        assert!(!relation.rows.is_empty(), "{name}");
        for row in &relation.rows {
            let Some(Cell::Text(id)) = row.get(column) else {
                panic!("{name} row without a bridge: {row:?}");
            };
            assert!(declarations.contains(id.as_str()), "{name} {row:?}");
            assert!(seen.insert(id.clone()), "{id} bridged twice");
        }
    }
}

#[test]
fn every_relation_is_declared_once_and_names_existing_companions() {
    let mut names = BTreeSet::new();
    for relation in schema::RELATIONS {
        assert!(names.insert(relation.name), "{}", relation.name);
        if let Some(basis) = relation.basis {
            assert!(relation.position(basis).is_some(), "{}", relation.name);
        }
        if let Some(unknowns) = relation.unknowns {
            let companion = schema::relation(unknowns).expect("unknowns relation");
            assert_eq!(companion.family, relation.family, "{}", relation.name);
        }
    }
}
