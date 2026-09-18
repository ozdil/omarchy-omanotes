import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "ozdil.omanotes"
  ipcTarget: "ozdil.omanotes"
  manageIpc: false

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  // State properties
  property var allNotes: []
  property int totalNotes: 0
  property int pinnedNotesCount: 0
  property int archivedNotesCount: 0
  property bool e2eeEnabled: false
  property var cloudConfig: ({})

  // UI Filter State (No keyboard needed)
  property string filterMode: "all" // "all", "pinned", "checklist", "yellow", "green", "blue", "purple", "red", "teal"
  property string searchQuery: ""
  property string activeView: "notes" // "notes", "archived", "settings"
  property string toastMsg: ""
  readonly property string fontFamily: (root.bar && root.bar.fontFamily) ? root.bar.fontFamily : ((typeof Style !== "undefined" && Style.font && Style.font.family) ? Style.font.family : "JetBrainsMono Nerd Font, JetBrains Mono, monospace")

  // New note creation state
  property bool isCreatingChecklist: false
  property string newNoteTitle: ""
  property string newNoteContent: ""
  property string newNoteColor: "yellow"

  // Settings edit state
  property string e2eeInputPassword: ""
  property string cloudProviderInput: "rclone"
  property string cloudRemoteInput: "gdrive:OmarchyNotes"
  property string cloudGitInput: ""
  property bool isSyncing: false

  // Stdin framing payload buffers (cleared immediately after handoff)
  property string pendingActionPayload: ""
  property string pendingSyncPayload: ""

  function resolveEnginePath() {
    return Qt.resolvedUrl("omanotes-engine").toString().replace(/^file:\/\//, "")
  }

  function showToast(msg) {
    root.toastMsg = msg
    toastTimer.restart()
  }

  function refresh() {
    if (!statusProc.running) {
      statusProc.running = true
    }
  }

  // --- Keyboardless Actions ---
  function addFromClipboard(color) {
    actionProc.command = [root.resolveEnginePath(), "--add-clipboard", color || root.newNoteColor]
    actionProc.running = true
    root.showToast("📋 Added note from clipboard")
  }

  function addTemplate(type) {
    actionProc.command = [root.resolveEnginePath(), "--add-template", type]
    actionProc.running = true
    var label = (type === "shopping" ? "Grocery List" : (type === "daily" ? "Daily Priorities" : (type === "idea" ? "New Idea" : "Reminder")))
    root.showToast("➕ Created " + label)
  }

  function duplicateNote(id) {
    if (!id) return
    actionProc.command = [root.resolveEnginePath(), "--duplicate", id]
    actionProc.running = true
    root.showToast("Note duplicated")
  }

  function clearCompleted(id) {
    if (!id) return
    actionProc.command = [root.resolveEnginePath(), "--clear-completed", id]
    actionProc.running = true
    root.showToast("Completed items cleared")
  }

  function deleteNote(id) {
    if (!id) return
    actionProc.command = [root.resolveEnginePath(), "--delete", id]
    actionProc.running = true
    root.showToast("Note deleted")
  }

  function togglePin(id) {
    if (!id) return
    actionProc.command = [root.resolveEnginePath(), "--toggle-pin", id]
    actionProc.running = true
  }

  function toggleArchive(id) {
    if (!id) return
    actionProc.command = [root.resolveEnginePath(), "--toggle-archive", id]
    actionProc.running = true
    root.showToast("Note archive updated")
  }

  function setNoteColor(id, color) {
    if (!id || !color) return
    actionProc.command = [root.resolveEnginePath(), "--set-color", id, color]
    actionProc.running = true
  }

  function toggleCheckItem(noteId, itemId) {
    if (!noteId || !itemId) return
    actionProc.command = [root.resolveEnginePath(), "--toggle-check", noteId, itemId]
    actionProc.running = true
  }

  function addManualNote() {
    var title = newNoteTitle.trim()
    var content = newNoteContent.trim()
    if (!title && !content) return

    var payload = {
      title: title || "Quick Note",
      content: content,
      color: root.newNoteColor,
      is_checklist: root.isCreatingChecklist,
      tags: []
    }
    root.pendingActionPayload = JSON.stringify(payload)
    actionProc.command = [root.resolveEnginePath(), "--add"]
    actionProc.running = true

    newNoteTitle = ""
    newNoteContent = ""
    root.showToast("Note saved")
  }

  function saveE2EEPassword() {
    var pwd = root.e2eeInputPassword
    root.pendingActionPayload = JSON.stringify({ password: pwd })
    actionProc.command = [root.resolveEnginePath(), "--set-e2ee"]
    actionProc.running = true
    root.e2eeInputPassword = ""
    root.showToast(pwd ? "E2EE Master Password Updated" : "E2EE Encryption Disabled")
  }

  function saveCloudSettings() {
    var payload = {
      provider: root.cloudProviderInput,
      rclone_remote: root.cloudRemoteInput,
      git_remote: root.cloudGitInput,
      auto_sync: false
    }
    root.pendingActionPayload = JSON.stringify(payload)
    actionProc.command = [root.resolveEnginePath(), "--set-cloud"]
    actionProc.running = true
    root.showToast("Cloud configuration saved")
  }

  function triggerCloudSync() {
    if (root.isSyncing) return
    root.isSyncing = true
    var pwd = root.e2eeInputPassword
    root.pendingSyncPayload = JSON.stringify({ password: pwd })
    syncProc.command = [root.resolveEnginePath(), "--sync"]
    syncProc.running = true
    root.e2eeInputPassword = ""
    root.showToast("Syncing with cloud...")
  }

  function getCardBg(colorKey) {
    switch (colorKey) {
      case "yellow": return "#2e2412"
      case "green":  return "#16281b"
      case "blue":   return "#142332"
      case "purple": return "#28162d"
      case "red":    return "#2e1618"
      case "teal":   return "#122a2a"
      default:       return Color.surface
    }
  }

  function getCardBorder(colorKey) {
    switch (colorKey) {
      case "yellow": return "#8a6624"
      case "green":  return "#2d6f3e"
      case "blue":   return "#2a5a84"
      case "purple": return "#6f2d79"
      case "red":    return "#792d34"
      case "teal":   return "#276b6b"
      default:       return Color.border
    }
  }

  function isNoteVisible(note) {
    if (note.archived) return false

    // Filter mode checks
    if (root.filterMode === "pinned" && !note.pinned) return false
    if (root.filterMode === "checklist" && !note.is_checklist) return false
    if (["yellow", "green", "blue", "purple", "red", "teal"].indexOf(root.filterMode) !== -1) {
      if (note.color !== root.filterMode) return false
    }

    // Optional text search check
    if (root.searchQuery !== "") {
      var q = root.searchQuery.toLowerCase()
      var t = (note.title || "").toLowerCase()
      var c = (note.content || "").toLowerCase()
      return t.indexOf(q) !== -1 || c.indexOf(q) !== -1
    }
    return true
  }

  IpcHandler {
    target: "ozdil.omanotes"
    function open() { root.open() }
    function close() { root.close() }
    function show() { root.open() }
    function hide() { root.close() }
    function toggle() { root.toggle() }
    function refresh() { root.refresh() }
    function sync() { root.triggerCloudSync() }
    function paste() { root.addFromClipboard("yellow") }
  }

  Process {
    id: statusProc
    command: [root.resolveEnginePath(), "--status"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try {
          var cleanText = String(text || "").slice(0, 262144)
          var d = JSON.parse(cleanText)
          if (d.success) {
            root.allNotes = d.notes || []
            root.totalNotes = d.total_notes || 0
            root.pinnedNotesCount = d.pinned_notes || 0
            root.archivedNotesCount = d.archived_notes || 0
            root.e2eeEnabled = !!d.e2ee_enabled
            root.cloudConfig = d.cloud || ({})
            if (d.cloud) {
              root.cloudProviderInput = d.cloud.provider || "rclone"
              root.cloudRemoteInput = d.cloud.rclone_remote || "gdrive:OmarchyNotes"
              root.cloudGitInput = d.cloud.git_remote || ""
            }
          }
        } catch (e) {}
      }
    }
  }

  Process {
    id: actionProc
    stdinEnabled: true
    onStarted: {
      if (root.pendingActionPayload) {
        actionProc.write(root.pendingActionPayload + "\n")
        root.pendingActionPayload = ""
      }
    }
    onExited: {
      root.refresh()
    }
  }

  Process {
    id: syncProc
    stdinEnabled: true
    onStarted: {
      if (root.pendingSyncPayload) {
        syncProc.write(root.pendingSyncPayload + "\n")
        root.pendingSyncPayload = ""
      }
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.isSyncing = false
        try {
          var cleanText = String(text || "").slice(0, 65536)
          var d = JSON.parse(cleanText)
          root.showToast(d.message || "Sync completed")
        } catch (e) {
          root.showToast("Sync completed")
        }
        root.refresh()
      }
    }
  }

  Timer {
    id: toastTimer
    interval: 3500
    onTriggered: root.toastMsg = ""
  }

  Timer {
    id: autoRefreshTimer
    interval: 10000
    repeat: true
    running: root.opened
    onTriggered: root.refresh()
  }

  Component.onCompleted: refresh()

  Component.onDestruction: {
    if (statusProc.running) statusProc.running = false
    if (actionProc.running) actionProc.running = false
    if (syncProc.running) syncProc.running = false
    if (autoRefreshTimer.running) autoRefreshTimer.running = false
    if (toastTimer.running) toastTimer.running = false
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󰠮"
    useActiveColor: false
    foreground: root.e2eeEnabled ? Color.accent : (root.bar ? root.bar.foreground : Color.foreground)
    tooltipText: "OmaNotes: " + root.totalNotes + " notes" + (root.e2eeEnabled ? " (E2EE Protected)" : "")
    onPressed: function(b) {
      root.toggle()
    }
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    contentWidth: panel.fittedContentWidth(Style.space(660))
    contentHeight: panel.fittedContentHeight(panelColumn.implicitHeight + Style.space(24), Style.space(840))

    ScrollView {
      id: scrollArea
      anchors.fill: parent
      clip: true
      ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
      ScrollBar.vertical.policy: panelColumn.implicitHeight > height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff

      Column {
        id: panelColumn
        width: scrollArea.availableWidth
        spacing: Style.space(14)

        // ==================== Top Header ====================
        Item {
          width: parent.width
          implicitHeight: Math.max(heroLabels.implicitHeight, headerActions.implicitHeight)

          Column {
            id: heroLabels
            anchors.left: parent.left
            anchors.right: headerActions.left
            anchors.rightMargin: Style.space(10)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(2)

            Row {
              spacing: Style.space(8)

              Text {
                textFormat: Text.PlainText
                text: "OmaNotes"
                color: root.bar ? root.bar.foreground : Color.foreground
                font.family: root.fontFamily
                font.pixelSize: Style.font.title
                font.bold: true
              }

              BorderSurface {
                anchors.verticalCenter: parent.verticalCenter
                radius: Style.cornerRadius
                color: root.e2eeEnabled ? Style.selectedFillFor(Color.foreground, Color.accent) : "transparent"
                borderSpec: Border.controlSpec(root.e2eeEnabled ? "selected" : "normal", Color.foreground, Color.accent)
                implicitHeight: Style.space(18)
                implicitWidth: e2eeBadgeText.implicitWidth + Style.space(10)

                Text {
                  id: e2eeBadgeText
                  textFormat: Text.PlainText
                  anchors.centerIn: parent
                  text: root.e2eeEnabled ? "E2EE ENCRYPTED" : "LOCAL"
                  color: root.e2eeEnabled ? Color.accent : Color.muted
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption - 1
                  font.bold: true
                }
              }
            }

            Text {
              textFormat: Text.PlainText
              text: root.cloudConfig.last_sync_msg || "Zero-Knowledge Google Keep for Omarchy Linux"
              color: Color.muted
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              elide: Text.ElideRight
              width: parent.width
            }
          }

          Row {
            id: headerActions
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(6)

            Button {
              text: "☕"
              tooltipText: "Buy Me a Coffee"
              foreground: "#FFDD00"
              fontSize: Style.font.caption
              bordered: true
              onClicked: Qt.openUrlExternally("https://buymeacoffee.com/ozdil")
            }

            Button {
              text: root.isSyncing ? "Syncing..." : "Sync"
              iconText: "󰓦"
              enabled: !root.isSyncing
              onClicked: root.triggerCloudSync()
            }

            Button {
              text: root.activeView === "settings" ? "Notes" : "Settings"
              iconText: root.activeView === "settings" ? "󰠮" : ""
              onClicked: {
                root.activeView = (root.activeView === "settings" ? "notes" : "settings")
              }
            }
          }
        }

        // ==================== Toast Message ====================
        BorderSurface {
          width: parent.width
          visible: root.toastMsg !== ""
          radius: Style.cornerRadius
          color: Style.selectedFillFor(Color.foreground, Color.accent)
          borderSpec: Border.controlSpec("selected", Color.foreground, Color.accent)
          implicitHeight: Style.space(32)

          Text {
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: root.toastMsg
            color: Color.accent
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            font.bold: true
          }
        }

        // ==================== SETTINGS VIEW ====================
        Column {
          width: parent.width
          visible: root.activeView === "settings"
          spacing: Style.space(12)

          PanelSectionHeader {
            text: "Zero-Knowledge End-to-End Encryption (E2EE)"
            width: parent.width
          }

          Text {
            width: parent.width
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: "Notes are encrypted locally with AES-256-GCM before being sent to the cloud. If you lose your master password, encrypted backups cannot be recovered."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }

          RowLayout {
            width: parent.width
            spacing: Style.space(8)

            TextField {
              id: e2eePwdField
              Layout.fillWidth: true
              placeholderText: "Enter E2EE Master Password..."
              text: root.e2eeInputPassword
              echoMode: TextInput.Password
              onTextChanged: root.e2eeInputPassword = text
            }

            Button {
              text: "Save Key"
              onClicked: root.saveE2EEPassword()
            }
          }

          PanelSeparator { width: parent.width }

          PanelSectionHeader {
            text: "Cloud Sync Provider"
            width: parent.width
          }

          RowLayout {
            width: parent.width
            spacing: Style.space(8)

            Button {
              text: "Google Drive / rclone"
              selected: root.cloudProviderInput === "rclone"
              onClicked: root.cloudProviderInput = "rclone"
            }

            Button {
              text: "Private Git Repo"
              selected: root.cloudProviderInput === "git"
              onClicked: root.cloudProviderInput = "git"
            }

            Button {
              text: "Local Only (Disabled)"
              selected: root.cloudProviderInput === "none"
              onClicked: root.cloudProviderInput = "none"
            }
          }

          Column {
            width: parent.width
            spacing: Style.space(6)
            visible: root.cloudProviderInput === "rclone"

            Text {
              textFormat: Text.PlainText
              text: "rclone Remote Target (e.g. gdrive:OmarchyNotes or nextcloud:Notes)"
              color: Color.muted
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }

            TextField {
              width: parent.width
              text: root.cloudRemoteInput
              onTextChanged: root.cloudRemoteInput = text
            }
          }

          Column {
            width: parent.width
            spacing: Style.space(6)
            visible: root.cloudProviderInput === "git"

            Text {
              textFormat: Text.PlainText
              text: "Local Git Repository Directory (commits notes.enc)"
              color: Color.muted
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }

            TextField {
              width: parent.width
              text: root.cloudGitInput
              placeholderText: "/home/ozdil/Documents/notes-vault"
              onTextChanged: root.cloudGitInput = text
            }
          }

          Button {
            text: "Save Cloud Configuration"
            onClicked: root.saveCloudSettings()
          }
        }

        // ==================== MAIN NOTES VIEW ====================
        Column {
          width: parent.width
          visible: root.activeView === "notes"
          spacing: Style.space(12)

          // ---------- 1-TAP KEYBOARDLESS ACTION BAR ----------
          RowLayout {
            width: parent.width
            spacing: Style.space(8)

            // Primary: Paste from Clipboard as Note (Zero Typing!)
            Button {
              Layout.fillWidth: true
              text: "📋 Paste from Clipboard"
              iconText: "󰅌"
              onClicked: root.addFromClipboard("yellow")
            }

            // Quick templates
            Button {
              text: "🛒 Groceries"
              onClicked: root.addTemplate("shopping")
            }

            Button {
              text: "📅 Priorities"
              onClicked: root.addTemplate("daily")
            }

            Button {
              text: "💡 Idea"
              onClicked: root.addTemplate("idea")
            }
          }

          // ---------- KEYBOARDLESS FILTER CHIPS ----------
          Flow {
            width: parent.width
            spacing: Style.space(6)

            Button {
              text: "All (" + root.totalNotes + ")"
              selected: root.filterMode === "all"
              onClicked: root.filterMode = "all"
            }

            Button {
              text: "📌 Pinned (" + root.pinnedNotesCount + ")"
              selected: root.filterMode === "pinned"
              onClicked: root.filterMode = "pinned"
            }

            Button {
              text: "󰄲 Checklist"
              selected: root.filterMode === "checklist"
              onClicked: root.filterMode = "checklist"
            }

            // Color chips for mouse-driven filtering
            Row {
              spacing: Style.space(4)

              Repeater {
                model: ["yellow", "green", "blue", "purple", "red", "teal"]
                delegate: Rectangle {
                  width: Style.space(22)
                  height: Style.space(22)
                  radius: Style.space(11)
                  color: root.getCardBg(modelData)
                  border.color: root.filterMode === modelData ? Color.accent : root.getCardBorder(modelData)
                  border.width: root.filterMode === modelData ? 2 : 1

                  MouseArea {
                    anchors.fill: parent
                    onClicked: {
                      root.filterMode = (root.filterMode === modelData ? "all" : modelData)
                    }
                  }
                }
              }
            }
          }

          // ---------- MANUAL "TAKE A NOTE..." BOX ----------
          BorderSurface {
            width: parent.width
            radius: Style.cornerRadius
            color: root.getCardBg(root.newNoteColor)
            borderSpec: Border.controlSpec("normal", Color.foreground, root.getCardBorder(root.newNoteColor))
            implicitHeight: newNoteCol.implicitHeight + Style.space(16)

            Column {
              id: newNoteCol
              anchors.fill: parent
              anchors.margins: Style.space(10)
              spacing: Style.space(8)

              TextField {
                width: parent.width
                placeholderText: "Title..."
                text: root.newNoteTitle
                onTextChanged: root.newNoteTitle = text
              }

              TextField {
                width: parent.width
                placeholderText: root.isCreatingChecklist ? "Checklist items (one per line)..." : "Take a note..."
                text: root.newNoteContent
                onTextChanged: root.newNoteContent = text
              }

              RowLayout {
                width: parent.width

                Row {
                  spacing: Style.space(4)

                  Repeater {
                    model: ["yellow", "green", "blue", "purple", "red", "teal"]
                    delegate: Rectangle {
                      width: Style.space(18)
                      height: Style.space(18)
                      radius: Style.space(9)
                      color: root.getCardBg(modelData)
                      border.color: root.newNoteColor === modelData ? Color.accent : root.getCardBorder(modelData)
                      border.width: root.newNoteColor === modelData ? 2 : 1

                      MouseArea {
                        anchors.fill: parent
                        onClicked: root.newNoteColor = modelData
                      }
                    }
                  }
                }

                Item { Layout.fillWidth: true }

                Button {
                  text: root.isCreatingChecklist ? "󰄲 List" : "󰠮 Note"
                  selected: root.isCreatingChecklist
                  onClicked: root.isCreatingChecklist = !root.isCreatingChecklist
                }

                Button {
                  text: "Save"
                  iconText: "󰐕"
                  onClicked: root.addManualNote()
                }
              }
            }
          }

          // ---------- PINNED SECTION ----------
          Column {
            width: parent.width
            spacing: Style.space(8)
            visible: {
              if (root.filterMode !== "all" && root.filterMode !== "pinned") return false
              for (var i = 0; i < root.allNotes.length; i++) {
                if (root.allNotes[i].pinned && root.isNoteVisible(root.allNotes[i])) return true
              }
              return false
            }

            PanelSectionHeader {
              text: "PINNED"
              width: parent.width
            }

            Flow {
              width: parent.width
              spacing: Style.space(10)

              Repeater {
                model: root.allNotes
                delegate: Item {
                  width: (parent.width - Style.space(10)) / 2
                  implicitHeight: cardSurface.implicitHeight
                  visible: modelData.pinned && root.isNoteVisible(modelData)

                  BorderSurface {
                    id: cardSurface
                    width: parent.width
                    radius: Style.cornerRadius
                    color: root.getCardBg(modelData.color)
                    borderSpec: Border.controlSpec("normal", Color.foreground, root.getCardBorder(modelData.color))
                    implicitHeight: cardCol.implicitHeight + Style.space(16)

                    Column {
                      id: cardCol
                      anchors.fill: parent
                      anchors.margins: Style.space(10)
                      spacing: Style.space(6)

                      RowLayout {
                        width: parent.width

                        Text {
                          Layout.fillWidth: true
                          textFormat: Text.PlainText
                          text: modelData.title || "Untitled"
                          font.family: Style.font.family
                          font.pixelSize: Style.font.body
                          font.bold: true
                          color: Color.foreground
                          elide: Text.ElideRight
                        }

                        Button {
                          bordered: false
                          text: "󰤱"
                          onClicked: root.togglePin(modelData.id)
                        }
                      }

                      // Checklist view (Interactive checkboxes)
                      Column {
                        width: parent.width
                        spacing: Style.space(4)
                        visible: modelData.is_checklist && modelData.checklist_items && modelData.checklist_items.length > 0

                        Repeater {
                          model: modelData.checklist_items || []
                          delegate: RowLayout {
                            width: parent.width
                            spacing: Style.space(6)

                            Button {
                              bordered: false
                              text: modelData.checked ? "󰄲" : "󰄱"
                              onClicked: root.toggleCheckItem(cardCol.parent.parent.parent.modelData.id, modelData.id)
                            }

                            Text {
                              Layout.fillWidth: true
                              textFormat: Text.PlainText
                              text: modelData.text || ""
                              font.family: Style.font.family
                              font.pixelSize: Style.font.caption
                              font.strikeout: modelData.checked
                              color: modelData.checked ? Color.muted : Color.foreground
                              elide: Text.ElideRight
                            }
                          }
                        }
                      }

                      // Plaintext view
                      Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: modelData.content || ""
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                        color: Color.foreground
                        wrapMode: Text.Wrap
                        visible: !modelData.is_checklist && modelData.content !== ""
                      }

                      // Bottom actions (Mouse-only tools)
                      RowLayout {
                        width: parent.width

                        Row {
                          spacing: Style.space(3)
                          Repeater {
                            model: ["yellow", "green", "blue", "purple", "red", "teal"]
                            delegate: Rectangle {
                              width: Style.space(12)
                              height: Style.space(12)
                              radius: Style.space(6)
                              color: root.getCardBg(modelData)
                              border.color: root.getCardBorder(modelData)
                              MouseArea {
                                anchors.fill: parent
                                onClicked: root.setNoteColor(cardCol.parent.parent.parent.modelData.id, modelData)
                              }
                            }
                          }
                        }

                        Item { Layout.fillWidth: true }

                        Button {
                          bordered: false
                          text: "󰆏"
                          onClicked: root.duplicateNote(modelData.id)
                        }

                        Button {
                          bordered: false
                          text: "󰅖"
                          onClicked: root.deleteNote(modelData.id)
                        }
                      }
                    }
                  }
                }
              }
            }
          }

          // ---------- OTHER NOTES SECTION ----------
          Column {
            width: parent.width
            spacing: Style.space(8)

            PanelSectionHeader {
              text: root.filterMode === "pinned" ? "PINNED" : "NOTES"
              width: parent.width
            }

            Flow {
              width: parent.width
              spacing: Style.space(10)

              Repeater {
                model: root.allNotes
                delegate: Item {
                  width: (parent.width - Style.space(10)) / 2
                  implicitHeight: otherCardSurface.implicitHeight
                  visible: {
                    if (root.filterMode === "pinned") return false
                    return !modelData.pinned && root.isNoteVisible(modelData)
                  }

                  BorderSurface {
                    id: otherCardSurface
                    width: parent.width
                    radius: Style.cornerRadius
                    color: root.getCardBg(modelData.color)
                    borderSpec: Border.controlSpec("normal", Color.foreground, root.getCardBorder(modelData.color))
                    implicitHeight: otherCardCol.implicitHeight + Style.space(16)

                    Column {
                      id: otherCardCol
                      anchors.fill: parent
                      anchors.margins: Style.space(10)
                      spacing: Style.space(6)

                      RowLayout {
                        width: parent.width

                        Text {
                          Layout.fillWidth: true
                          textFormat: Text.PlainText
                          text: modelData.title || "Untitled"
                          font.family: Style.font.family
                          font.pixelSize: Style.font.body
                          font.bold: true
                          color: Color.foreground
                          elide: Text.ElideRight
                        }

                        Button {
                          bordered: false
                          text: modelData.pinned ? "󰤱" : "󰤰"
                          onClicked: root.togglePin(modelData.id)
                        }
                      }

                      // Checklist items
                      Column {
                        width: parent.width
                        spacing: Style.space(4)
                        visible: modelData.is_checklist && modelData.checklist_items && modelData.checklist_items.length > 0

                        Repeater {
                          model: modelData.checklist_items || []
                          delegate: RowLayout {
                            width: parent.width
                            spacing: Style.space(6)

                            Button {
                              bordered: false
                              text: modelData.checked ? "󰄲" : "󰄱"
                              onClicked: root.toggleCheckItem(otherCardCol.parent.parent.parent.modelData.id, modelData.id)
                            }

                            Text {
                              Layout.fillWidth: true
                              textFormat: Text.PlainText
                              text: modelData.text || ""
                              font.family: Style.font.family
                              font.pixelSize: Style.font.caption
                              font.strikeout: modelData.checked
                              color: modelData.checked ? Color.muted : Color.foreground
                              elide: Text.ElideRight
                            }
                          }
                        }
                      }

                      // Plaintext content
                      Text {
                        width: parent.width
                        textFormat: Text.PlainText
                        text: modelData.content || ""
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                        color: Color.foreground
                        wrapMode: Text.Wrap
                        visible: !modelData.is_checklist && modelData.content !== ""
                      }

                      // Bottom actions (Mouse-only tools)
                      RowLayout {
                        width: parent.width

                        Row {
                          spacing: Style.space(3)
                          Repeater {
                            model: ["yellow", "green", "blue", "purple", "red", "teal"]
                            delegate: Rectangle {
                              width: Style.space(12)
                              height: Style.space(12)
                              radius: Style.space(6)
                              color: root.getCardBg(modelData)
                              border.color: root.getCardBorder(modelData)
                              MouseArea {
                                anchors.fill: parent
                                onClicked: root.setNoteColor(otherCardCol.parent.parent.parent.modelData.id, modelData)
                              }
                            }
                          }
                        }

                        Item { Layout.fillWidth: true }

                        Button {
                          bordered: false
                          text: "󰆏"
                          onClicked: root.duplicateNote(modelData.id)
                        }

                        Button {
                          bordered: false
                          text: "󰅖"
                          onClicked: root.deleteNote(modelData.id)
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
