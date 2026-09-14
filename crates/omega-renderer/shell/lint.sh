#!/usr/bin/env sh
# Lint shipped QML using diagnostic categories supported across Qt versions.
# Host-provided qs imports and dynamic Loader properties cannot be resolved here.
# Import resolution, unqualified access, deprecation, and unused-import warnings
# are excluded from the fatal categories below.
set -eu

lint=${QMLLINT:-$(command -v qmllint || echo /usr/lib/qt6/bin/qmllint)}
if [ ! -x "$lint" ]; then
    echo "qmllint not found; set QMLLINT to it" >&2
    exit 127
fi

# Treat structural and type errors as failures.
fatal='\[(syntax|var-used-before-declaration|required|incompatible-type|duplicate-property-binding|duplicated-name|duplicate-import|duplicate-enum-entries|duplicate-inline-component)\]'

found=$(find "$(dirname "$0")/core" "$(dirname "$0")/fixtures" "$(dirname "$0")/desktop" "$(dirname "$0")/preview" "$(dirname "$0")/shell.qml" "$(dirname "$0")/../../omega-omarchy/shell" -name '*.qml' | sort)
# Filter diagnostics explicitly; qmllint exit codes include non-fatal categories.
report=$(printf '%s\n' "$found" | xargs "$lint" 2>&1 || true)

if printf '%s\n' "$report" | grep -qE "$fatal"; then
    printf '%s\n' "$report" | grep -E -B2 "$fatal" >&2
    exit 1
fi
