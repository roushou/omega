import QtQuick
import QtTest
import "../../../omega-omarchy/shell" as Omarchy

TestCase {
    id: test
    name: "PlacementConnections"
    Component {
        id: factory
        Omarchy.PlacementConnection { unit: "audio"; surface: "indicator"; module: "audio" }
    }
    function init() { failOnWarning(/.*/) }
    function lease(properties) {
        var result = createTemporaryObject(factory, test, properties || {})
        verify(result !== null)
        return result
    }
    function snapshot(link, revision) {
        link.onLine(JSON.stringify({unit:"audio",surface:"indicator",
            instance:{id:"audio-instance",incarnation:"session"}, requested:2,
            view:{revision:revision,root:{type:"text",key:"volume"}}}))
    }
    function test_monitors_share_attachment_views_and_interactions() {
        var laptop = lease()
        var link = laptop.connection
        tryCompare(link, "connected", true)
        var initialWrites = link.socket.written
        var external = lease()
        compare(external.connection, link)
        compare(link.socket.written, initialWrites)
        link.onLine(JSON.stringify({streamId:link.attachmentStream,result:{instances:{instances:[]}}}))
        snapshot(link, "1")
        compare(external.connection.tree, laptop.connection.tree)
        verify(external.connection.interact(link.instance, "1", "volume", "press"))
        verify(laptop.connection.busy("volume"))
        snapshot(link, "2")
        compare(external.connection.revision, "2")
        compare(laptop.connection.revision, "2")
    }
    function test_removing_original_monitor_retains_transport_until_last_release() {
        var laptop = lease()
        var external = lease()
        var link = laptop.connection
        var key = laptop.heldKey
        laptop.destroy()
        wait(1)
        compare(external.connection, link)
        verify(link.connected)
        compare(Omarchy.PlacementConnections.entries[key].users, 1)
        var returned = lease()
        compare(returned.connection, link)
        external.destroy()
        returned.destroy()
        wait(1)
        compare(Omarchy.PlacementConnections.entries[key], undefined)
    }
    function test_reconnect_is_shared_by_both_monitors() {
        var laptop = lease()
        var external = lease()
        var link = laptop.connection
        link.attached = true
        snapshot(link, "1")
        link.reconnect()
        compare(external.connection.tree, null)
        compare(laptop.connection.tree, null)
        tryCompare(link, "connected", true)
        link.attached = true
        snapshot(link, "2")
        compare(external.connection.revision, "2")
        compare(laptop.connection.revision, "2")
    }
    function test_scopes_and_daemons_remain_separate() {
        var original = lease()
        for (var properties of [{unit:"power"}, {surface:"panel"}, {module:"second"}, {socketPath:"/tmp/other-omega.sock"}]) {
            var other = lease(properties)
            verify(other.connection !== original.connection)
        }
        var explicitDefault = lease({socketPath:original.connection.socketPath})
        compare(explicitDefault.connection, original.connection)
    }
    function test_changing_placement_releases_only_its_lease() {
        var laptop = lease()
        var external = lease()
        var link = laptop.connection
        laptop.module = "replacement"
        verify(laptop.connection !== link)
        compare(external.connection, link)
        verify(link.connected)
        laptop.module = "audio"
        compare(laptop.connection, link)
    }
    function test_absent_panel_does_not_open_a_connection() {
        var panel = lease({surface:""})
        compare(panel.connection, null)
        panel.surface = "panel"
        verify(panel.connection !== null)
        var key = panel.heldKey
        panel.surface = ""
        compare(panel.connection, null)
        compare(Omarchy.PlacementConnections.entries[key], undefined)
    }
}
