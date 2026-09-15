import QtQuick

Item {
    property var bar: null
    property string moduleName: ""
    property var settings: ({})
    function setting(name, fallback) {
        return settings[name] === undefined ? fallback : settings[name]
    }
}
