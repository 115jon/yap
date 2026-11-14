pragma Singleton
import QtQuick
import com.yap.native
import Yap.Ui

// The settings dialog's shared state (SettingsContext): which panel shows,
// the search query, the row a search hit or a deep link landed on, and the
// requests to open the dialog, the profile editor and the theme preview.
// The map (labels, descriptions, sections) is `Settings.schema()`, read
// once (yap-core settings.rs, the web's schema.ts).
QtObject {
    id: nav

    readonly property var schema: JSON.parse(Settings.schema())
    readonly property var panels: {
        const map = {}
        for (const panel of schema.panels)
            map[panel.id] = panel
        return map
    }
    readonly property var rows: {
        const map = {}
        for (const panel of schema.panels) {
            for (const row of panel.rail_rows)
                map[row.id] = row
            for (const section of panel.sections)
                for (const row of section.rows)
                    map[row.id] = row
        }
        return map
    }
    readonly property var sections: {
        const map = {}
        for (const panel of schema.panels)
            for (const section of panel.sections)
                map[section.id] = section
        return map
    }

    property bool open: false
    property string panel: "account"
    property string query: ""
    property string target: ""
    // The theme preview sheet hides the dialog without closing it.
    property bool previewing: false
    // The last settings link shown (Invites state `settingsSeq`).
    property int linkSeq: 0

    readonly property string trimmed: query.trim()
    readonly property var hits: trimmed.length > 0 ? JSON.parse(Settings.search(trimmed)) : []

    signal profileEditorRequested()

    // A theme's name (Theme.themeCatalog), Midnight for an unknown id.
    function themeName(id) {
        return Theme.themeLabel(id) || qsTr("Midnight")
    }

    // Camera capture presets (CAMERA_QUALITY_PROFILES), "720p, 30 fps".
    readonly property list<string> qualityIds: ["480p30", "720p30", "720p60", "1080p30", "1080p60"]
    function qualityName(id) {
        const match = /^(\d+p)(\d+)$/.exec(id)
        return match ? qsTr("%1, %2 fps").arg(match[1]).arg(match[2]) : id
    }

    function show(panelId, rowId) {
        panel = panels[panelId] ? panelId : "account"
        target = rowId ?? ""
        query = ""
        previewing = false
        open = true
    }
    function close() {
        open = false
        previewing = false
        query = ""
        target = ""
    }
    function go(panelId, rowId) {
        panel = panelId
        target = rowId ?? ""
        if (target.length > 0)
            query = ""
    }
    function label(rowId) {
        return rows[rowId]?.label ?? ""
    }
    function desc(rowId) {
        return rows[rowId]?.desc ?? ""
    }
    // The row is shown for the current search.
    function matches(rowId) {
        return trimmed.length === 0 || Settings.rowMatches(rowId, trimmed)
    }
    // Any of the section's rows is shown.
    function sectionShown(sectionId) {
        const section = sections[sectionId]
        if (!section)
            return false
        return section.rows.some(row => matches(row.id))
    }
    function hitsOn(panelId) {
        return hits.filter(hit => hit.panel === panelId)
    }

    // The query's words marked in `text` (Highlight): accent tint, underlined.
    function highlight(text) {
        const words = trimmed.toLowerCase().split(/\s+/).filter(w => w.length > 0)
        const escape = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        if (words.length === 0)
            return escape(text)
        const pattern = new RegExp("(" + words.map(w => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|") + ")", "ig")
        const tint = Theme.solid(Theme.accentTint)
        return text.split(pattern).map((part, i) => i % 2 === 1
            ? "<span style=\"background-color:" + tint + "; text-decoration:underline;\">" + escape(part) + "</span>"
            : escape(part)).join("")
    }
}
