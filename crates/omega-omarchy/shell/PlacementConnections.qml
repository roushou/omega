pragma Singleton
import QtQuick

QtObject {
    id: pool
    property var entries: ({})
    property Component factory: Component { Connection {} }

    function acquire(key) {
        var entry = entries[key]
        if (!entry) {
            var scope = JSON.parse(key)
            var connection = factory.createObject(pool, {
                socketPath: scope[0], unit: scope[1], surface: scope[2], module: scope[3]
            })
            if (!connection) throw new Error("Could not create placement connection")
            entry = { connection: connection, users: 0 }
            entries[key] = entry
        }
        entry.users++
        return entry.connection
    }

    function release(key) {
        var entry = entries[key]
        if (!entry) throw new Error("Unknown placement connection")
        if (--entry.users === 0) {
            delete entries[key]
            entry.connection.destroy()
        }
    }
}
