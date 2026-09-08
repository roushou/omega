#!/usr/bin/env sh
# Lint the QML this crate ships.
#
# The renderer is the one part of Omega no compiler sees. `cargo test` pins the
# props the QML reads and the file list it carries; nothing reads the QML
# itself. Five bugs reached the tree that way — a binding loop, two properties
# shadowing ones `Item` already has, a leftover signal, and a `var` shadowing
# the model it was reading, which broke a list's whole reconcile.
#
# **What we fail on is a list, not what we ignore.** qmllint's flags differ
# across Qt minors — the first version of this script passed `--max-warnings`
# and two category switches that the runner's Qt had never heard of, so the
# job failed on the job rather than on the QML. Naming the categories that
# matter works on any version: one that does not emit a category simply never
# matches, and one that emits a category we have not listed is noise we were
# going to ignore anyway.
#
# Three categories are deliberately absent, and every one of them fires on
# something legal:
#
#   import, unresolved-type, inheritance-cycle
#       Omarchy's `qs.Ui` and `qs.Commons` are mapped by Quickshell itself,
#       not by an import path qmllint can be given. Everything reached through
#       them reads as missing, including our own base types.
#   unqualified, missing-property
#       A delegate loaded by url reaching a file-scope id, and a property read
#       through `Loader.item`, which is a QObject until it loads. Both are the
#       shape this renderer is built on.
set -eu

lint=${QMLLINT:-$(command -v qmllint || echo /usr/lib/qt6/bin/qmllint)}
if [ ! -x "$lint" ]; then
    echo "qmllint not found; set QMLLINT to it" >&2
    exit 127
fi

# The problems worth a red build: anything that is a mistake rather than a
# thing this shell's dynamic loading makes unprovable.
# Mistakes, not opinions. `deprecated` and `unused-imports` are left out
# deliberately: what a given Qt calls deprecated moves between minors, and a
# red build that depends on which Ubuntu the runner is on teaches people to
# ignore the build.
fatal='\[(syntax|var-used-before-declaration|required|incompatible-type|duplicate-property-binding|duplicated-name|duplicate-import|duplicate-enum-entries|duplicate-inline-component)\]'

found=$(find "$(dirname "$0")/plugins" -name '*.qml' | sort)
# qmllint's own exit code is not the signal: it varies by version, and it
# counts warnings we have chosen not to care about.
report=$(printf '%s\n' "$found" | xargs "$lint" 2>&1 || true)

if printf '%s\n' "$report" | grep -qE "$fatal"; then
    printf '%s\n' "$report" | grep -E -B2 "$fatal" >&2
    exit 1
fi
