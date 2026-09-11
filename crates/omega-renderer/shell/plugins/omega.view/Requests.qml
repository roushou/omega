import QtQuick

// Pending invocations retain identity and time, never arguments or form secrets.
QtObject {
    id: requests
    signal settled(string key, bool success)
    property var pending: ({})
    property string error: ""

    function busy(key) {
        var entries = requests.pending
        for (var stream in entries) {
            if (entries[stream].key === key) return true
        }
        return false
    }

    function begin(stream, key, now) {
        if (requests.busy(key)) return false
        if (Object.keys(requests.pending).length >= 16) {
            requests.error = "Too many commands are waiting."
            return false
        }
        var next = Object.assign({}, requests.pending)
        next[stream] = { key: key, started: now }
        requests.pending = next
        requests.error = ""
        return true
    }

    function finish(stream, result) {
        if (!requests.pending[stream] || !result.done) return
        var key = requests.pending[stream].key
        var next = Object.assign({}, requests.pending)
        delete next[stream]
        requests.pending = next
        if (result.error) requests.error = result.error.message || "Command refused."
        requests.settled(key, !result.error)
    }

    function disconnected() {
        if (Object.keys(requests.pending).length)
            requests.error = "Connection lost; command outcome is unknown."
        requests.pending = ({})
    }

    function expire(now) {
        // Reconnect instead of releasing uncertain work on a live connection.
        for (var stream in requests.pending) {
            if (now - requests.pending[stream].started >= 30000) {
                requests.error = "Command timed out; it may still be running."
                return true
            }
        }
        return false
    }
}
