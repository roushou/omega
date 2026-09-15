import QtQuick

// Control feedback belongs to one instance even when its transport serves several.
QtObject {
    id: session
    property Navigation navigation: Navigation {}
    required property var connection
    required property var snapshot
    readonly property var identity: snapshot ? snapshot.instance : null
    readonly property string prefix: identity ? identity.id + "/" : ""
    readonly property var requests: QtObject {
        id: feedback
        signal settled(string key, bool success)
        property string error: ""
        function busy(key) { return session.prefix !== "" && session.connection.requests.busy(session.prefix + key) }
    }
    property Connections completions: Connections {
        target: session.connection ? session.connection.requests : null
        function onSettled(key, success) {
            if (!session.prefix || key.indexOf(session.prefix) !== 0) return
            feedback.error = success ? "" : session.connection.requests.error
            feedback.settled(key.substring(session.prefix.length), success)
        }
    }
    function press(bound, value, key, event) {
        var admitted = session.connection.interact(session.identity,
            session.snapshot && session.snapshot.view ? session.snapshot.view.revision || "0" : "0", key, event, value)
        feedback.error = admitted ? "" : session.connection.requests.error
        return admitted
    }
}
