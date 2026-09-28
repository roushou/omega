import QtQuick
import QtTest
import "../core" as Renderer
import "../core/Props.js" as Props
import "fixtures/UiContract.js" as Fixtures

TestCase {
    id: test
    name: "UiContract"
    when: windowShown
    visible: true
    width: 500
    height: 500
    property var emitted: null
    Renderer.Requests { id: requestState }
    QtObject {
        id: testSession
        readonly property var requests: requestState
        function press(bound, value, key, event, model) {
            test.emitted = Props.encodeEvent(model, event, value)
            return test.emitted !== null
        }
    }
    function init() { failOnWarning(/.*/) }
    function test_every_native_event_uses_canonical_protobuf_json_data() {
        return Fixtures.cases.map(function(item) { return {tag: item.kind + "/" + item.event, item: item} })
    }
    function test_every_native_event_uses_canonical_protobuf_json(data) {
        var item = data.item
        var actual = Props.encodeEvent({type:item.kind}, item.event, item.absent ? undefined : item.input)
        if (item.absent) compare(actual, undefined)
        else compare(JSON.parse(JSON.stringify(actual)), item.encoded)
    }
    function test_payload_encoding_rejects_bad_shapes_and_preserves_fractional_values() {
        compare(Props.encodeEvent({type:"button"}, "press", 1), null)
        compare(Props.encodeEvent({type:"slider"}, "change", "1"), null)
        compare(Props.encodeEvent({type:"slider"}, "change", 2), null)
        compare(Props.encodeEvent({type:"slider"}, "change", NaN), null)
        compare(Props.encodeEvent({type:"slider"}, "change", Infinity), null)
        compare(Props.encodeEvent({type:"slider"}, "change", 1), {doubleValue:1})
        compare(Props.encodeEvent({type:"field"}, "change", {text:"x",revision:1}), null)
        compare(Props.encodeEvent({type:"field"}, "change", {text:"x",revision:1.5,reset:0}), null)
        compare(Props.encodeEvent({type:"field"}, "change", {text:"x",revision:4294967296,reset:0}), null)
        compare(Props.encodeEvent({type:"form"}, "submit", {count:42}), null)
        compare(Props.encodeEvent({type:"unknown"}, "press", undefined), null)
        compare(Props.encodeEvent({type:"text",shortcuts:[{event:"escape"}]}, "escape", undefined), undefined)
        var gesture = {zoom:1,offset_x:0.5,offset_y:-3,x:10,y:20,dx:0,dy:0}
        var encoded = Props.encodeEvent({type:"viewport"}, "wheel", gesture)
        compare(encoded.map.entries.offset_x, {doubleValue:0.5})
        compare(encoded.map.entries.x, {doubleValue:10})
        var sparse = Props.encodeEvent({type:"viewport"}, "wheel", {})
        compare(sparse.map.entries.zoom, {doubleValue:1})
        compare(sparse.map.entries.dy, {doubleValue:0})
        compare(Props.encodeEvent({type:"viewport"}, "wheel", {zoom:"invalid"}), null)
    }
    Component { id: factory; Renderer.ViewNode { session: testSession } }
    function test_every_declared_node_kind_has_a_renderer_data() {
        return Fixtures.nodes.map(function(kind) { return {tag:kind,kind:kind} })
    }
    function test_every_declared_node_kind_has_a_renderer(data) {
        var root = createTemporaryObject(factory,test,{model:{type:data.kind,key:"node"}})
        verify(root !== null)
        tryVerify(function() { return root.children[0].item !== null })
    }
    function test_native_edit_revisions_preserve_unsigned_range_data() {
        return [{tag:"field",kind:"field"}, {tag:"textarea",kind:"textarea"}]
    }
    function test_native_edit_revisions_preserve_unsigned_range(data) {
        test.emitted = null
        var root = createTemporaryObject(factory,test,{model:{type:data.kind,key:"edit",props:{
            controlled:{boolValue:true},value:{stringValue:""},
            edit_revision:{intValue:"2147483648"},reset_revision:{intValue:"4294967294"}
        },events:{change:{local:"1"}}}})
        var editor = findChild(root,"editor")
        editor.forceActiveFocus()
        keyClick(Qt.Key_A)
        tryVerify(function() { return test.emitted !== null })
        compare(test.emitted.map.entries.revision, {intValue:"2147483649"})
        compare(test.emitted.map.entries.reset, {intValue:"4294967294"})
    }
    function test_viewport_fit_and_pointer_anchor_data() {
        return [{tag:"contain",fit:"contain",w:300,h:150},
            {tag:"cover",fit:"cover",w:400,h:200},
            {tag:"fill",fit:"fill",w:300,h:200},
            {tag:"none",fit:"none",w:100,h:50}]
    }
    function test_viewport_fit_and_pointer_anchor(data) {
        var root = createTemporaryObject(factory,test,{model:{type:"viewport",key:"canvas",props:{
            width:{intValue:"300"},height:{intValue:"200"},fit:{stringValue:data.fit}
        },children:[{type:"spacer",key:"content",props:{width:{intValue:"100"},height:{intValue:"50"}}}]}})
        var layer = findChild(root,"viewportLayer")
        var top = layer.mapToItem(root,0,0)
        var bottom = layer.mapToItem(root,100,50)
        fuzzyCompare(bottom.x-top.x,data.w,0.001)
        fuzzyCompare(bottom.y-top.y,data.h,0.001)
        var viewport = layer.parent
        var anchor = viewport.contentAt(70,80)
        viewport.zoomTo(2,70,80)
        var after = layer.mapToItem(root,anchor.x,anchor.y)
        fuzzyCompare(after.x,70,0.001)
        fuzzyCompare(after.y,80,0.001)
    }
}
