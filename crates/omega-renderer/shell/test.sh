#!/usr/bin/env sh
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/crates/omega-renderer/shell" "$scratch/crates/omega-omarchy"
cp -r "$root/crates/omega-renderer/shell/tests" "$scratch/crates/omega-renderer/shell/"
cp -r "$root/crates/omega-omarchy/shell" "$scratch/crates/omega-omarchy/"
ln -s "$root/crates/omega-renderer/shell/core" "$scratch/crates/omega-renderer/shell/core"
ln -s "$root/crates/omega-renderer/shell/core" "$scratch/crates/omega-omarchy/shell/core"
ln -s "$root/crates/omega-renderer/shell/preview" "$scratch/crates/omega-renderer/shell/preview"
ln -s "$root/crates/omega-renderer/shell/fixtures" "$scratch/crates/omega-renderer/shell/fixtures"
runner=${QMLTESTRUNNER:-/usr/lib/qt6/bin/qmltestrunner}
QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
QT_QUICK_CONTROLS_STYLE=Basic QT_STYLE_OVERRIDE=Fusion \
"$runner" -import "$(dirname "$0")/tests/imports" -input "$scratch/crates/omega-renderer/shell/tests"
