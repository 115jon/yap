//! `localstorage-fixture <out dir> <origin> <items.json>`: a Chromium
//! localStorage database in `<out dir>` (a `Local Storage\leveldb` folder)
//! holding the string items of the JSON object `<items.json>` for
//! `<origin>`. The encoding mirrors desktop-native
//! `yap_core::tauri::write_local_storage_fixture`: keys `_<origin>\0<key>`,
//! strings as a format byte then Latin-1 (1) or UTF-16LE (0), and a
//! `VERSION` entry, flushed to a table file as Chromium's compacted
//! databases have. `localstorage-fixture --read <dir>` only opens and
//! lists an existing one.

use std::io::Read;
use std::path::Path;
use std::rc::Rc;

use rusty_leveldb::env::Env;
use rusty_leveldb::{MemEnv, Options, DB};

fn encode(text: &str) -> Vec<u8> {
    if text.chars().all(|c| (c as u32) < 256) {
        std::iter::once(1u8)
            .chain(text.chars().map(|c| c as u8))
            .collect()
    } else {
        std::iter::once(0u8)
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect()
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--read <dir>`: open an existing database (one Chromium wrote) the way
    // the import does and list its origins and keys; exit 1 if it cannot.
    if let [flag, dir] = args.as_slice() {
        if flag == "--read" {
            let read = read_back(Path::new(dir))?;
            let mut origins = std::collections::BTreeMap::<&str, Vec<&str>>::new();
            for (origin, key) in read.keys() {
                origins.entry(origin).or_default().push(key);
            }
            println!("opened {dir}: {} items in {} origins", read.len(), origins.len());
            for (origin, keys) in origins {
                println!("  {origin}: {}", keys.join(", "));
            }
            return Ok(());
        }
    }
    let [out, origin, items] = args.as_slice() else {
        return Err("usage: localstorage-fixture <out dir> <origin> <items.json>".into());
    };
    let items: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&std::fs::read(items).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;

    const NAME: &str = "fixture";
    let env = Rc::new(Box::new(MemEnv::new()) as Box<dyn Env>);
    let options = Options {
        env: env.clone(),
        ..Options::default()
    };
    let mut db = DB::open(NAME, options).map_err(|e| e.to_string())?;
    db.put(b"VERSION", b"1").map_err(|e| e.to_string())?;
    for (key, value) in &items {
        let value = value
            .as_str()
            .ok_or_else(|| format!("{key}: localStorage values are strings"))?;
        let mut k = format!("_{origin}").into_bytes();
        k.push(0);
        k.extend(encode(key));
        db.put(&k, &encode(value)).map_err(|e| e.to_string())?;
    }
    db.flush().map_err(|e| e.to_string())?;
    db.close().map_err(|e| e.to_string())?;
    drop(db);

    let out = Path::new(out);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    for path in env.children(Path::new(NAME)).map_err(|e| e.to_string())? {
        let Some(name) = path.file_name() else {
            continue;
        };
        let full = Path::new(NAME).join(name);
        let mut bytes = Vec::new();
        env.open_sequential_file(&full)
            .and_then(|mut f| f.read_to_end(&mut bytes).map_err(Into::into))
            .map_err(|e| e.to_string())?;
        std::fs::write(out.join(name), bytes).map_err(|e| e.to_string())?;
        println!("wrote {}", out.join(name).display());
    }

    // Read back the way the import does (yap_core::tauri::read_local_storage):
    // the files into a fresh MemEnv, LOCK skipped, nothing created.
    let read = read_back(out)?;
    for (key, value) in &items {
        if read.get(&(origin.clone(), key.clone())).map(String::as_str) != value.as_str() {
            return Err(format!("{key} did not read back"));
        }
    }
    println!("read back {} items for {origin}", items.len());
    Ok(())
}

fn decode(bytes: &[u8]) -> Option<String> {
    let (format, rest) = bytes.split_first()?;
    match format {
        0 if rest.len() % 2 == 0 => String::from_utf16(
            &rest
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
        .ok(),
        1 => Some(rest.iter().map(|&b| char::from(b)).collect()),
        _ => None,
    }
}

fn read_back(dir: &Path) -> Result<std::collections::BTreeMap<(String, String), String>, String> {
    use rusty_leveldb::LdbIterator;
    use std::io::Write;
    const NAME: &str = "localstorage";
    let env = MemEnv::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name();
        if name == "LOCK" || !entry.path().is_file() {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|e| e.to_string())?;
        let mut file = env
            .open_writable_file(&Path::new(NAME).join(&name))
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    let options = Options {
        env: Rc::new(Box::new(env)),
        create_if_missing: false,
        ..Options::default()
    };
    let mut db = DB::open(NAME, options).map_err(|e| e.to_string())?;
    let mut iter = db.new_iter().map_err(|e| e.to_string())?;
    let mut out = std::collections::BTreeMap::new();
    while let Some((key, value)) = iter.next() {
        let Some(rest) = key.strip_prefix(b"_") else {
            continue;
        };
        let Some(split) = rest.iter().position(|&b| b == 0) else {
            continue;
        };
        let origin = String::from_utf8_lossy(&rest[..split]).into_owned();
        if let (Some(k), Some(v)) = (decode(&rest[split + 1..]), decode(&value)) {
            out.insert((origin, k), v);
        }
    }
    Ok(out)
}
