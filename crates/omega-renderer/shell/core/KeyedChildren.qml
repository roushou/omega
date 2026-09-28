import QtQml.Models

// Sibling keys and kinds define delegate lifetime. Payload changes update retained
// delegates; removal or a change of kind ends their local state.
ListModel {
    id: children
    dynamicRoles: true

    function reconcile(wanted) {
        for (var i = children.count - 1; i >= 0; i--) {
            var held = children.get(i)
            var retained = false
            for (var w = 0; w < wanted.length; w++) {
                if ((wanted[w].key || "") === held.key && wanted[w].type === held.node.type) {
                    retained = true
                    break
                }
            }
            if (!retained) children.remove(i)
        }

        for (var target = 0; target < wanted.length; target++) {
            var key = wanted[target].key || ""
            var payload = JSON.stringify(wanted[target])
            var found = -1
            for (var j = target; j < children.count; j++) {
                if (children.get(j).key === key && children.get(j).node.type === wanted[target].type) {
                    found = j
                    break
                }
            }
            if (found === -1) {
                children.insert(target, {key: key, node: wanted[target], payload: payload})
            } else {
                if (found !== target) children.move(found, target, 1)
                // Bindings and descendants are part of the payload, not just props.
                if (children.get(target).payload !== payload) {
                    children.setProperty(target, "node", wanted[target])
                    children.setProperty(target, "payload", payload)
                }
            }
        }
        while (children.count > wanted.length) children.remove(children.count - 1)
    }
}
