use omega::{
    View,
    ui::{Column, Text, TextArea, Viewport},
};

#[test]
fn viewport_requires_one_composed_canvas_and_valid_zoom() {
    let multiple: View = Viewport::new()
        .child(Text::new("first"))
        .child(Text::new("second"))
        .into();
    assert!(
        multiple
            .try_into_tree()
            .unwrap_err()
            .to_string()
            .contains("one canvas")
    );
    let composed: View = Viewport::new()
        .child(
            Column::new()
                .child(Text::new("first"))
                .child(Text::new("second")),
        )
        .into();
    assert!(composed.try_into_tree().is_ok());
    for zoom in [0.0, 9.0, f64::NAN, f64::INFINITY] {
        let view: View = Viewport::new().zoom(zoom).into();
        assert!(view.try_into_tree().is_err());
    }
    for zoom in [0.25, 1.0, 8.0] {
        let view: View = Viewport::new().zoom(zoom).into();
        assert!(view.try_into_tree().is_ok());
    }
    let area: View = TextArea::new("Notes").rows(0).into();
    assert!(area.try_into_tree().is_ok());
}
