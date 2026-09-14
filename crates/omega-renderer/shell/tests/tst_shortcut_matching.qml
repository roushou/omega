import QtQuick
import QtTest
import "../core/Keyboard.js" as Keyboard
import "KeyboardCases.js" as Corpus

TestCase {
    name: "ShortcutMatching"
    function test_rust_conformance() {
        var identities = Corpus.identities()
        for (var index = 0; index < identities.length; ++index)
            compare(Keyboard.identity(identities[index].qt), identities[index].identity)
        var cases = Corpus.cases()
        for (var i = 0; i < cases.length; ++i) {
            var c = cases[i]
            compare(Keyboard.identity(c.qt), c.identity, "identity case " + i)
            compare(Keyboard.matches(c.binding, Keyboard.identity(c.qt), c.modifiers, c.release, c.repeat), c.matches, "matching case " + i)
        }
    }
    function test_native_modifier_translation() {
        compare(Keyboard.modifiers(Qt.ControlModifier | Qt.ShiftModifier | Qt.KeypadModifier), 3)
        compare(Keyboard.modifiers(Qt.GroupSwitchModifier), 16)
        compare(Keyboard.modifiers(Qt.AltModifier | Qt.MetaModifier), 12)
        compare(Keyboard.identity(Qt.Key_Backtab), "key:Tab")
        compare(Keyboard.identity(Qt.Key_Enter), "key:Enter")
        compare(Keyboard.identity(Qt.Key_unknown), "")
    }
}
