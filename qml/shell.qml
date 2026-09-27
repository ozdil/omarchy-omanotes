import QtQuick
import Quickshell
import Quickshell.Io
import "theme"

ShellRoot {
    id: root

    FloatingWindow {
        id: win
        title: "OmaNotes - Encrypted Markdown & Checklist Notes"
        implicitWidth: 1060
        implicitHeight: 700
        color: Theme.bgBase

        MainWindow {
            id: mainWin
            anchors.fill: parent
        }
    }

    IpcHandler {
        target: "ozdil.omanotes"

        function toggle(): bool {
            win.visible = !win.visible;
            return win.visible;
        }

        function show(): bool {
            win.visible = true;
            return true;
        }

        function hide(): bool {
            win.visible = false;
            return false;
        }

        function refresh(): string {
            mainWin.refresh();
            return "OK";
        }
    }
}
