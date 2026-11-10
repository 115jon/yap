//! Valve KeyValues, as far as Yap reads them: `"key"  "value"` lines in
//! `libraryfolders.vdf` and `appmanifest_<id>.acf`.

use std::path::{Path, PathBuf};

/// The values of `"key"  "value"` lines in a Valve KeyValues file, with
/// `\\` unescaped.
pub(crate) fn vdf_values<'a>(text: &'a str, key: &'a str) -> impl Iterator<Item = String> + 'a {
    let quoted = format!("\"{key}\"");
    text.lines().filter_map(move |line| {
        let rest = line.trim().strip_prefix(quoted.as_str())?;
        let value = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
        Some(value.replace("\\\\", "\\"))
    })
}

/// Every Steam library's `steamapps` folder: Steam's own first, then the
/// `path`s of `libraryfolders.vdf` (Steam's own is usually listed too; it
/// is not repeated).
pub(crate) fn steam_libraries(steam: &Path) -> Vec<PathBuf> {
    let own = steam.join("steamapps");
    let mut libraries = vec![own.clone()];
    if let Ok(vdf) = std::fs::read_to_string(own.join("libraryfolders.vdf")) {
        for path in vdf_values(&vdf, "path") {
            let library = Path::new(&path).join("steamapps");
            let same = |a: &Path, b: &Path| {
                a.to_string_lossy().replace('\\', "/").to_lowercase()
                    == b.to_string_lossy().replace('\\', "/").to_lowercase()
            };
            if !libraries.iter().any(|known| same(known, &library)) {
                libraries.push(library);
            }
        }
    }
    libraries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_names_and_library_paths_are_read() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"570\"\n\t\"name\"\t\t\"Dota 2\"\n}\n";
        assert_eq!(vdf_values(acf, "name").collect::<Vec<_>>(), ["Dota 2"]);
        let vdf =
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}";
        assert_eq!(
            vdf_values(vdf, "path").collect::<Vec<_>>(),
            [r"D:\SteamLibrary"]
        );
    }
}
