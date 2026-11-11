//! The Qt app's session token on disk, sealed by a key the platform keeps
//! out of the app's files (the Android Keystore: client.rs's
//! `KeystoreSealer` and android/src/dev/yap/qtproto/TokenVault.java).
//!
//! The file holds only what [`Sealer::seal`] returned. A copy of the app's
//! data folder without the device's key (a backup tool, a file copied off a
//! rooted phone, a forensic image) does not give the token away. Code
//! running as the app on the device can still unseal it: the key needs no
//! user authentication, so a notification's Reply works with the app killed
//! and the phone locked.
//!
//! Older builds kept the token in plain text ([`LEGACY_FILE`]): the first
//! [`Vault::load`] seals it and deletes the plain file. A sealed file that
//! can never open (the key gone or invalidated, the file damaged) signs out:
//! the file and the key are dropped, and the next sign-in makes new ones. A
//! key store that cannot be asked signs out for that run only and keeps the
//! file.
//!
//! The Java side (push/PushStore.java) reads the same files with the app
//! killed. It never writes them: the core owns them, and each write is a
//! rename of a complete file, so a reader sees the old file or the new one.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The sealed token's file name in the app's data folder.
pub const SEALED_FILE: &str = "session.sealed";
/// The plain-text token of older builds, migrated and deleted.
pub const LEGACY_FILE: &str = "session.token";

/// Why the platform could not seal or unseal (no secret in the text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    /// The bytes can never open: no key, another key, a key the platform
    /// invalidated, or damaged bytes.
    Invalid(String),
    /// The key store could not be asked (it may answer next time).
    Unavailable(String),
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(why) => write!(f, "invalid: {why}"),
            Self::Unavailable(why) => write!(f, "unavailable: {why}"),
        }
    }
}

/// Authenticated encryption with a key the platform holds.
pub trait Sealer: Send + Sync + 'static {
    /// `plain` sealed, with the key made first if there is none.
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, SealError>;
    /// What [`seal`](Self::seal) sealed; [`SealError::Invalid`] for a
    /// missing or changed key and for damaged or foreign bytes.
    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, SealError>;
    /// Drops the key: the next seal makes a new one.
    fn reset(&self);
}

/// The session token's files: [`SEALED_FILE`] and, until migrated,
/// [`LEGACY_FILE`]. Calls block (file I/O and the platform's key store);
/// the core makes them on a blocking thread, one at a time.
pub struct Vault<S> {
    sealer: S,
    sealed: PathBuf,
    legacy: PathBuf,
}

impl<S: Sealer> Vault<S> {
    /// The vault in the app's data folder.
    pub fn in_dir(sealer: S, dir: &Path) -> Self {
        Self {
            sealer,
            sealed: dir.join(SEALED_FILE),
            legacy: dir.join(LEGACY_FILE),
        }
    }

    /// The stored token, or `None` (signed out). Migrates a plain-text
    /// token; drops a sealed one that no longer opens.
    pub fn load(&self) -> Option<String> {
        let bytes = match fs::read(&self.sealed) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return self.migrate(),
            Err(e) => {
                // Not a key problem: leave the file for the next start.
                log(&format!("could not read the sealed session: {e}"));
                return None;
            }
        };
        // A plain copy never outlives the sealed one (a migration that
        // stopped between the two steps).
        remove(&self.legacy);
        // A key store that could not be asked gets one more try.
        let opened = match self.sealer.open(&bytes) {
            Err(SealError::Unavailable(_)) => self.sealer.open(&bytes),
            other => other,
        };
        match opened.map(String::from_utf8) {
            Ok(Ok(text)) if !text.trim().is_empty() => Some(text.trim().to_owned()),
            Ok(_) => {
                log("the sealed session is not a token: signed out");
                self.discard();
                None
            }
            Err(e @ SealError::Invalid(_)) => {
                log(&format!(
                    "the sealed session does not open ({e}): signed out"
                ));
                self.discard();
                None
            }
            Err(e @ SealError::Unavailable(_)) => {
                // Signed out for this run only: the file stays for the next
                // start (a sign-in meanwhile replaces it).
                log(&format!("the key store is unavailable ({e}): signed out"));
                None
            }
        }
    }

    /// Keeps `token` sealed (and no plain copy).
    pub fn store(&self, token: &str) {
        if !self.write(token) {
            log("could not seal the session: signed out at the next start");
        }
        remove(&self.legacy);
    }

    /// Signed out: no token on disk. The key stays for the next sign-in.
    pub fn clear(&self) {
        remove(&self.sealed);
        remove(&self.legacy);
        remove(&tmp_path(&self.sealed));
    }

    /// The plain-text token of an older build, sealed; the plain file goes
    /// whatever happens. A token that cannot be sealed still serves this run.
    fn migrate(&self) -> Option<String> {
        let text = match fs::read_to_string(&self.legacy) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => {
                log(&format!("could not read the old session file: {e}"));
                remove(&self.legacy);
                return None;
            }
        };
        let token = text.trim().to_owned();
        if token.is_empty() {
            remove(&self.legacy);
            return None;
        }
        if self.write(&token) {
            log("moved the session into the sealed file");
        } else {
            log("could not seal the old session: signed out at the next start");
        }
        // Only after the sealed file is complete (write renames it in).
        remove(&self.legacy);
        Some(token)
    }

    /// Seals `token` into the sealed file. On a failure the key is made
    /// anew and tried once more; if that fails too, an older sealed token
    /// (maybe another account's) is dropped rather than left to come back.
    fn write(&self, token: &str) -> bool {
        let sealed = match self.sealer.seal(token.as_bytes()) {
            Ok(sealed) => sealed,
            Err(first) => {
                log(&format!("seal failed ({first}): a new key"));
                self.sealer.reset();
                match self.sealer.seal(token.as_bytes()) {
                    Ok(sealed) => sealed,
                    Err(e) => {
                        log(&format!("seal failed again ({e})"));
                        remove(&self.sealed);
                        return false;
                    }
                }
            }
        };
        match write_atomic(&self.sealed, &sealed) {
            Ok(()) => true,
            Err(e) => {
                log(&format!("could not write the sealed session: {e}"));
                remove(&self.sealed);
                false
            }
        }
    }

    /// A sealed file that does not open: gone, with its key.
    fn discard(&self) {
        remove(&self.sealed);
        self.sealer.reset();
    }
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// The whole file or the old one: written beside it, synced, renamed over.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_path(path);
    let written = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if written.is_err() {
        remove(&tmp);
    }
    written
}

fn remove(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => log(&format!("could not delete {}: {e}", file_name(path))),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// To stderr, which the app sends to logcat. Never the token.
fn log(message: &str) {
    eprintln!("[session] {message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A stand-in key store: XOR with a key byte that `reset` changes,
    /// behind a tag, so a reset key fails to open the old bytes as the
    /// Keystore's AES-GCM does. Failures are counted down per call.
    #[derive(Clone, Default)]
    struct Fake(Arc<FakeState>);

    #[derive(Default)]
    struct FakeState {
        key: AtomicU32,
        resets: AtomicUsize,
        seal_failures: AtomicUsize,
        open_failures: AtomicUsize,
    }

    const TAG: &[u8] = b"fake1";

    fn take(counter: &AtomicUsize) -> bool {
        counter
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
    }

    impl Fake {
        fn byte(&self) -> u8 {
            0x5a ^ self.0.key.load(Ordering::SeqCst) as u8
        }
        fn fail_seals(&self, n: usize) {
            self.0.seal_failures.store(n, Ordering::SeqCst);
        }
        fn fail_opens(&self, n: usize) {
            self.0.open_failures.store(n, Ordering::SeqCst);
        }
        fn resets(&self) -> usize {
            self.0.resets.load(Ordering::SeqCst)
        }
    }

    impl Sealer for Fake {
        fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, SealError> {
            if take(&self.0.seal_failures) {
                return Err(SealError::Unavailable("seal".into()));
            }
            let mut out = TAG.to_vec();
            out.push(self.byte());
            out.extend(plain.iter().map(|b| b ^ self.byte()));
            Ok(out)
        }

        fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, SealError> {
            if take(&self.0.open_failures) {
                return Err(SealError::Unavailable("open".into()));
            }
            let body = sealed
                .strip_prefix(TAG)
                .ok_or_else(|| SealError::Invalid("tag".into()))?;
            let (&key, rest) = body
                .split_first()
                .ok_or_else(|| SealError::Invalid("short".into()))?;
            if key != self.byte() {
                return Err(SealError::Invalid("key".into()));
            }
            Ok(rest.iter().map(|b| b ^ key).collect())
        }

        fn reset(&self) {
            self.0.resets.fetch_add(1, Ordering::SeqCst);
            self.0.key.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "yap-vault-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn sealed(&self) -> PathBuf {
            self.0.join(SEALED_FILE)
        }
        fn legacy(&self) -> PathBuf {
            self.0.join(LEGACY_FILE)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn vault(dir: &Dir) -> (Vault<Fake>, Fake) {
        let fake = Fake::default();
        (Vault::in_dir(fake.clone(), &dir.0), fake)
    }

    const TOKEN: &str = "tok-abc.123";

    #[test]
    fn stores_and_loads_sealed() {
        let dir = Dir::new();
        let (vault, _) = vault(&dir);
        assert_eq!(vault.load(), None);
        vault.store(TOKEN);
        let on_disk = fs::read(dir.sealed()).unwrap();
        assert!(!on_disk.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()));
        assert!(!dir.legacy().exists());
        assert!(!tmp_path(&dir.sealed()).exists());
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
    }

    #[test]
    fn store_replaces_the_last_token() {
        let dir = Dir::new();
        let (vault, _) = vault(&dir);
        vault.store("first");
        vault.store("second");
        assert_eq!(vault.load().as_deref(), Some("second"));
    }

    #[test]
    fn migrates_a_plain_token() {
        let dir = Dir::new();
        fs::write(dir.legacy(), format!("{TOKEN}\n")).unwrap();
        let (vault, _) = vault(&dir);
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
        assert!(!dir.legacy().exists());
        assert!(dir.sealed().exists());
        // The next start reads the sealed file.
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
    }

    #[test]
    fn a_failed_migration_keeps_no_plain_copy() {
        let dir = Dir::new();
        fs::write(dir.legacy(), TOKEN).unwrap();
        let (vault, fake) = vault(&dir);
        fake.fail_seals(2);
        // This run stays signed in; the next one is signed out.
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
        assert!(!dir.legacy().exists());
        assert!(!dir.sealed().exists());
        assert_eq!(vault.load(), None);
    }

    #[test]
    fn an_empty_plain_file_is_dropped() {
        let dir = Dir::new();
        fs::write(dir.legacy(), " \n").unwrap();
        let (vault, _) = vault(&dir);
        assert_eq!(vault.load(), None);
        assert!(!dir.legacy().exists());
        assert!(!dir.sealed().exists());
    }

    #[test]
    fn a_stale_plain_copy_goes_and_the_sealed_token_wins() {
        let dir = Dir::new();
        let (vault, _) = vault(&dir);
        vault.store(TOKEN);
        fs::write(dir.legacy(), "older").unwrap();
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
        assert!(!dir.legacy().exists());
    }

    #[test]
    fn a_changed_key_signs_out() {
        let dir = Dir::new();
        let (vault, fake) = vault(&dir);
        vault.store(TOKEN);
        // The key was invalidated or replaced behind the app's back.
        fake.0.key.fetch_add(7, Ordering::SeqCst);
        assert_eq!(vault.load(), None);
        assert!(!dir.sealed().exists());
        assert_eq!(fake.resets(), 1);
        // A new sign-in works with the new key.
        vault.store("next");
        assert_eq!(vault.load().as_deref(), Some("next"));
    }

    #[test]
    fn damaged_bytes_sign_out() {
        let dir = Dir::new();
        fs::write(dir.sealed(), b"garbage").unwrap();
        let (vault, fake) = vault(&dir);
        assert_eq!(vault.load(), None);
        assert!(!dir.sealed().exists());
        assert_eq!(fake.resets(), 1);
    }

    #[test]
    fn one_failed_open_is_tried_again() {
        let dir = Dir::new();
        let (vault, fake) = vault(&dir);
        vault.store(TOKEN);
        fake.fail_opens(1);
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
        assert_eq!(fake.resets(), 0);
        assert!(dir.sealed().exists());
    }

    #[test]
    fn an_unavailable_key_store_keeps_the_file() {
        let dir = Dir::new();
        let (vault, fake) = vault(&dir);
        vault.store(TOKEN);
        fake.fail_opens(2);
        // Signed out for this run, nothing dropped.
        assert_eq!(vault.load(), None);
        assert_eq!(fake.resets(), 0);
        assert!(dir.sealed().exists());
        // The next start opens it.
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
    }

    #[test]
    fn a_failed_seal_gets_a_new_key() {
        let dir = Dir::new();
        let (vault, fake) = vault(&dir);
        fake.fail_seals(1);
        vault.store(TOKEN);
        assert_eq!(fake.resets(), 1);
        assert_eq!(vault.load().as_deref(), Some(TOKEN));
    }

    #[test]
    fn a_store_that_cannot_seal_drops_the_older_token() {
        let dir = Dir::new();
        let (vault, fake) = vault(&dir);
        vault.store("someone-else");
        fake.fail_seals(2);
        vault.store(TOKEN);
        assert!(!dir.sealed().exists());
        assert_eq!(vault.load(), None);
    }

    #[test]
    fn clear_removes_every_copy() {
        let dir = Dir::new();
        let (vault, fake) = vault(&dir);
        vault.store(TOKEN);
        fs::write(dir.legacy(), TOKEN).unwrap();
        fs::write(tmp_path(&dir.sealed()), b"partial").unwrap();
        vault.clear();
        assert!(!dir.sealed().exists());
        assert!(!dir.legacy().exists());
        assert!(!tmp_path(&dir.sealed()).exists());
        assert_eq!(fake.resets(), 0);
        assert_eq!(vault.load(), None);
    }
}
