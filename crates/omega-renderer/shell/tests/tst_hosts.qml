import QtQuick
import QtTest
import "../core" as Core
import "../fixtures" as Fixtures
import "../../../omega-omarchy/shell" as Omarchy
import qs.Commons

TestCase {
    id: test
    name: "RendererHosts"
    when: windowShown
    visible: true
    width: 500
    height: 550
    Component { id: assets; Core.Assets {} }
    Component { id: fixture; Fixtures.Fixture { width: 480; height: 500 } }
    Component { id: omarchyTheme; Omarchy.OmarchyTheme {} }
    function init() { failOnWarning(/.*/) }
    function test_fixture_scenarios_and_resize() {
        var window = createTemporaryObject(fixture, test)
        verify(window !== null)
        for (var scenario of ["normal", "loading", "long labels", "disabled"]) {
            window.scenario = scenario
            window.width = 300
            wait(1)
            window.width = 480
            wait(1)
        }
        compare(window.lastInteraction, "Interactions are captured here; no desktop commands run.")
    }
    function test_omarchy_adapter_uses_host_tokens() {
        var theme = createTemporaryObject(omarchyTheme, test)
        verify(theme !== null)
        compare(theme.foreground, Color.foreground)
        compare(theme.urgent, Color.urgent)
        compare(theme.font.body, Style.font.body)
        compare(theme.space(16), Style.space(16))
        compare(theme.controlFill(true, false, "white", "blue"), Style.controlFill(true, false, "white", "blue"))
    }
    function test_assets_delegate_icons_without_network_fallback() {
        var resolver = createTemporaryObject(assets, test)
        compare(resolver.resolve("https://example.invalid/icon.png"), "")
        compare(resolver.resolve("icon://files"), "")
        resolver.iconResolver = function(name) { return "resolved:" + name }
        compare(resolver.resolve("icon://files"), "resolved:files")
        compare(resolver.resolve("/tmp/local.png"), "/tmp/local.png")
    }
}
