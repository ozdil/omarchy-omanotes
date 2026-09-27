import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "theme"

Item {
    id: root

    property int totalNotes: 0
    property int pinnedNotes: 0
    property int archivedNotes: 0
    property bool e2eeEnabled: false
    property var notes: []
    property var selectedNote: null
    property string filterMode: "active" // "active", "pinned", "archived"
    property string searchQuery: ""

    readonly property string enginePath: {
        var base = Qt.resolvedUrl(".").toString().replace(/^file:\/\//, "");
        var parent = base.replace(/\/qml\/?$/, "");
        return parent + "/omanotes-engine";
    }

    readonly property string statusPath: {
        var base = Qt.resolvedUrl(".").toString().replace(/^file:\/\//, "");
        var parent = base.replace(/\/qml\/?$/, "");
        return parent + "/omanotes-status";
    }

    readonly property var displayNotes: {
        if (!root.notes || root.notes.length === 0) return [];
        var list = [];
        var query = root.searchQuery.toLowerCase().trim();

        for (var i = 0; i < root.notes.length; i++) {
            var n = root.notes[i];
            if (!n) continue;

            if (root.filterMode === "active" && n.archived) continue;
            if (root.filterMode === "pinned" && (!n.pinned || n.archived)) continue;
            if (root.filterMode === "archived" && !n.archived) continue;

            if (query.length > 0) {
                var titleMatch = (n.title || "").toLowerCase().indexOf(query) !== -1;
                var contentMatch = (n.content || "").toLowerCase().indexOf(query) !== -1;
                if (!titleMatch && !contentMatch) continue;
            }

            list.push(n);
        }
        return list;
    }

    function refresh() {
        if (!statusProc.running) {
            statusProc.running = true;
        }
    }

    function togglePin(id) {
        actionProc.command = [root.enginePath, "--toggle-pin", id];
        actionProc.running = true;
    }

    function toggleArchive(id) {
        actionProc.command = [root.enginePath, "--toggle-archive", id];
        actionProc.running = true;
    }

    function deleteNote(id) {
        actionProc.command = [root.enginePath, "--delete", id];
        actionProc.running = true;
        root.selectedNote = null;
    }

    function duplicateNote(id) {
        actionProc.command = [root.enginePath, "--duplicate", id];
        actionProc.running = true;
    }

    function toggleItem(noteId, itemId) {
        actionProc.command = [root.enginePath, "--toggle-item", noteId, itemId];
        actionProc.running = true;
    }

    function addTemplate(type) {
        actionProc.command = [root.enginePath, "--add-template", type];
        actionProc.running = true;
    }

    Process {
        id: statusProc
        command: [root.statusPath]
        stdout: StdioCollector {
            waitForEnd: true
            onStreamFinished: {
                try {
                    var data = JSON.parse(text || "{}");
                    root.totalNotes = Number(data.total_notes) || 0;
                    root.pinnedNotes = Number(data.pinned_notes) || 0;
                    root.archivedNotes = Number(data.archived_notes) || 0;
                    root.e2eeEnabled = !!data.e2ee_enabled;
                    root.notes = data.notes || [];

                    // Reselect current note if updated
                    if (root.selectedNote) {
                        for (var i = 0; i < root.notes.length; i++) {
                            if (root.notes[i].id === root.selectedNote.id) {
                                root.selectedNote = root.notes[i];
                                break;
                            }
                        }
                    } else if (root.notes.length > 0) {
                        root.selectedNote = root.notes[0];
                    }
                } catch(e) {
                    console.warn("Failed to parse omanotes status:", e);
                }
            }
        }
    }

    Process {
        id: actionProc
        onExited: root.refresh()
    }

    Component.onCompleted: refresh()

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 14

        // Header Bar
        Rectangle {
            Layout.fillWidth: true
            height: 68
            radius: Theme.radiusMd
            color: Theme.bgSurface
            border.color: Theme.border
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                spacing: 12

                Text {
                    text: Theme.iconNote
                    font.family: Theme.iconFont
                    font.pixelSize: 24
                    color: Theme.accent
                }

                ColumnLayout {
                    spacing: 2
                    Text {
                        text: "OMANOTES ENCRYPTED VAULT"
                        font.family: Theme.fontFamily
                        font.pixelSize: 15
                        font.bold: true
                        color: Theme.textMain
                    }
                    Text {
                        text: "Argon2id & AES-256-GCM Secure Markdown & Checklist Notes"
                        font.family: Theme.fontFamily
                        font.pixelSize: 11
                        color: Theme.textMuted
                    }
                }

                Item { Layout.fillWidth: true }

                // E2EE Pill
                Rectangle {
                    height: 32
                    implicitWidth: e2eeRow.implicitWidth + 20
                    radius: Theme.radiusSm
                    color: Theme.bgCard
                    border.color: Theme.border
                    border.width: 1

                    RowLayout {
                        id: e2eeRow
                        anchors.centerIn: parent
                        spacing: 8

                        Text {
                            text: root.e2eeEnabled ? Theme.iconLock : Theme.iconUnlock
                            font.family: Theme.iconFont
                            font.pixelSize: 12
                            color: root.e2eeEnabled ? Theme.accentSuccess : Theme.accentWarning
                        }

                        Text {
                            text: root.e2eeEnabled ? "E2EE ENCRYPTED" : "STANDARD VAULT"
                            font.family: Theme.monoFont
                            font.pixelSize: 10
                            font.bold: true
                            color: Theme.textMain
                        }
                    }
                }

                // Add Template Dropdown / Action
                Rectangle {
                    height: 34
                    implicitWidth: addRow.implicitWidth + 20
                    radius: Theme.radiusSm
                    color: addArea.containsMouse ? Theme.bgCardHover : Theme.bgCard
                    border.color: Theme.border
                    border.width: 1

                    RowLayout {
                        id: addRow
                        anchors.centerIn: parent
                        spacing: 6

                        Text {
                            text: Theme.iconPlus
                            font.family: Theme.iconFont
                            font.pixelSize: 12
                            color: Theme.accentSuccess
                        }
                        Text {
                            text: "New Checklist"
                            font.family: Theme.fontFamily
                            font.pixelSize: 11
                            color: Theme.textMain
                        }
                    }

                    MouseArea {
                        id: addArea
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.addTemplate("daily")
                    }
                }

                // Refresh Button
                Rectangle {
                    width: 36
                    height: 36
                    radius: Theme.radiusSm
                    color: refreshArea.containsMouse ? Theme.bgCardHover : Theme.bgCard
                    border.color: Theme.border
                    border.width: 1

                    Text {
                        anchors.centerIn: parent
                        text: Theme.iconRefresh
                        font.family: Theme.iconFont
                        font.pixelSize: 14
                        color: Theme.textMain
                    }

                    MouseArea {
                        id: refreshArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.refresh()
                    }
                }
            }
        }

        // Main Layout: Master-Detail
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 14

            // Left Sidebar: Filters, Search & Note List
            Rectangle {
                Layout.preferredWidth: 380
                Layout.fillHeight: true
                radius: Theme.radiusMd
                color: Theme.bgSurface
                border.color: Theme.border
                border.width: 1

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 10

                    // Filters & Search
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 6

                        Repeater {
                            model: [
                                { id: "active", name: "Active" },
                                { id: "pinned", name: "Pinned" },
                                { id: "archived", name: "Archived" }
                            ]

                            delegate: Rectangle {
                                height: 26
                                Layout.fillWidth: true
                                radius: Theme.radiusSm
                                color: root.filterMode === modelData.id ? Theme.bgCardHover : Theme.bgCard
                                border.color: root.filterMode === modelData.id ? Theme.borderLight : Theme.border
                                border.width: 1

                                Text {
                                    anchors.centerIn: parent
                                    text: modelData.name
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 10
                                    font.bold: root.filterMode === modelData.id
                                    color: root.filterMode === modelData.id ? Theme.textMain : Theme.textMuted
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.filterMode = modelData.id
                                }
                            }
                        }
                    }

                    // Search Field
                    Rectangle {
                        Layout.fillWidth: true
                        height: 32
                        radius: Theme.radiusSm
                        color: Theme.bgCard
                        border.color: noteSearchInput.activeFocus ? Theme.borderLight : Theme.border
                        border.width: 1

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            anchors.rightMargin: 8
                            spacing: 6

                            Text {
                                text: Theme.iconSearch
                                font.family: Theme.iconFont
                                font.pixelSize: 12
                                color: Theme.textMuted
                            }

                            TextInput {
                                id: noteSearchInput
                                Layout.fillWidth: true
                                font.family: Theme.fontFamily
                                font.pixelSize: 11
                                color: Theme.textMain
                                clip: true
                                onTextChanged: root.searchQuery = text

                                Text {
                                    anchors.fill: parent
                                    text: "Search notes and items..."
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 11
                                    color: Theme.textDim
                                    visible: !noteSearchInput.text && !noteSearchInput.activeFocus
                                }
                            }
                        }
                    }

                    // Notes List
                    ListView {
                        id: notesListView
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        spacing: 6
                        model: root.displayNotes

                        delegate: Rectangle {
                            width: notesListView.width
                            height: 64
                            radius: Theme.radiusSm
                            color: root.selectedNote && root.selectedNote.id === modelData.id ? Theme.bgCardHover : Theme.bgCard
                            border.color: root.selectedNote && root.selectedNote.id === modelData.id ? Theme.accent : Theme.border
                            border.width: 1

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.selectedNote = modelData
                            }

                            ColumnLayout {
                                anchors.fill: parent
                                anchors.margins: 10
                                spacing: 4

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 6

                                    Text {
                                        text: modelData.is_checklist ? Theme.iconList : Theme.iconNote
                                        font.family: Theme.iconFont
                                        font.pixelSize: 12
                                        color: Theme.accent
                                    }

                                    Text {
                                        Layout.fillWidth: true
                                        text: modelData.title || "Untitled Note"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: 12
                                        font.bold: true
                                        color: Theme.textMain
                                        elide: Text.ElideRight
                                    }

                                    Text {
                                        visible: !!modelData.pinned
                                        text: Theme.iconPin
                                        font.family: Theme.iconFont
                                        font.pixelSize: 11
                                        color: Theme.accentWarning
                                    }
                                }

                                Text {
                                    Layout.fillWidth: true
                                    text: modelData.is_checklist ? ((modelData.checklist_items ? modelData.checklist_items.length : 0) + " items") : (modelData.content || "Empty content")
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 11
                                    color: Theme.textMuted
                                    elide: Text.ElideRight
                                }
                            }
                        }

                        Text {
                            anchors.centerIn: parent
                            visible: root.displayNotes.length === 0
                            text: "No notes found."
                            font.family: Theme.fontFamily
                            font.pixelSize: 12
                            color: Theme.textMuted
                        }
                    }
                }
            }

            // Right Detail Area: Note Editor & Checklist Inspector
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: Theme.radiusMd
                color: Theme.bgSurface
                border.color: Theme.border
                border.width: 1

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 16
                    spacing: 12
                    visible: root.selectedNote !== null

                    // Title & Actions Toolbar
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Text {
                            Layout.fillWidth: true
                            text: root.selectedNote ? (root.selectedNote.title || "Untitled Note") : ""
                            font.family: Theme.fontFamily
                            font.pixelSize: 15
                            font.bold: true
                            color: Theme.textMain
                            elide: Text.ElideRight
                        }

                        // Toggle Pin
                        Rectangle {
                            width: 32
                            height: 32
                            radius: Theme.radiusSm
                            color: Theme.bgCard
                            border.color: Theme.border
                            border.width: 1

                            Text {
                                anchors.centerIn: parent
                                text: Theme.iconPin
                                font.family: Theme.iconFont
                                font.pixelSize: 12
                                color: (root.selectedNote && root.selectedNote.pinned) ? Theme.accentWarning : Theme.textMuted
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: if (root.selectedNote) root.togglePin(root.selectedNote.id)
                            }
                        }

                        // Toggle Archive
                        Rectangle {
                            width: 32
                            height: 32
                            radius: Theme.radiusSm
                            color: Theme.bgCard
                            border.color: Theme.border
                            border.width: 1

                            Text {
                                anchors.centerIn: parent
                                text: Theme.iconArchive
                                font.family: Theme.iconFont
                                font.pixelSize: 12
                                color: (root.selectedNote && root.selectedNote.archived) ? Theme.accent : Theme.textMuted
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: if (root.selectedNote) root.toggleArchive(root.selectedNote.id)
                            }
                        }

                        // Duplicate Note
                        Rectangle {
                            width: 32
                            height: 32
                            radius: Theme.radiusSm
                            color: Theme.bgCard
                            border.color: Theme.border
                            border.width: 1

                            Text {
                                anchors.centerIn: parent
                                text: Theme.iconCopy
                                font.family: Theme.iconFont
                                font.pixelSize: 12
                                color: Theme.textMuted
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: if (root.selectedNote) root.duplicateNote(root.selectedNote.id)
                            }
                        }

                        // Delete Note
                        Rectangle {
                            width: 32
                            height: 32
                            radius: Theme.radiusSm
                            color: Theme.bgCard
                            border.color: Theme.border
                            border.width: 1

                            Text {
                                anchors.centerIn: parent
                                text: Theme.iconTrash
                                font.family: Theme.iconFont
                                font.pixelSize: 12
                                color: Theme.accentDanger
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: if (root.selectedNote) root.deleteNote(root.selectedNote.id)
                            }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        height: 1
                        color: Theme.border
                    }

                    // Checklist view if note is checklist
                    ListView {
                        id: checklistListView
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        spacing: 8
                        visible: root.selectedNote && root.selectedNote.is_checklist
                        model: (root.selectedNote && root.selectedNote.checklist_items) ? root.selectedNote.checklist_items : []

                        delegate: Rectangle {
                            width: checklistListView.width
                            height: 38
                            radius: Theme.radiusSm
                            color: Theme.bgCard
                            border.color: Theme.border
                            border.width: 1

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    if (root.selectedNote) {
                                        root.toggleItem(root.selectedNote.id, modelData.id);
                                    }
                                }
                            }

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 12
                                anchors.rightMargin: 12
                                spacing: 10

                                Text {
                                    text: modelData.checked ? Theme.iconCheckSquare : Theme.iconSquare
                                    font.family: Theme.iconFont
                                    font.pixelSize: 14
                                    color: modelData.checked ? Theme.accentSuccess : Theme.textMuted
                                }

                                Text {
                                    Layout.fillWidth: true
                                    text: modelData.text || ""
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                    font.strikeout: !!modelData.checked
                                    color: modelData.checked ? Theme.textMuted : Theme.textMain
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }

                    // Content TextArea if standard note
                    ScrollView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        visible: root.selectedNote && !root.selectedNote.is_checklist
                        clip: true

                        TextArea {
                            text: (root.selectedNote && root.selectedNote.content) ? root.selectedNote.content : ""
                            font.family: Theme.fontFamily
                            font.pixelSize: 13
                            color: Theme.textMain
                            wrapMode: Text.WordWrap
                            readOnly: true
                            background: Rectangle { color: "transparent" }
                        }
                    }
                }

                Text {
                    anchors.centerIn: parent
                    visible: root.selectedNote === null
                    text: "Select a note from the left panel to inspect and manage."
                    font.family: Theme.fontFamily
                    font.pixelSize: 13
                    color: Theme.textMuted
                }
            }
        }
    }
}
