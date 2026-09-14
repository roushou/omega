use omega::{
    View,
    ui::{Column, Component, Field, List, Row, Text, ViewError},
};

struct Search;
impl Component for Search {
    fn render(&self) -> View {
        Column::new()
            .child(Row::new().child(Field::new("Search").navigate("results")))
            .child(Column::new().child(List::new().id("results").key("items")))
            .into()
    }
}

#[test]
fn references_cross_layouts_and_remain_private_to_each_component() {
    let view: View = Column::new()
        .child(Search.key("a"))
        .child(Search.key("b"))
        .into();
    let root = view.try_into_tree().unwrap().root.unwrap();
    for component in &root.children {
        assert_eq!(
            component.children[0].children[0].navigation_target,
            component.children[1].children[0].key
        );
    }
    assert_ne!(
        root.children[0].children[0].children[0].navigation_target,
        root.children[1].children[0].children[0].navigation_target
    );
    let inaccessible: View = Column::new()
        .child(Field::new("Outside").navigate("results"))
        .child(Search)
        .into();
    assert!(matches!(
        inaccessible.try_into_tree(),
        Err(ViewError::MissingId { .. })
    ));
}

struct Results;
impl Component for Results {
    fn render(&self) -> View {
        List::new().into()
    }
}

#[test]
fn component_root_ids_are_exposed_without_changing_reconciliation_keys() {
    let view: View = Column::new()
        .child(Field::new("Search").navigate("public"))
        .child(Results.id("public").key("stable"))
        .into();
    let root = view.try_into_tree().unwrap().root.unwrap();
    assert_eq!(root.children[1].key, "stable");
    assert_eq!(root.children[0].navigation_target, "stable");
}

#[test]
fn invalid_references_report_the_offending_id() {
    let cases: Vec<(View, &str)> = vec![
        (
            Column::new()
                .child(Text::new("a").id("same"))
                .child(Row::new().child(List::new().id("same")))
                .into(),
            "duplicate ID \"same\"",
        ),
        (
            Field::new("Search").navigate("missing").into(),
            "missing ID \"missing\"",
        ),
        (
            Column::new()
                .child(Field::new("Search").navigate("label"))
                .child(Text::new("label").id("label"))
                .into(),
            "which is text, not a list",
        ),
        (List::new().id("").into(), "empty ID"),
        (
            Column::new()
                .child(Field::new("Search").navigate("key"))
                .child(List::new().key("key"))
                .into(),
            "missing ID \"key\"",
        ),
    ];
    for (view, message) in cases {
        assert!(
            view.try_into_tree()
                .unwrap_err()
                .to_string()
                .contains(message)
        );
    }
}
