import QtQuick
import com.yap.native

// A settings row whose control is the switch (SettingsSwitch in an inline
// row), named by the row. `toggled` asks for the flip; `checked` shows the
// stored value.
SettingsRow {
    id: row

    property bool checked: false
    signal toggled()

    layout: "inline"

    SettingSwitch {
        checked: row.checked
        enabled: !row.disabled
        Accessible.name: row.shownLabel
        Accessible.description: row.shownDesc
        onClicked: row.toggled()
    }
}
