//! Which endpoint a saved device choice means. Pure.
//!
//! A choice is matched by its endpoint id (`IMMDevice::GetId`, stable
//! across reboots and hot-plug). Names are only a migration path, for a
//! choice whose id is not listed: an older setting (the outside-call player
//! used to match by name), or one carried over from a client whose ids are
//! not endpoint ids. `waveOut` cut names to 31 characters, so a saved name
//! of that length also matches a listed name that starts with it.

/// An output or input endpoint as the system lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
}

/// `WAVEOUTCAPSW.szPname` holds 31 characters and a terminator.
pub const WAVE_OUT_NAME_CHARS: usize = 31;

fn normal(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The listed endpoint a saved choice means: the one with `id`, else the
/// one named `name` (case-insensitive; a 31-character name also matches a
/// longer listed name it starts with, when exactly one does). `None`: no
/// such endpoint (the caller uses the default).
pub fn match_endpoint<'a>(
    id: Option<&str>,
    name: Option<&str>,
    list: &'a [Endpoint],
) -> Option<&'a Endpoint> {
    if let Some(found) = id
        .filter(|id| !id.is_empty())
        .and_then(|id| list.iter().find(|e| e.id == id))
    {
        return Some(found);
    }
    let wanted = normal(name?);
    if wanted.is_empty() {
        return None;
    }
    if let Some(exact) = list.iter().find(|e| normal(&e.name) == wanted) {
        return Some(exact);
    }
    if wanted.chars().count() < WAVE_OUT_NAME_CHARS {
        return None;
    }
    let mut prefixed = list.iter().filter(|e| normal(&e.name).starts_with(&wanted));
    match (prefixed.next(), prefixed.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Endpoint> {
        [
            ("{0.0.0.00000000}.{518a}", "Speakers (Realtek(R) Audio)"),
            (
                "{0.0.0.00000000}.{e801}",
                "Headphones (Arctis Nova Pro Wireless Game)",
            ),
            (
                "{0.0.0.00000000}.{e802}",
                "Headphones (Arctis Nova Pro Wireless Chat)",
            ),
            ("{0.0.0.00000000}.{faa8}", "Speakers (USB Audio)"),
        ]
        .into_iter()
        .map(|(id, name)| Endpoint {
            id: id.into(),
            name: name.into(),
        })
        .collect()
    }

    fn id_of(found: Option<&Endpoint>) -> Option<&str> {
        found.map(|e| e.id.as_str())
    }

    #[test]
    fn the_id_should_win_over_the_name() {
        let list = list();
        let found = match_endpoint(
            Some("{0.0.0.00000000}.{faa8}"),
            Some("Speakers (Realtek(R) Audio)"),
            &list,
        );
        assert_eq!(id_of(found), Some("{0.0.0.00000000}.{faa8}"));
    }

    #[test]
    fn an_unlisted_id_should_migrate_by_its_exact_name() {
        let list = list();
        let found = match_endpoint(Some("browser-hash-1234"), Some(" speakers (usb audio) "), &list);
        assert_eq!(id_of(found), Some("{0.0.0.00000000}.{faa8}"));
    }

    #[test]
    fn a_truncated_wave_out_name_should_match_the_one_full_name() {
        let list = list();
        let full = "Speakers (Realtek(R) Audio) and more";
        let cut: String = full.chars().take(WAVE_OUT_NAME_CHARS).collect();
        let realtek = Endpoint {
            id: "r".into(),
            name: full.into(),
        };
        let found = match_endpoint(None, Some(&cut), std::slice::from_ref(&realtek));
        assert_eq!(id_of(found), Some("r"));
        // Two headsets share the first 31 characters: no guess.
        let cut: String = "Headphones (Arctis Nova Pro Wireless Game)"
            .chars()
            .take(WAVE_OUT_NAME_CHARS)
            .collect();
        assert_eq!(match_endpoint(None, Some(&cut), &list), None);
    }

    #[test]
    fn a_short_name_should_not_match_by_prefix() {
        let list = list();
        assert_eq!(match_endpoint(None, Some("Speakers"), &list), None);
    }

    #[test]
    fn nothing_saved_or_unknown_should_mean_the_default() {
        let list = list();
        assert_eq!(match_endpoint(None, None, &list), None);
        assert_eq!(match_endpoint(Some(""), Some("  "), &list), None);
        assert_eq!(match_endpoint(Some("gone"), Some("Monitor (HDMI)"), &list), None);
    }
}
