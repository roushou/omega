import QtQuick
// No real sockets: tests deliver daemon messages through Connection.onLine.
QtObject {
    property string path: ""
    property bool connected: false
    property QtObject parser
    property string written: ""
    signal error(int error)
    function write(text) { written += text }
}
