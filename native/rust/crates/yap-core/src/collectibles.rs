//! Collectibles without the store: what a user's `avatar_display` wears
//! (src/lib/avatar-display.ts), the shop's views of the catalog
//! (collectible-catalog-view.ts), the asset URLs a grid or a row should load
//! (collectible-assets.ts), a nameplate's row tint
//! (user-nameplate-presentation.ts) and the display a preview or an equip
//! sends (collectible-selection.ts).

use serde::Serialize;
use serde_json::{json, Map, Value};
use yap_protocol::collectibles::{Catalog, Category, EffectLayer, Frame, Item, Price};

use crate::store::time::parse_rfc3339;

/// The four slots, in the shop's tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Kind {
    #[serde(rename = "avatar_decoration")]
    Decoration,
    #[serde(rename = "profile_effect")]
    Effect,
    #[serde(rename = "nameplate")]
    Nameplate,
    #[serde(rename = "profile_frame")]
    Frame,
}

pub const KINDS: [Kind; 4] = [Kind::Decoration, Kind::Effect, Kind::Nameplate, Kind::Frame];

impl Kind {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "avatar_decoration" => Self::Decoration,
            "profile_effect" => Self::Effect,
            "nameplate" => Self::Nameplate,
            "profile_frame" => Self::Frame,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decoration => "avatar_decoration",
            Self::Effect => "profile_effect",
            Self::Nameplate => "nameplate",
            Self::Frame => "profile_frame",
        }
    }

    /// `KIND_COPY[kind].one`.
    pub fn one(self) -> &'static str {
        match self {
            Self::Decoration => "Avatar decoration",
            Self::Effect => "Profile effect",
            Self::Nameplate => "Nameplate",
            Self::Frame => "Profile frame",
        }
    }

    /// `KIND_COPY[kind].many` (the tab labels).
    pub fn many(self) -> &'static str {
        match self {
            Self::Decoration => "Decorations",
            Self::Effect => "Effects",
            Self::Nameplate => "Nameplates",
            Self::Frame => "Frames",
        }
    }

    /// `KIND_COPY[kind].noneLabel`.
    pub fn none_label(self) -> &'static str {
        match self {
            Self::Decoration => "No decoration",
            Self::Effect => "No effect",
            Self::Nameplate => "No nameplate",
            Self::Frame => "No frame",
        }
    }

    /// The `collectibles` key of an avatar display.
    fn display_key(self) -> &'static str {
        match self {
            Self::Decoration => "avatarDecoration",
            Self::Effect => "profileEffect",
            Self::Nameplate => "nameplate",
            Self::Frame => "profileFrame",
        }
    }
}

const ASSET_HOSTS: [&str; 3] = [
    "cdn.discordapp.com",
    "media.discordapp.net",
    "cdn.yapper.shop",
];

fn host_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    Some(
        host.rsplit('@')
            .next()?
            .split(':')
            .next()
            .unwrap_or_default(),
    )
}

/// `cleanAssetUrl`: https on the collectible CDNs only.
pub fn asset_url(value: Option<&Value>) -> Option<String> {
    let url = value?.as_str()?.trim();
    if url.is_empty() || url.len() > 800 {
        return None;
    }
    let host = host_of(url)?.to_ascii_lowercase();
    ASSET_HOSTS.contains(&host.as_str()).then(|| url.to_owned())
}

/// True for a URL [`asset_url`] keeps: https on a collectible CDN.
pub fn is_asset_url(url: &str) -> bool {
    asset_url(Some(&Value::String(url.to_owned()))).is_some()
}

fn text(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    (!text.is_empty()).then(|| text.chars().take(120).collect())
}

/// The display object, from the column's JSON text or an object.
fn display_object(display: Option<&Value>) -> Option<Map<String, Value>> {
    let value = match display? {
        Value::String(text) => serde_json::from_str(text).ok()?,
        other => other.clone(),
    };
    match value {
        Value::Object(map) if map.get("version").and_then(Value::as_i64) == Some(1) => Some(map),
        _ => None,
    }
}

fn collectibles_of(display: Option<&Value>) -> Map<String, Value> {
    display_object(display)
        .and_then(|mut d| d.remove("collectibles"))
        .and_then(|c| match c {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// The avatar display's crop, `[x, y, width, height]` in percent of the
/// picture, when it is valid (`normalizeAvatarDisplay`: each finite, the
/// size 1-100, inside the picture).
pub fn avatar_crop(display: Option<&Value>) -> Option<[f64; 4]> {
    let crop = display_object(display)?.remove("crop")?;
    let field = |name: &str| {
        crop.get(name)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
    };
    let (x, y, w, h) = (field("x")?, field("y")?, field("width")?, field("height")?);
    let sized = (1.0..=100.0).contains(&w) && (1.0..=100.0).contains(&h);
    let inside = x >= 0.0 && y >= 0.0 && x + w <= 100.0 && y + h <= 100.0;
    (sized && inside).then_some([x, y, w, h])
}

/// A picture URL that carries its avatar crop as `#crop=x,y,w,h` (a
/// fragment never goes over the wire), so every Avatar that draws the URL
/// shows the crop (AvatarImage.tsx `avatarDisplayToImageStyle`).
pub fn with_avatar_crop(url: String, display: Option<&Value>) -> String {
    match avatar_crop(display) {
        Some([x, y, w, h]) if !url.is_empty() && !url.contains('#') => {
            format!("{url}#crop={x},{y},{w},{h}")
        }
        _ => url,
    }
}

/// The decoration an avatar wears: its APNG (`imageUrl`).
pub fn decoration_url(display: Option<&Value>) -> String {
    let collectibles = collectibles_of(display);
    let Some(deco) = collectibles.get("avatarDecoration") else {
        return String::new();
    };
    let complete = text(deco.get("skuId")).is_some()
        && text(deco.get("name")).is_some()
        && text(deco.get("asset")).is_some();
    if !complete {
        return String::new();
    }
    asset_url(deco.get("imageUrl")).unwrap_or_default()
}

/// The sku in one slot of a display (`selectedSkuForKind`).
pub fn selected_sku(display: Option<&Value>, kind: Kind) -> Option<String> {
    let collectibles = collectibles_of(display);
    text(collectibles.get(kind.display_key())?.get("skuId"))
}

/// A nameplate as a row draws it: the still, the motion and the palette.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Plate {
    pub still: String,
    pub motion: String,
    /// The row's edge tint (`#AARRGGBB`).
    pub border: String,
    /// The palette's accent is light, so keys over the plate take dark ink
    /// (`NameplateTheme.isLightAccent`).
    pub light: bool,
    /// A bare `nameplate_url` with no collectible behind it: the account row
    /// fades its left side into the panel so the name reads
    /// (`needsContrastAssist`, the identity mask).
    pub assist: bool,
}

/// A user's nameplate: the collectible's still and webm, or a bare
/// `nameplate_url` (shown as it is).
pub fn plate(
    display: Option<&Value>,
    nameplate_url: Option<&str>,
    content_type: Option<&str>,
    user_id: &str,
) -> Plate {
    let url = nameplate_url.map(str::trim).unwrap_or_default();
    if url.is_empty() {
        return Plate::default();
    }
    let collectibles = collectibles_of(display);
    let selection = collectibles.get("nameplate");
    let still = selection.and_then(|n| asset_url(n.get("staticUrl")));
    let motion = selection.and_then(|n| asset_url(n.get("animatedUrl")));
    let video = content_type.is_some_and(|t| t.starts_with("video/"))
        || url
            .to_ascii_lowercase()
            .split(['?', '#'])
            .next()
            .is_some_and(|p| p.ends_with(".webm") || p.ends_with(".mp4"));
    let (still, motion) = match (still, motion) {
        (Some(still), motion) => (still, motion.unwrap_or_default()),
        (None, _) if video => (String::new(), url.to_owned()),
        (None, _) => (url.to_owned(), String::new()),
    };
    let palette = selection.and_then(|n| text(n.get("palette")));
    let seed = selection
        .and_then(|n| text(n.get("skuId")))
        .unwrap_or_else(|| {
            if url.is_empty() {
                user_id.to_owned()
            } else {
                url.to_owned()
            }
        });
    let (r, g, b) = accent(palette.as_deref(), &seed);
    let light = luminance(r, g, b) > 0.36;
    let alpha: u8 = if light { 46 } else { 87 };
    Plate {
        still,
        motion,
        border: format!("#{alpha:02x}{r:02x}{g:02x}{b:02x}"),
        light,
        assist: selection.is_none(),
    }
}

/// `accentFromPalette`: Discord's own names, CSS names, else a hue from
/// the seed (the web's 32-bit string hash).
fn accent(palette: Option<&str>, seed: &str) -> (u8, u8, u8) {
    let key = palette
        .map(|p| p.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let named = match key.as_str() {
        "lemon" => Some(0xfacc15),
        "arctic" => Some(0xcdddf2),
        "clouds" => Some(0x9ec9ff),
        "amethyst" => Some(0x9251ff),
        "base" => Some(0xf5f5f5),
        "crimson" => Some(0xdc143c),
        "violet" => Some(0xee82ee),
        "black" => Some(0x000000),
        "white" => Some(0xffffff),
        "teal" => Some(0x008080),
        "gold" => Some(0xffd700),
        "blue" => Some(0x0000ff),
        "red" => Some(0xff0000),
        "green" => Some(0x008000),
        "orange" => Some(0xffa500),
        "pink" => Some(0xffc0cb),
        "purple" => Some(0x800080),
        "indigo" => Some(0x4b0082),
        "coral" => Some(0xff7f50),
        "lavender" => Some(0xe6e6fa),
        "silver" => Some(0xc0c0c0),
        "yellow" => Some(0xffff00),
        _ => None,
    };
    if let Some(rgb) = named {
        return (
            ((rgb >> 16) & 0xff) as u8,
            ((rgb >> 8) & 0xff) as u8,
            (rgb & 0xff) as u8,
        );
    }
    let mut hash: i32 = 0;
    for unit in seed.encode_utf16() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(i32::from(unit));
    }
    hsl(f64::from(hash.unsigned_abs() % 360), 0.72, 0.62)
}

fn hsl(hue: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match hue {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (byte(r), byte(g), byte(b))
}

fn luminance(r: u8, g: u8, b: u8) -> f64 {
    let channel = |c: u8| {
        let n = f64::from(c) / 255.0;
        if n <= 0.03928 {
            n / 12.92
        } else {
            ((n + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// A profile effect as the effect layer plays it: layers in stage
/// fractions, and the stills it rests on.
pub fn effect_json(display: Option<&Value>) -> Value {
    let collectibles = collectibles_of(display);
    let Some(effect) = collectibles.get("profileEffect") else {
        return Value::Null;
    };
    let layers: Vec<EffectLayer> = effect
        .get("effects")
        .and_then(|e| serde_json::from_value::<Vec<Value>>(e.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|layer| serde_json::from_value(layer).ok())
        .take(12)
        .collect();
    let still = [
        "staticFrameSrc",
        "reducedMotionSrc",
        "staticUrl",
        "thumbnailPreviewSrc",
        "previewUrl",
    ]
    .iter()
    .find_map(|key| asset_url(effect.get(*key)))
    .unwrap_or_default();
    let animated = asset_url(effect.get("animatedUrl"));
    let layers: Vec<Value> = layers
        .iter()
        .filter_map(|layer| {
            let src = asset_url(Some(&Value::String(layer.src.clone()))).or_else(|| {
                layer
                    .randomized_sources
                    .first()
                    .and_then(|s| asset_url(Some(&Value::String(s.src.clone()))))
            })?;
            Some(layer_json(layer, &src))
        })
        .collect();
    let layers = if layers.is_empty() {
        animated
            .map(|src| vec![json!({"src": src, "loop": false, "start": 0, "duration": 0, "x": 0, "y": 0, "w": 1, "h": 1, "z": 0})])
            .unwrap_or_default()
    } else {
        layers
    };
    if layers.is_empty() && still.is_empty() {
        return Value::Null;
    }
    json!({ "layers": layers, "still": still })
}

fn layer_json(layer: &EffectLayer, src: &str) -> Value {
    const STAGE_W: f64 = 450.0;
    const STAGE_H: f64 = 880.0;
    let pos = layer.position.unwrap_or_default();
    let w = layer
        .width
        .filter(|w| *w > 0.0)
        .map_or(1.0, |w| w / STAGE_W);
    let h = layer
        .height
        .filter(|h| *h > 0.0)
        .map_or(1.0, |h| h / STAGE_H);
    json!({
        "src": src,
        "loop": layer.looping == Some(true) && layer.loop_delay.unwrap_or(0.0) <= 0.0,
        "start": layer.start.unwrap_or(0.0).max(0.0).round(),
        "duration": layer.duration.unwrap_or(0.0).max(0.0).round(),
        "x": pos.x / STAGE_W,
        "y": pos.y / STAGE_H,
        "w": w,
        "h": h,
        "z": layer.z_index.unwrap_or(0.0),
    })
}

/// A profile frame as the card draws it: the footprint around the card
/// (fractions of the frame box width) and its layers.
pub fn frame_json(display: Option<&Value>) -> Value {
    let collectibles = collectibles_of(display);
    let Some(frame) = collectibles.get("profileFrame") else {
        return Value::Null;
    };
    let Ok(frame) = serde_json::from_value::<Frame>(frame.clone()) else {
        return Value::Null;
    };
    frame_value(&frame)
}

fn positive(value: Option<f64>, fallback: f64, minimum: f64) -> f64 {
    value
        .filter(|v| v.is_finite() && *v >= minimum && *v <= 5000.0)
        .map_or(fallback, f64::round)
}

fn frame_value(frame: &Frame) -> Value {
    let layers: Vec<Value> = frame
        .layers
        .iter()
        .filter_map(|layer| {
            let src = asset_url(Some(&Value::String(layer.src.clone())))?;
            Some(json!({
                "src": src,
                "type": if layer.kind.is_empty() { "staple" } else { layer.kind.as_str() },
                "order": if layer.order == "back" { "back" } else { "front" },
                "anchor": if layer.anchor.is_empty() { "top" } else { layer.anchor.as_str() },
            }))
        })
        .take(20)
        .collect();
    if layers.is_empty() {
        return Value::Null;
    }
    let inner = positive(frame.inner_width, 1200.0, 1.0);
    let side = positive(frame.overflow_horizontal, 50.0, 0.0);
    let reaches = |want_bottom: bool| {
        frame.layers.iter().any(|l| {
            l.kind == "rail" || (l.kind != "border" && (l.anchor == "bottom") == want_bottom)
        })
    };
    let top = if reaches(false) {
        positive(frame.overflow_top, 300.0, 0.0)
    } else {
        0.0
    };
    let bottom = if reaches(true) {
        positive(frame.overflow_bottom, 200.0, 0.0)
    } else {
        0.0
    };
    let outer = inner + side * 2.0;
    json!({
        "side": side / outer,
        "top": top / outer,
        "bottom": bottom / outer,
        "layers": layers,
    })
}

// Asset URLs (collectible-assets.ts)

const CDN_HOST: &str = "cdn.discordapp.com";

fn with_params(url: &str, params: &[(&str, String)]) -> String {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (k.to_owned(), v.to_owned())
        })
        .collect();
    for (key, value) in params {
        match pairs.iter_mut().find(|(k, _)| k == key) {
            Some(pair) => pair.1.clone_from(value),
            None => pairs.push(((*key).to_owned(), value.clone())),
        }
    }
    let query: Vec<String> = pairs.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{base}?{}", query.join("&"))
}

/// True for an `https://` URL. Catalog art is only ever https on a CDN;
/// anything else (`file://host/x` opens as a UNC path on Windows) loads
/// nothing.
pub fn is_https(url: &str) -> bool {
    url.get(..8)
        .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
        && url.len() > 8
}

/// `url` when it is https, else empty ([`is_https`]).
pub fn https_only(url: Option<&str>) -> String {
    url.filter(|u| is_https(u))
        .map(str::to_owned)
        .unwrap_or_default()
}

/// A decoration's still first frame, sized for a tile.
pub fn decoration_still_url(item: &Item, size: u32) -> String {
    if let Some(asset) = item.asset.as_deref().filter(|a| !a.is_empty()) {
        return format!("https://{CDN_HOST}/avatar-decoration-presets/{asset}.png?size={size}&passthrough=false");
    }
    item.static_url
        .as_deref()
        .or(item.preview_url.as_deref())
        .filter(|url| is_https(url))
        .map(|url| {
            with_params(
                url,
                &[("passthrough", "false".into()), ("size", size.to_string())],
            )
        })
        .unwrap_or_default()
}

/// A decoration's APNG (only the previewed item loads it).
pub fn decoration_motion_url(item: &Item) -> String {
    if let Some(asset) = item.asset.as_deref().filter(|a| !a.is_empty()) {
        return format!(
            "https://{CDN_HOST}/avatar-decoration-presets/{asset}.png?size=240&passthrough=true"
        );
    }
    https_only(item.animated_url.as_deref().or(item.static_url.as_deref()))
}

/// A shop image resized by Discord's media proxy (a 900 x 1760 PNG of
/// 260 KB becomes 17 KB at 225 x 440).
pub fn resized_shop_image_url(url: &str, width: f64, height: f64) -> String {
    if !is_https(url) {
        return String::new();
    }
    let Some(rest) = url.strip_prefix(&format!("https://{CDN_HOST}")) else {
        return url.to_owned();
    };
    if !rest.starts_with("/media/v1/collectibles-shop/") {
        return url.to_owned();
    }
    with_params(
        &format!("https://media.discordapp.net{rest}"),
        &[
            ("width", (width.round() as i64).to_string()),
            ("height", (height.round() as i64).to_string()),
        ],
    )
}

/// An effect's still for a tile (portrait, 450:880).
pub fn effect_still_url(item: &Item, width: f64) -> String {
    let effect = item.profile_effect.as_ref();
    let source = effect
        .and_then(|e| e.thumbnail_preview_src.clone())
        .or_else(|| effect.and_then(|e| e.reduced_motion_src.clone()))
        .or_else(|| effect.and_then(|e| e.static_frame_src.clone()))
        .or_else(|| item.static_url.clone())
        .unwrap_or_default();
    if source.is_empty() {
        return source;
    }
    resized_shop_image_url(&source, width, (width * 880.0 / 450.0).round())
}

/// A collection's art for its card or hero, with its aspect.
pub fn collection_art(category: &Category, width: f64, hero: bool) -> Option<(String, f64)> {
    const STRIP: f64 = 2120.0 / 360.0;
    const HERO: f64 = 3330.0 / 1050.0;
    let banner = category.banner_url.as_deref().filter(|b| !b.is_empty());
    let hero_url = category
        .hero_banner_url
        .as_deref()
        .filter(|b| !b.is_empty());
    if !hero {
        if let Some(banner) = banner.filter(|b| Some(*b) != hero_url) {
            return Some((resized_shop_image_url(banner, width, width / STRIP), STRIP));
        }
    }
    if let Some(hero_url) = hero_url {
        return Some((resized_shop_image_url(hero_url, width, width / HERO), HERO));
    }
    banner.map(|b| (resized_shop_image_url(b, width, width / STRIP), STRIP))
}

// Catalog views (collectible-catalog-view.ts)

/// One collection's items of the asked kinds, matching the query.
pub struct Section<'a> {
    pub category: &'a Category,
    pub items: Vec<&'a Item>,
}

fn matches(item: &Item, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let haystack = [
        item.name.as_str(),
        item.category_name.as_str(),
        item.label.as_deref().unwrap_or_default(),
        item.variant_label.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_lowercase();
    haystack.contains(query)
}

/// `buildCollectionSections`: collections in source order, each item in
/// its first collection only; items no collection lists go to "More".
pub fn sections<'a>(catalog: &'a Catalog, kinds: Option<&[Kind]>, query: &str) -> Vec<Section<'a>> {
    use std::collections::{HashMap, HashSet};
    let query = query.trim().to_lowercase();
    let by_id: HashMap<&str, &Item> = catalog.items.iter().map(|i| (i.id.as_str(), i)).collect();
    let wanted = |item: &Item| {
        kinds.is_none_or(|kinds| Kind::parse(&item.kind).is_some_and(|k| kinds.contains(&k)))
    };
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for category in &catalog.categories {
        let mut items = Vec::new();
        for id in &category.item_ids {
            if seen.contains(id.as_str()) {
                continue;
            }
            let Some(item) = by_id.get(id.as_str()) else {
                continue;
            };
            if !wanted(item) {
                continue;
            }
            seen.insert(id.as_str());
            if matches(item, &query) {
                items.push(*item);
            }
        }
        if !items.is_empty() {
            out.push(Section { category, items });
        }
    }
    let orphans: Vec<&Item> = catalog
        .items
        .iter()
        .filter(|i| !seen.contains(i.id.as_str()) && wanted(i) && matches(i, &query))
        .collect();
    if !orphans.is_empty() {
        out.push(Section {
            category: more_category(),
            items: orphans,
        });
    }
    out
}

fn more_category() -> &'static Category {
    use std::sync::OnceLock;
    static MORE: OnceLock<Category> = OnceLock::new();
    MORE.get_or_init(|| Category {
        id: "__more".into(),
        name: "More".into(),
        ..Category::default()
    })
}

/// Colour variants of one design, the base first.
pub struct Family<'a> {
    pub key: String,
    pub base: &'a Item,
    pub variants: Vec<&'a Item>,
}

/// `groupVariantFamilies`.
pub fn families<'a>(items: &[&'a Item]) -> Vec<Family<'a>> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<&'a Item>> = Default::default();
    for item in items {
        let key = format!(
            "{}:{}",
            item.kind,
            item.variant_of.as_deref().unwrap_or(&item.sku_id)
        );
        groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        if let Some(group) = groups.get_mut(&key) {
            group.push(item);
        }
    }
    order
        .into_iter()
        .filter_map(|key| {
            let members = groups.remove(&key)?;
            let base = members
                .iter()
                .find(|m| m.variant_of.as_deref() == Some(m.sku_id.as_str()))
                .copied()
                .or_else(|| members.first().copied())?;
            let mut variants = vec![base];
            variants.extend(members.iter().filter(|m| !std::ptr::eq(**m, base)).copied());
            Some(Family {
                key,
                base,
                variants,
            })
        })
        .collect()
}

/// "Leaves in N days" within two weeks of the pull, "Retired" after it.
pub fn collection_status(category: &Category, now_ms: i64) -> Option<(&'static str, String)> {
    let at = parse_rfc3339(category.unpublished_at.as_deref()?)?;
    if at <= now_ms {
        return Some(("retired", "Retired".into()));
    }
    const DAY: i64 = 86_400_000;
    let days = (at - now_ms + DAY - 1) / DAY;
    if days > 14 {
        return None;
    }
    let label = if days <= 1 {
        "Leaves today".to_owned()
    } else {
        format!("Leaves in {days} days")
    };
    Some(("leaving", label))
}

fn group_thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// `formatSourcePrice`: the source shop's price (struck through: here
/// everything is free).
pub fn format_price(item: &Item) -> Option<String> {
    let price: &Price = item.price.as_ref().or(item.nitro_price.as_ref())?;
    if price.currency == "discord_orb" {
        return Some(format!("{} Orbs", group_thousands(price.amount)));
    }
    let symbol = match price.currency.to_ascii_lowercase().as_str() {
        "usd" => "$",
        "eur" => "€",
        "gbp" => "£",
        "" => return None,
        _ => "",
    };
    let scale = 10_i64.checked_pow(price.exponent)?;
    let whole = price.amount / scale;
    let cents = (price.amount % scale).unsigned_abs();
    let decimals = usize::try_from(price.exponent).ok()?;
    let body = if decimals == 0 {
        group_thousands(whole)
    } else {
        format!("{}.{cents:0decimals$}", group_thousands(whole))
    };
    Some(if symbol.is_empty() {
        format!("{} {body}", price.currency.to_uppercase())
    } else {
        format!("{symbol}{body}")
    })
}

/// The name without a trailing "(Variant)".
pub fn base_name(item: &Item) -> String {
    let name = item.name.trim_end();
    let base = match (name.ends_with(')'), name.rfind(" (")) {
        (true, Some(at)) => name[..at].trim(),
        _ => name,
    };
    if base.is_empty() {
        item.name.clone()
    } else {
        base.to_owned()
    }
}

pub fn variant_name(item: &Item) -> Option<String> {
    if let Some(label) = item.variant_label.as_deref().filter(|l| !l.is_empty()) {
        return Some(label.to_owned());
    }
    let name = item.name.trim_end();
    let inner = name.strip_suffix(')')?;
    let at = inner.rfind('(')?;
    Some(inner[at + 1..].to_owned()).filter(|v| !v.is_empty())
}

/// A CSS `linear-gradient(<angle>deg, <colour> <pos>%, ...)` as a QML
/// gradient: vertical unless the angle is (roughly) sideways.
pub fn gradient(css: Option<&str>) -> Value {
    let Some(css) = css.map(str::trim) else {
        return Value::Null;
    };
    let Some(inner) = css
        .strip_prefix("linear-gradient(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        if css.starts_with('#') {
            return json!({ "vertical": true, "stops": [[0.0, css], [1.0, css]] });
        }
        return Value::Null;
    };
    let mut parts = inner.split(',').map(str::trim);
    let mut angle = 180.0;
    let mut stops = Vec::new();
    if let Some(first) = parts.next() {
        if let Some(deg) = first
            .strip_suffix("deg")
            .and_then(|d| d.trim().parse::<f64>().ok())
        {
            angle = deg;
        } else {
            parts = inner.split(',').map(str::trim);
        }
    }
    let colours: Vec<&str> = parts.collect();
    let count = colours.len();
    for (i, stop) in colours.iter().enumerate() {
        let mut bits = stop.split_whitespace();
        let Some(colour) = bits.next().filter(|c| c.starts_with('#')) else {
            continue;
        };
        let at = bits
            .next()
            .and_then(|p| p.strip_suffix('%'))
            .and_then(|p| p.parse::<f64>().ok())
            .map_or_else(
                || {
                    if count > 1 {
                        i as f64 / (count - 1) as f64
                    } else {
                        0.0
                    }
                },
                |p| p / 100.0,
            );
        stops.push(json!([at, colour]));
    }
    if stops.is_empty() {
        return Value::Null;
    }
    let turned = (angle.rem_euclid(360.0) - 90.0).abs() < 45.0
        || (angle.rem_euclid(360.0) - 270.0).abs() < 45.0;
    let reversed = angle.rem_euclid(360.0) >= 315.0
        || angle.rem_euclid(360.0) < 45.0
        || (angle.rem_euclid(360.0) - 270.0).abs() < 45.0;
    if reversed {
        stops = stops
            .into_iter()
            .rev()
            .map(|s| json!([1.0 - s[0].as_f64().unwrap_or(0.0), s[1]]))
            .collect();
    }
    json!({ "vertical": !turned, "stops": stops })
}

// Selection (collectible-selection.ts)

/// What one item puts in its slot of a display.
pub fn selection_value(item: &Item) -> Option<Value> {
    let kind = Kind::parse(&item.kind)?;
    Some(match kind {
        Kind::Decoration => {
            let asset = item.asset.clone().filter(|a| !a.is_empty())?;
            json!({
                "skuId": item.sku_id,
                "name": item.name,
                "asset": asset,
                "imageUrl": item.static_url.clone().or_else(|| item.preview_url.clone()).or_else(|| item.animated_url.clone()).unwrap_or_default(),
            })
        }
        Kind::Effect => {
            let effect = item.profile_effect.clone().unwrap_or_default();
            let urls: Vec<String> = if effect.effects.is_empty() {
                item.animated_url.iter().cloned().collect()
            } else {
                effect.effects.iter().map(|e| e.src.clone()).collect()
            };
            json!({
                "skuId": item.sku_id,
                "name": item.name,
                "animationType": effect.animation_type,
                "previewUrl": item.preview_url,
                "thumbnailPreviewSrc": effect.thumbnail_preview_src,
                "reducedMotionSrc": effect.reduced_motion_src,
                "staticFrameSrc": effect.static_frame_src,
                "staticUrl": item.static_url,
                "animatedUrl": item.animated_url,
                "effectUrls": urls,
                "effects": effect.effects,
            })
        }
        Kind::Nameplate => json!({
            "skuId": item.sku_id,
            "name": item.name,
            "staticUrl": item.static_url.clone().or_else(|| item.preview_url.clone()).unwrap_or_default(),
            "animatedUrl": item.animated_url,
            "palette": item.palette,
        }),
        Kind::Frame => {
            let frame = item.frame.clone()?;
            json!({
                "skuId": item.sku_id,
                "name": item.name,
                "innerWidth": frame.inner_width,
                "overflowTop": frame.overflow_top,
                "overflowBottom": frame.overflow_bottom,
                "overflowHorizontal": frame.overflow_horizontal,
                "layers": frame.layers,
            })
        }
    })
}

/// `displayWithCollectible`: `display` with one slot swapped for `item`
/// (or emptied), crop and the other slots kept.
pub fn display_with(display: Option<&Value>, kind: Kind, item: Option<&Item>) -> Value {
    let mut base = display_object(display).unwrap_or_default();
    base.insert("version".into(), json!(1));
    let mut collectibles = match base.remove("collectibles") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    collectibles.remove(kind.display_key());
    if let Some(value) = item
        .filter(|i| i.kind == kind.as_str())
        .and_then(selection_value)
    {
        collectibles.insert(kind.display_key().into(), value);
    }
    if !collectibles.is_empty() {
        base.insert("collectibles".into(), Value::Object(collectibles));
    }
    Value::Object(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, sku: &str, name: &str) -> Item {
        Item {
            id: format!("{kind}:{sku}"),
            sku_id: sku.into(),
            name: name.into(),
            kind: kind.into(),
            ..Item::default()
        }
    }

    #[test]
    fn a_decoration_should_come_from_its_image_url() {
        let display = json!({"version": 1, "collectibles": {"avatarDecoration": {"skuId": "1", "name": "Duck", "asset": "a_1", "imageUrl": "https://cdn.discordapp.com/d.png"}}});
        assert_eq!(
            decoration_url(Some(&display)),
            "https://cdn.discordapp.com/d.png"
        );
    }

    #[test]
    fn an_avatar_crop_should_ride_on_the_picture_url() {
        let display = json!({"version": 1, "crop": {"x": 10, "y": 5.5, "width": 50, "height": 50}});
        assert_eq!(
            with_avatar_crop("https://x.invalid/a.png".into(), Some(&display)),
            "https://x.invalid/a.png#crop=10,5.5,50,50"
        );
        // Outside the picture, or no picture: nothing is added.
        let bad = json!({"version": 1, "crop": {"x": 60, "y": 0, "width": 50, "height": 50}});
        assert_eq!(
            with_avatar_crop("https://x.invalid/a.png".into(), Some(&bad)),
            "https://x.invalid/a.png"
        );
        assert_eq!(with_avatar_crop(String::new(), Some(&display)), "");
    }

    #[test]
    fn a_display_as_json_text_should_parse() {
        let display = Value::String(r#"{"version":1,"collectibles":{"avatarDecoration":{"skuId":"1","name":"D","asset":"a","imageUrl":"https://cdn.discordapp.com/x.png"}}}"#.into());
        assert!(!decoration_url(Some(&display)).is_empty());
    }

    #[test]
    fn decorations_off_the_cdn_should_be_dropped() {
        let display = json!({"version": 1, "collectibles": {"avatarDecoration": {"skuId": "1", "name": "D", "asset": "a", "imageUrl": "https://evil.example/d.png"}}});
        assert_eq!(decoration_url(Some(&display)), "");
    }

    #[test]
    fn a_collectible_nameplate_should_rest_on_its_still() {
        let display = json!({"version": 1, "collectibles": {"nameplate": {"skuId": "9", "name": "N", "staticUrl": "https://cdn.discordapp.com/s.png", "animatedUrl": "https://cdn.discordapp.com/a.webm", "palette": "crimson"}}});
        let plate = plate(
            Some(&display),
            Some("https://cdn.discordapp.com/a.webm"),
            Some("video/webm"),
            "u",
        );
        assert_eq!(
            (plate.still.as_str(), plate.motion.as_str()),
            (
                "https://cdn.discordapp.com/s.png",
                "https://cdn.discordapp.com/a.webm"
            )
        );
    }

    #[test]
    fn a_collectible_plate_should_not_need_the_identity_fade() {
        let display = json!({"version": 1, "collectibles": {"nameplate": {"skuId": "9", "name": "N", "staticUrl": "https://cdn.discordapp.com/s.png", "palette": "gold"}}});
        let own = plate(
            Some(&display),
            Some("https://cdn.discordapp.com/s.png"),
            Some("image/png"),
            "u",
        );
        let bare = plate(None, Some("https://cdn.discordapp.com/b.png"), None, "u");
        // Gold is a light accent: dark ink over it.
        assert_eq!((own.assist, own.light, bare.assist), (false, true, true));
    }

    #[test]
    fn a_crimson_plate_should_tint_the_row_crimson() {
        let display = json!({"version": 1, "collectibles": {"nameplate": {"skuId": "9", "name": "N", "staticUrl": "https://cdn.discordapp.com/s.png", "palette": "crimson"}}});
        assert_eq!(
            plate(
                Some(&display),
                Some("https://cdn.discordapp.com/s.png"),
                Some("image/png"),
                "u"
            )
            .border,
            "#57dc143c"
        );
    }

    #[test]
    fn no_nameplate_url_should_mean_no_plate() {
        assert_eq!(plate(None, None, None, "u"), Plate::default());
    }

    #[test]
    fn unknown_palettes_should_hash_the_seed_like_the_web() {
        // JS: hash("abc") = 96354 -> hue 234.
        assert_eq!(accent(Some("cobalt"), "abc"), hsl(234.0, 0.72, 0.62));
    }

    #[test]
    fn prices_should_format_as_dollars() {
        let mut i = item("nameplate", "1", "N");
        i.price = Some(Price {
            amount: 599,
            exponent: 2,
            currency: "usd".into(),
        });
        assert_eq!(format_price(&i).as_deref(), Some("$5.99"));
    }

    #[test]
    fn orb_prices_should_group_thousands() {
        let mut i = item("nameplate", "1", "N");
        i.price = Some(Price {
            amount: 12000,
            exponent: 0,
            currency: "discord_orb".into(),
        });
        assert_eq!(format_price(&i).as_deref(), Some("12,000 Orbs"));
    }

    #[test]
    fn variants_should_group_under_their_base() {
        let mut a = item("nameplate", "1", "Ripples");
        a.variant_of = Some("1".into());
        let mut b = item("nameplate", "2", "Ripples (Golden)");
        b.variant_of = Some("1".into());
        let c = item("nameplate", "3", "Other");
        let fams = families(&[&b, &a, &c]);
        assert_eq!(
            (
                fams.len(),
                fams[0].base.sku_id.as_str(),
                fams[0].variants.len()
            ),
            (2, "1", 2)
        );
    }

    #[test]
    fn base_names_should_drop_the_variant() {
        assert_eq!(
            base_name(&item("nameplate", "1", "Ripples (Golden)")),
            "Ripples"
        );
    }

    #[test]
    fn variant_names_should_come_from_the_suffix() {
        assert_eq!(
            variant_name(&item("nameplate", "1", "Ripples (Golden)")).as_deref(),
            Some("Golden")
        );
    }

    #[test]
    fn a_collection_leaving_soon_should_say_so() {
        let category = Category {
            id: "c".into(),
            unpublished_at: Some("2026-11-08T00:00:00Z".into()),
            ..Category::default()
        };
        let now = parse_rfc3339("2026-11-05T00:00:00Z").unwrap_or_default();
        assert_eq!(
            collection_status(&category, now).map(|s| s.1),
            Some("Leaves in 3 days".into())
        );
    }

    #[test]
    fn a_pulled_collection_should_be_retired() {
        let category = Category {
            id: "c".into(),
            unpublished_at: Some("2026-11-01T00:00:00Z".into()),
            ..Category::default()
        };
        let now = parse_rfc3339("2026-11-05T00:00:00Z").unwrap_or_default();
        assert_eq!(
            collection_status(&category, now).map(|s| s.0),
            Some("retired")
        );
    }

    #[test]
    fn sections_should_list_an_item_once() {
        let catalog = Catalog {
            categories: vec![
                Category {
                    id: "a".into(),
                    item_ids: vec!["nameplate:1".into()],
                    ..Category::default()
                },
                Category {
                    id: "b".into(),
                    item_ids: vec!["nameplate:1".into()],
                    ..Category::default()
                },
            ],
            items: vec![item("nameplate", "1", "N")],
            ..Catalog::default()
        };
        assert_eq!(sections(&catalog, None, "").len(), 1);
    }

    #[test]
    fn search_should_match_the_collection_name() {
        let mut i = item("nameplate", "1", "N");
        i.category_name = "Summer Bliss".into();
        let catalog = Catalog {
            items: vec![i],
            ..Catalog::default()
        };
        assert_eq!(sections(&catalog, None, "bliss").len(), 1);
    }

    #[test]
    fn shop_images_should_resize_through_the_media_proxy() {
        assert_eq!(
            resized_shop_image_url("https://cdn.discordapp.com/media/v1/collectibles-shop/1/2/static", 225.0, 440.0),
            "https://media.discordapp.net/media/v1/collectibles-shop/1/2/static?width=225&height=440"
        );
    }

    #[test]
    fn decoration_stills_should_ask_for_a_still() {
        let mut i = item("avatar_decoration", "1", "D");
        i.asset = Some("a_1".into());
        assert_eq!(decoration_still_url(&i, 160), "https://cdn.discordapp.com/avatar-decoration-presets/a_1.png?size=160&passthrough=false");
    }

    #[test]
    fn equipping_should_keep_the_other_slots() {
        let display = json!({"version": 1, "crop": {"x": 0, "y": 0, "width": 50, "height": 50}, "collectibles": {"nameplate": {"skuId": "9"}}});
        let mut deco = item("avatar_decoration", "1", "D");
        deco.asset = Some("a".into());
        let next = display_with(Some(&display), Kind::Decoration, Some(&deco));
        assert_eq!(
            (
                next["collectibles"]["nameplate"]["skuId"].as_str(),
                next["crop"]["width"].as_i64()
            ),
            (Some("9"), Some(50))
        );
    }

    #[test]
    fn removing_should_empty_the_slot() {
        let display = json!({"version": 1, "collectibles": {"nameplate": {"skuId": "9"}}});
        assert!(display_with(Some(&display), Kind::Nameplate, None)
            .get("collectibles")
            .is_none());
    }

    #[test]
    fn a_vertical_gradient_should_keep_its_stops() {
        let g = gradient(Some("linear-gradient(180deg, #2B2957 0%, #6C6773 100%)"));
        assert_eq!(
            (g["vertical"].as_bool(), g["stops"][1][1].as_str()),
            (Some(true), Some("#6C6773"))
        );
    }

    #[test]
    fn effects_should_place_layers_in_stage_fractions() {
        let display = json!({"version": 1, "collectibles": {"profileEffect": {"skuId": "1", "name": "E", "effects": [{"src": "https://cdn.discordapp.com/e", "start": 3000, "duration": 3000, "loop": true, "width": 225, "height": 880, "position": {"x": 225, "y": 0}}]}}});
        let e = effect_json(Some(&display));
        assert_eq!(
            (
                e["layers"][0]["x"].as_f64(),
                e["layers"][0]["w"].as_f64(),
                e["layers"][0]["loop"].as_bool()
            ),
            (Some(0.5), Some(0.5), Some(true))
        );
    }

    #[test]
    fn frames_should_reserve_their_footprint() {
        let display = json!({"version": 1, "collectibles": {"profileFrame": {"skuId": "1", "name": "F", "innerWidth": 1200, "overflowTop": 300, "overflowBottom": 200, "overflowHorizontal": 50, "layers": [{"id": "a", "src": "https://cdn.discordapp.com/f", "type": "staple", "order": "front", "anchor": "top"}]}}});
        let f = frame_json(Some(&display));
        assert_eq!(
            (f["top"].as_f64(), f["bottom"].as_f64()),
            (Some(300.0 / 1300.0), Some(0.0))
        );
    }
}
