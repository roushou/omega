//! Host-independent QML controls, embedded once and shared by all hosts.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    pub name: &'static str,
    pub contents: &'static str,
}

#[derive(Debug)]
pub struct Core;

impl Core {
    pub const SOURCE: &'static str = "crates/omega-renderer/shell/core";
    pub const FILES: &'static [Asset] = &[
        Asset {
            name: "core/Keyboard.js",
            contents: include_str!("../shell/core/Keyboard.js"),
        },
        Asset {
            name: "core/Navigation.qml",
            contents: include_str!("../shell/core/Navigation.qml"),
        },
        Asset {
            name: "core/InstanceSession.qml",
            contents: include_str!("../shell/core/InstanceSession.qml"),
        },
        Asset {
            name: "core/RendererConnection.qml",
            contents: include_str!("../shell/core/RendererConnection.qml"),
        },
        Asset {
            name: "core/Requests.qml",
            contents: include_str!("../shell/core/Requests.qml"),
        },
        Asset {
            name: "core/Assets.qml",
            contents: include_str!("../shell/core/Assets.qml"),
        },
        Asset {
            name: "core/ViewNode.qml",
            contents: include_str!("../shell/core/ViewNode.qml"),
        },
        Asset {
            name: "core/Theme.qml",
            contents: include_str!("../shell/core/Theme.qml"),
        },
        Asset {
            name: "core/Props.js",
            contents: include_str!("../shell/core/Props.js"),
        },
        Asset {
            name: "core/Icons.js",
            contents: include_str!("../shell/core/Icons.js"),
        },
        Asset {
            name: "core/nodes/Badge.qml",
            contents: include_str!("../shell/core/nodes/Badge.qml"),
        },
        Asset {
            name: "core/nodes/Button.qml",
            contents: include_str!("../shell/core/nodes/Button.qml"),
        },
        Asset {
            name: "core/nodes/Checkbox.qml",
            contents: include_str!("../shell/core/nodes/Checkbox.qml"),
        },
        Asset {
            name: "core/nodes/Dialog.qml",
            contents: include_str!("../shell/core/nodes/Dialog.qml"),
        },
        Asset {
            name: "core/nodes/Disclosure.qml",
            contents: include_str!("../shell/core/nodes/Disclosure.qml"),
        },
        Asset {
            name: "core/nodes/Dropdown.qml",
            contents: include_str!("../shell/core/nodes/Dropdown.qml"),
        },
        Asset {
            name: "core/nodes/Field.qml",
            contents: include_str!("../shell/core/nodes/Field.qml"),
        },
        Asset {
            name: "core/nodes/Form.qml",
            contents: include_str!("../shell/core/nodes/Form.qml"),
        },
        Asset {
            name: "core/nodes/Graph.qml",
            contents: include_str!("../shell/core/nodes/Graph.qml"),
        },
        Asset {
            name: "core/nodes/Grid.qml",
            contents: include_str!("../shell/core/nodes/Grid.qml"),
        },
        Asset {
            name: "core/nodes/Group.qml",
            contents: include_str!("../shell/core/nodes/Group.qml"),
        },
        Asset {
            name: "core/nodes/Header.qml",
            contents: include_str!("../shell/core/nodes/Header.qml"),
        },
        Asset {
            name: "core/nodes/Icon.qml",
            contents: include_str!("../shell/core/nodes/Icon.qml"),
        },
        Asset {
            name: "core/nodes/Image.qml",
            contents: include_str!("../shell/core/nodes/Image.qml"),
        },
        Asset {
            name: "core/nodes/Keycap.qml",
            contents: include_str!("../shell/core/nodes/Keycap.qml"),
        },
        Asset {
            name: "core/nodes/List.qml",
            contents: include_str!("../shell/core/nodes/List.qml"),
        },
        Asset {
            name: "core/nodes/Progress.qml",
            contents: include_str!("../shell/core/nodes/Progress.qml"),
        },
        Asset {
            name: "core/nodes/Scroll.qml",
            contents: include_str!("../shell/core/nodes/Scroll.qml"),
        },
        Asset {
            name: "core/nodes/Separator.qml",
            contents: include_str!("../shell/core/nodes/Separator.qml"),
        },
        Asset {
            name: "core/nodes/Slider.qml",
            contents: include_str!("../shell/core/nodes/Slider.qml"),
        },
        Asset {
            name: "core/nodes/Spacer.qml",
            contents: include_str!("../shell/core/nodes/Spacer.qml"),
        },
        Asset {
            name: "core/nodes/Stack.qml",
            contents: include_str!("../shell/core/nodes/Stack.qml"),
        },
        Asset {
            name: "core/nodes/Status.qml",
            contents: include_str!("../shell/core/nodes/Status.qml"),
        },
        Asset {
            name: "core/nodes/Text.qml",
            contents: include_str!("../shell/core/nodes/Text.qml"),
        },
        Asset {
            name: "core/nodes/Toggle.qml",
            contents: include_str!("../shell/core/nodes/Toggle.qml"),
        },
    ];
}
