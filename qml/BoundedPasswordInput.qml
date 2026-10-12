import QtQuick
import Quickshell
import Quickshell.Io

// Password entry that is deliberately NOT a Qt text editor.
//
// TextInput/TextField read the full foreign clipboard text on every clipboard
// change (QQuickTextInput::q_canPasteChanged) without a byte limit or deadline,
// and TextEdit has no echo mode: it exposes the plain secret to the input
// method (surrounding text), the Wayland primary selection and accessibility.
// This item keeps the secret in a private string, never accepts input-method
// events, has no selection or copy path, renders only a bullet mask, and
// routes paste through a bounded, deadlined wl-paste process.
FocusScope {
    id: control

    property string text: ""
    property int maxLength: 256
    property int maxPasteBytes: 4096
    property string placeholderText: ""
    property color maskColor: "#c0caf5"
    property color placeholderTextColor: "#666666"
    property color cursorColor: maskColor
    property alias font: mask.font
    property int leftPadding: 0
    property int rightPadding: 0
    property string pasteNotice: ""

    signal accepted()

    implicitHeight: Math.max(mask.implicitHeight, 20)
    activeFocusOnTab: true

    Accessible.role: Accessible.EditableText
    Accessible.passwordEdit: true
    Accessible.name: placeholderText

    function appendClean(raw) {
        var clean = String(raw || "").replace(/[\u0000-\u001f\u007f]/g, "");
        var room = Math.max(0, maxLength - text.length);
        if (clean.length > room) clean = clean.substring(0, room);
        if (clean.length > 0) text = text + clean;
    }

    function startBoundedPaste() {
        if (pasteProcess.running) return;
        control.pasteNotice = "";
        pasteProcess.collectedBytes = 0;
        pasteProcess.collectedText = "";
        pasteProcess.overflowed = false;
        pasteProcess.command = ["timeout", "--signal=KILL", "1", "wl-paste", "--no-newline", "--type", "text/plain;charset=utf-8"];
        pasteProcess.running = true;
    }

    Process {
        id: pasteProcess
        property int collectedBytes: 0
        property string collectedText: ""
        property bool overflowed: false

        stdout: SplitParser {
            splitMarker: ""
            onRead: (chunk) => {
                if (pasteProcess.overflowed) {
                    pasteProcess.running = false;
                    return;
                }
                pasteProcess.collectedBytes += chunk.length;
                if (pasteProcess.collectedBytes > control.maxPasteBytes) {
                    pasteProcess.overflowed = true;
                    pasteProcess.running = false;
                    return;
                }
                pasteProcess.collectedText += chunk;
            }
        }

        onExited: (exitCode) => {
            var pasted = collectedText;
            collectedText = "";
            if (overflowed || exitCode === 124 || exitCode === 137) {
                control.pasteNotice = "Clipboard content exceeded byte limit or timed out.";
                return;
            }
            if (exitCode !== 0 || pasted.length === 0) return;
            control.appendClean(pasted);
        }
    }

    // Key handling without input-method participation (Item does not set
    // ItemAcceptsInputMethod), so the secret is never sent to an IME.
    Item {
        id: keySink
        anchors.fill: parent
        focus: true

        Keys.onPressed: (event) => {
            event.accepted = true;
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                control.accepted();
                return;
            }
            if (event.matches(StandardKey.Paste) ||
                (event.modifiers & Qt.ShiftModifier && event.key === Qt.Key_Insert)) {
                control.startBoundedPaste();
                return;
            }
            if (event.key === Qt.Key_Backspace) {
                if (event.modifiers & Qt.ControlModifier) control.text = "";
                else if (control.text.length > 0) control.text = control.text.slice(0, -1);
                return;
            }
            if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab || event.key === Qt.Key_Escape) {
                event.accepted = false;
                return;
            }
            // Printable characters, including Shift-modified ones; Ctrl/Alt/Meta
            // shortcuts (copy, cut, select-all, ...) are swallowed without effect.
            if (event.text.length > 0 && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
                control.appendClean(event.text);
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        cursorShape: Qt.IBeamCursor
        onPressed: (mouse) => {
            mouse.accepted = true;
            keySink.forceActiveFocus();
        }
    }

    Text {
        id: mask
        anchors.fill: parent
        anchors.leftMargin: control.leftPadding
        anchors.rightMargin: control.rightPadding
        verticalAlignment: Text.AlignVCenter
        textFormat: Text.PlainText
        clip: true
        color: control.text.length > 0 ? control.maskColor : control.placeholderTextColor
        text: control.text.length > 0 ? "\u2022".repeat(control.text.length) : (keySink.activeFocus ? "" : control.placeholderText)
        elide: control.text.length > 0 ? Text.ElideNone : Text.ElideRight
    }

    Rectangle {
        visible: keySink.activeFocus
        width: 1
        height: Math.max(12, mask.font.pixelSize > 0 ? mask.font.pixelSize + 2 : 14)
        anchors.verticalCenter: parent.verticalCenter
        x: control.leftPadding + Math.min(mask.contentWidth, control.width - control.leftPadding - control.rightPadding - 1) * (control.text.length > 0 ? 1 : 0)
        color: control.cursorColor
    }
}
