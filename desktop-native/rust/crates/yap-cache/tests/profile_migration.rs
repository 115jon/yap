//! The one-time move of the data folder from `%LOCALAPPDATA%\YapNative`
//! to `%LOCALAPPDATA%\Yap.Data` (`profile::migrate`, `profile::resolve_root`),
//! on temporary folders only.

use std::fs;
use std::path::Path;

use yap_cache::profile::{
    data_root_in, legacy_root_in, migrate, resolve_root, Migration, DATA_DIR, LEGACY_DATA_DIR,
    MIGRATED_MARKER, STAGING_DIR,
};

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("read")
}

/// An old layout with the default profile and three named ones.
fn old_layout(base: &Path) {
    let old = base.join(LEGACY_DATA_DIR);
    write(
        &old.join("desktop-settings.json"),
        r#"{"closeToTray":false}"#,
    );
    write(&old.join("cache").join("snapshot.db"), "default-db");
    write(&old.join("logs").join("installer.log"), "old installer log");
    for name in ["spike", "spikepeer", "jon"] {
        let root = old.join("profiles").join(name);
        write(&root.join("desktop-settings.json"), name);
        write(
            &root.join("cache").join("snapshot.db"),
            &format!("{name}-db"),
        );
    }
}

#[test]
fn a_fresh_start_has_nothing_to_move_and_marks_the_new_folder() {
    let base = tempfile::tempdir().expect("dir");
    assert_eq!(migrate(base.path()), Migration::NothingToMove);
    assert!(base.path().join(DATA_DIR).join(MIGRATED_MARKER).is_file());
    assert_eq!(migrate(base.path()), Migration::AlreadyDone);
}

#[test]
fn the_old_folder_moves_whole_with_every_named_profile() {
    let base = tempfile::tempdir().expect("dir");
    old_layout(base.path());

    assert_eq!(migrate(base.path()), Migration::Renamed);

    assert!(!base.path().join(LEGACY_DATA_DIR).exists());
    let new = base.path().join(DATA_DIR);
    assert_eq!(
        read(&new.join("desktop-settings.json")),
        r#"{"closeToTray":false}"#
    );
    assert_eq!(read(&new.join("cache").join("snapshot.db")), "default-db");
    for name in ["spike", "spikepeer", "jon"] {
        let root = data_root_in(base.path(), Some(name));
        assert_eq!(read(&root.join("desktop-settings.json")), name);
        assert_eq!(
            read(&root.join("cache").join("snapshot.db")),
            format!("{name}-db")
        );
    }
}

#[test]
fn it_runs_once_and_never_takes_a_folder_an_older_build_makes_again() {
    let base = tempfile::tempdir().expect("dir");
    old_layout(base.path());
    assert_eq!(migrate(base.path()), Migration::Renamed);

    // The older build, still installed, starts and makes its folder again.
    let again = base
        .path()
        .join(LEGACY_DATA_DIR)
        .join("desktop-settings.json");
    write(&again, "older build");

    assert_eq!(migrate(base.path()), Migration::AlreadyDone);
    assert_eq!(read(&again), "older build");
    let (root, _) = resolve_root(base.path(), None);
    assert_eq!(root, base.path().join(DATA_DIR));
    assert_eq!(
        read(&root.join("desktop-settings.json")),
        r#"{"closeToTray":false}"#
    );
}

#[test]
fn an_existing_new_folder_is_merged_into_and_conflicts_are_kept_aside() {
    let base = tempfile::tempdir().expect("dir");
    old_layout(base.path());
    // The install hook logged into the new folder before the first start.
    let new = base.path().join(DATA_DIR);
    write(&new.join("logs").join("installer.log"), "new installer log");

    let outcome = migrate(base.path());

    let Migration::Merged { conflicts, left } = outcome else {
        panic!("expected a merge, got {outcome:?}");
    };
    assert!(left.is_empty(), "nothing should be left: {left:?}");
    assert_eq!(
        conflicts,
        vec![new.join("logs").join("installer.legacy.log")]
    );
    assert_eq!(
        read(&new.join("logs").join("installer.log")),
        "new installer log"
    );
    assert_eq!(
        read(&new.join("logs").join("installer.legacy.log")),
        "old installer log"
    );
    assert_eq!(
        read(&new.join("desktop-settings.json")),
        r#"{"closeToTray":false}"#
    );
    for name in ["spike", "spikepeer", "jon"] {
        let root = data_root_in(base.path(), Some(name));
        assert_eq!(read(&root.join("desktop-settings.json")), name);
    }
    // Nothing is left behind: the staging folder and the old one are gone.
    assert!(!base.path().join(STAGING_DIR).exists());
    assert!(!base.path().join(LEGACY_DATA_DIR).exists());
    assert!(new.join(MIGRATED_MARKER).is_file());
}

#[test]
fn an_interrupted_merge_resumes_from_the_staging_folder() {
    let base = tempfile::tempdir().expect("dir");
    let staging = base.path().join(STAGING_DIR);
    write(
        &staging.join("profiles").join("jon").join("games.json"),
        "jon games",
    );
    write(
        &base.path().join(DATA_DIR).join("desktop-settings.json"),
        "moved already",
    );

    assert!(matches!(migrate(base.path()), Migration::Merged { .. }));
    assert_eq!(
        read(&data_root_in(base.path(), Some("jon")).join("games.json")),
        "jon games"
    );
    assert!(!staging.exists());
}

#[test]
fn a_merge_that_leaves_something_behind_is_not_marked_done() {
    let base = tempfile::tempdir().expect("dir");
    let new = base.path().join(DATA_DIR);
    let staging = base.path().join(STAGING_DIR);
    // A file both have, and its `.legacy` name taken too: it cannot move.
    write(&staging.join("games.json"), "old games");
    write(&new.join("games.json"), "new games");
    write(&new.join("games.legacy.json"), "older games");

    let outcome = migrate(base.path());

    let Migration::Merged { left, .. } = &outcome else {
        panic!("expected a merge, got {outcome:?}");
    };
    assert_eq!(left, &vec![staging.join("games.json")]);
    assert!(!new.join(MIGRATED_MARKER).exists());
    assert_eq!(read(&staging.join("games.json")), "old games");
    // The run still uses the new folder.
    assert!(!outcome.uses_legacy());

    // Once the way is clear, the next start finishes and marks it.
    fs::remove_file(new.join("games.legacy.json")).expect("rm");
    assert!(matches!(migrate(base.path()), Migration::Merged { ref left, .. } if left.is_empty()));
    assert!(new.join(MIGRATED_MARKER).is_file());
    assert!(!staging.exists());
    assert_eq!(read(&new.join("games.legacy.json")), "old games");
}

#[test]
fn resolve_logs_the_move_in_the_new_folder() {
    let base = tempfile::tempdir().expect("dir");
    old_layout(base.path());

    let (root, outcome) = resolve_root(base.path(), Some("spike"));

    assert_eq!(outcome, Migration::Renamed);
    assert_eq!(root, data_root_in(base.path(), Some("spike")));
    let log = read(&root.join("logs").join("migration.log"));
    assert!(log.contains("Renamed"), "{log}");
}

/// A file held open without delete sharing blocks the rename of the folder
/// it is in, as a running older build does. Windows only: elsewhere an open
/// file never stops a rename.
#[cfg(windows)]
#[test]
fn a_locked_old_folder_is_read_in_place_and_moved_on_a_later_start() {
    use std::os::windows::fs::OpenOptionsExt;

    let base = tempfile::tempdir().expect("dir");
    old_layout(base.path());
    let held = legacy_root_in(base.path(), Some("jon")).join("desktop-settings.json");
    // FILE_SHARE_READ only: no FILE_SHARE_DELETE, as a writer keeps it.
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1)
        .open(&held)
        .expect("open");

    let (root, outcome) = resolve_root(base.path(), Some("spike"));
    assert!(outcome.uses_legacy(), "{outcome:?}");
    assert_eq!(root, legacy_root_in(base.path(), Some("spike")));
    assert_eq!(read(&root.join("desktop-settings.json")), "spike");
    // Nothing moved and no marker: the next start tries again.
    assert!(!base.path().join(DATA_DIR).join(MIGRATED_MARKER).exists());
    assert_eq!(read(&held), "jon");

    drop(handle);
    let (root, outcome) = resolve_root(base.path(), Some("spike"));
    assert_eq!(outcome, Migration::Renamed);
    assert_eq!(root, data_root_in(base.path(), Some("spike")));
    assert_eq!(
        read(&data_root_in(base.path(), Some("jon")).join("desktop-settings.json")),
        "jon"
    );
}
