import QtQuick

// Registry lifetime is one instance, so identical node keys in other windows cannot match.
QtObject {
    property var sources: ({})
    function registerSource(key, source) { var next = Object.assign({}, sources); next[key] = source; sources = next }
    function forgetSource(key, source) { if (sources[key] !== source) return; var next = Object.assign({}, sources); delete next[key]; sources = next }
    function ready(control) {
        for (var key in sources) {
            var source = sources[key]
            if (source && resolve(key, source.navigationTarget) === control && !source.navigationReady) return false
        }
        return true
    }
    property var controls: ({})
    function register(key, control) { var next = Object.assign({}, controls); next[key] = control; controls = next }
    function forget(key, control) { if (controls[key] !== control) return; var next = Object.assign({}, controls); delete next[key]; controls = next }
    function resolve(from, target) {
        if (sources[from] && sources[from].navigationResolved) return controls[target] || null
        var split = from.lastIndexOf("/")
        var scoped = split < 0 ? target : from.substring(0, split + 1) + target.replace(/~/g, "~0").replace(/\//g, "~1")
        return controls[scoped] || null
    }
    function move(from, target, direction) { var control = resolve(from, target); if (control) control.move(direction) }
    function activate(from, target) { var control = resolve(from, target); if (control) control.activate(control.selected) }
}
