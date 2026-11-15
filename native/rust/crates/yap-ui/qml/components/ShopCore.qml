pragma ComponentBehavior: Bound
import QtQuick
import Yap.Ui

// What the collectibles shop and the profile editor's pickers do, for both
// apps (ShopView.tsx, ShopItemDetail.tsx, CollectiblePicker.tsx). Not
// visual: the desktop's shop page, detail dialog and picker workbench and
// the phone's pages each hold one and keep their own layout. The catalog,
// the rows, the variants and the equipped marks are the core's (store/
// shop.rs, through Shop); this holds what QML did around them:
// - the page: its state parsed (`page`), the open collection, the type
//   tabs with their counts (`tabs`) and the freshness line;
// - the detail: `detail` (null when none is open) and its keys, Equip or
//   Remove for the shown variant (`equipDetail`, `removeDetail`);
// - the picker (with `picks`): its model (`picker`, a stand-in while none
//   is open), whether the catalog is in, the "No <kind>" option, the
//   sections as one list (`groups`, the none option first while nothing is
//   searched) and every option in reading order (`order`), choosing
//   (`choose`, a draft only) and Apply (`apply`: an equip, or a remove for
//   the none option). `applied()` fires once an Apply lands without an
//   error; the app closes its picker then.
// Everything is free to equip: there are no payments anywhere.
QtObject {
    id: core

    // Parse the picker too (the picker's own instance; the shop page's
    // leaves it, so an open picker is not parsed twice).
    property bool picks: false

    // ---------- The page.
    // `{loaded, loading, error, freshness, counts, tab, query, collection,
    // busy, actionError, picker, pickerTitle, pickerQuery}`.
    readonly property var page: JSON.parse(Shop.state)
    readonly property var collection: page.collection ?? null
    readonly property bool loaded: page.loaded === true
    readonly property string tab: page.tab ?? "featured"
    readonly property string query: page.query ?? ""
    // A picker is open (its detail answers, not the shop page's).
    readonly property bool pickerOpen: (page.picker ?? "").length > 0
    readonly property string freshnessLine: (page.freshness ?? "").length > 0
        ? qsTr("Every collectible is free to equip · %1").arg(page.freshness)
        : qsTr("Every collectible is free to equip")
    // The type tabs (ShopView.tsx TABS); Featured has no count.
    readonly property var tabs: [
        { value: "featured", label: qsTr("Featured"), count: -1 },
        { value: "avatar_decoration", label: qsTr("Decorations"), count: page.counts?.avatar_decoration ?? 0 },
        { value: "profile_effect", label: qsTr("Effects"), count: page.counts?.profile_effect ?? 0 },
        { value: "nameplate", label: qsTr("Nameplates"), count: page.counts?.nameplate ?? 0 },
        { value: "profile_frame", label: qsTr("Frames"), count: page.counts?.profile_frame ?? 0 }
    ]
    // The four slots, as the editor's rows name them.
    readonly property var kinds: ["avatar_decoration", "profile_effect", "profile_frame", "nameplate"]
    function kindLabel(kind) {
        return kind === "avatar_decoration" ? qsTr("Avatar decoration")
            : kind === "profile_effect" ? qsTr("Profile effect")
            : kind === "profile_frame" ? qsTr("Profile frame")
            : kind === "nameplate" ? qsTr("Nameplate") : ""
    }

    function open() { Shop.open() }
    function setTab(value) { Shop.setTab(value) }
    function setQuery(text) { Shop.setQuery(text) }
    // A collection's page; empty for back to the shop.
    function openCollection(id) { Shop.openCollection(id ?? "") }

    // ---------- The detail.
    // `{id, kind, kindOne, name, collection, description, status, tone,
    // variants, variantName, price, error, busy, equipped, bg, deco, still,
    // plate, effect, frame}`, or null.
    readonly property var detail: JSON.parse(Shop.detail)
    readonly property bool detailOpen: detail !== null
    function openDetail(id) { Shop.openDetail(id) }
    function closeDetail() { Shop.openDetail("") }
    function selectVariant(id) { Shop.selectVariant(id) }
    function equipDetail() {
        if (detail !== null && !detail.busy)
            Shop.equip(detail.id)
    }
    function removeDetail() {
        if (detail !== null && !detail.busy)
            Shop.remove(detail.kind)
    }
    // Empties a slot at once (the editor's Remove).
    function remove(kind) { Shop.remove(kind) }

    // ---------- The picker.
    readonly property var pickerModel: picks ? JSON.parse(Shop.picker) : null
    readonly property var picker: pickerModel ?? ({ title: "", sections: [], chips: [], preview: null, kind: "" })
    readonly property string pickerKind: picker.kind ?? ""
    // The store sends `total` once the catalog is in.
    readonly property bool hasCatalog: picker.total !== undefined
    readonly property bool showNone: hasCatalog && (picker.query ?? "").trim().length === 0
    readonly property string selectedId: picker.selectedId ?? ""
    // "No <kind>" first (not while searching), then the sections.
    readonly property var groups: !hasCatalog ? []
        : (showNone ? [{ id: "__none_group", none: true, items: [null] }] : []).concat(picker.sections)
    // Every option in reading order (the none option is `__none`).
    readonly property var order: {
        const out = []
        for (const g of groups)
            for (const item of g.items)
                out.push(item === null ? "__none" : item.id)
        return out
    }
    // An Apply from this picker is in flight.
    property bool applying: false
    signal applied()

    function showPicker(kind) {
        applying = false
        Shop.openPicker(kind)
    }
    function closePicker() {
        applying = false
        Shop.openPicker("")
    }
    // Only a draft: nothing is saved until Apply.
    function choose(id) {
        Shop.choosePicker(id)
    }
    function setPickerQuery(text) {
        Shop.setPickerQuery(text)
    }
    function apply() {
        if (!picker.changed || picker.busy)
            return
        applying = true
        if (picker.selectedId === "__none")
            Shop.remove(pickerKind)
        else
            Shop.equip(picker.selectedId)
    }

    onPickerModelChanged: {
        if (applying && pickerModel && !pickerModel.busy) {
            applying = false
            if ((pickerModel.error ?? "").length === 0)
                applied()
        }
    }
}
