import QtQuick
import com.yap.native

// Label over value (Deadlock.tsx Stat): silkscreen label, the value in
// 18 px numbers with an optional small unit ("%", "k").
Column {
    id: stat

    property string label: ""
    property string value: ""
    property string unit: ""
    required property var dl

    spacing: 2
    Accessible.role: Accessible.StaticText
    Accessible.name: label + " " + value + unit

    Text {
        width: stat.width
        textFormat: Text.PlainText
        text: stat.label
        elide: Text.ElideRight
        color: Theme.ink3
        font: Theme.type.silk
    }
    Row {
        spacing: 2
        Text {
            id: valueText
            height: 22
            verticalAlignment: Text.AlignVCenter
            textFormat: Text.PlainText
            text: stat.value
            color: Theme.ink1
            font: stat.dl.statFont
        }
        Text {
            visible: stat.unit.length > 0
            height: 22
            verticalAlignment: Text.AlignBottom
            bottomPadding: 3
            textFormat: Text.PlainText
            text: stat.unit
            color: Theme.ink3
            font: stat.dl.unitFont
        }
    }
}
