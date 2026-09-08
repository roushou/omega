#!/usr/bin/env sh
# Lint the QML this crate ships.
#
# The renderer is the one part of Omega no compiler sees. `cargo test` pins the
# props the QML reads and the file list it carries; nothing reads the QML
# itself. Five bugs reached the tree that way — a binding loop, two properties
# shadowing ones `Item` already has, a leftover signal, and a `var` shadowing
# the model it was reading, which broke a list's whole reconcile.
#
# Four categories are off, and deliberately. Every one of them fires on
# something legal, and a warning that always fires trains people to ignore the
# tool:
#
#   import, unresolved-type, inheritance-cycle
#       Omarchy's `qs.Ui` and `qs.Commons` are mapped by Quickshell itself, not
#       by an import path qmllint can be given. Everything reached through them
#       reads as missing, including our own base types.
#   unqualified, missing-property
#       A delegate loaded by url reaching a file-scope id, and a property read
#       through `Loader.item`, which is a QObject until it loads. Both are the
#       shape this renderer is built on.
#
# What is left still catches the whole class the five bugs came from:
# shadowed and hoisted identifiers, duplicate bindings, incompatible types,
# deprecations, and syntax.
set -eu

lint=${QMLLINT:-$(command -v qmllint || echo /usr/lib/qt6/bin/qmllint)}
if [ ! -x "$lint" ]; then
    echo "qmllint not found; set QMLLINT to it" >&2
    exit 127
fi

find "$(dirname "$0")/plugins" -name '*.qml' -print0 | xargs -0 "$lint" \
    --max-warnings 0 \
    --import disable \
    --unresolved-type disable \
    --inheritance-cycle disable \
    --unqualified disable \
    --missing-property disable
