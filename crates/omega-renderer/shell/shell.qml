import "fixtures"
import QtQuick
import Quickshell

ShellRoot {
    FloatingWindow {
        title: "Omega · Renderer fixtures"
        visible: true
        implicitWidth: 480
        implicitHeight: 500
        Fixture { anchors.fill: parent }
    }
}
