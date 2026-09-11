#!/usr/bin/env sh
set -eu
runner=${QMLTESTRUNNER:-/usr/lib/qt6/bin/qmltestrunner}
QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
QT_QUICK_CONTROLS_STYLE=Basic QT_STYLE_OVERRIDE=Fusion \
"$runner" -import "$(dirname "$0")/tests/imports" -input "$(dirname "$0")/tests"
