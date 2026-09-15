import "core"

RendererConnection {
    id: link
    // One presentation transaction and one native popup owner per placement.
    readonly property PanelSession panelSession: PanelSession { connection: link }
}
