//! The in-app updater's rules (`yap_core::update`): versions and pins, the
//! channels' feeds, which copies update, the check schedule with its
//! backoff, and the restart prompt that waits for the end of a call.

use std::path::PathBuf;
use std::time::Duration;

use yap_core::update::{
    apply_at_start, eligibility, feed, friendly_error, github_asset_url, github_feed_url,
    github_releases_url, github_wants_next_page, is_newer, load_prefs, merge_feeds,
    native_releases, newest_full, parse_feed, parse_feed_file, parse_github_releases,
    parse_version, pin_allows, plain_notes, release_tag_version, restart_args, save_prefs,
    short_error, Channel, Feed, FeedAsset, FeedOrigin, Launch, Off, Phase, Prefs, Timing, Updater,
    CHECK_EVERY, DEFAULT_FEED_URL, FIRST_CHECK, GITHUB_MAX_PAGES, GITHUB_PAGE_SIZE,
    NATIVE_MIN_MAJOR, PREFS_FILE, RELEASE_TAG_PREFIX, RETRY_FIRST, RETRY_MAX,
};

const ROOT: &str = r"C:\Users\u\AppData\Local\Yap.App";
const EXE: &str = r"C:\Users\u\AppData\Local\Yap.App\current\Yap.exe";

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("update")
        .join(name);
    std::fs::read_to_string(path).expect("fixture")
}

// Versions

#[test]
fn versions_should_compare_as_semver() {
    assert!(is_newer("0.2.23", "0.2.22"));
    assert!(is_newer("0.2.10", "0.2.9"), "numeric, not text order");
    assert!(is_newer("0.3.0", "0.2.99"));
    assert!(is_newer("1.0.0", "0.99.99"));
    assert!(!is_newer("0.2.22", "0.2.22"));
    assert!(!is_newer("0.2.21", "0.2.22"));
    // A pre-release sorts before its release, after the one before.
    assert!(is_newer("0.2.23", "0.2.23-beta.1"));
    assert!(is_newer("0.2.23-beta.1", "0.2.22"));
    assert!(is_newer("0.2.23-beta.2", "0.2.23-beta.1"));
    assert!(is_newer("0.2.23-beta.10", "0.2.23-beta.9"));
}

#[test]
fn versions_should_take_a_leading_v_and_refuse_junk() {
    assert_eq!(parse_version("v0.2.22"), parse_version("0.2.22"));
    assert_eq!(
        parse_version(" 0.2.22 ").map(|v| v.to_string()),
        Some("0.2.22".into())
    );
    assert_eq!(parse_version("0.2"), None);
    assert_eq!(parse_version(""), None);
    assert_eq!(parse_version("latest"), None);
    assert!(!is_newer("junk", "0.2.22"));
    assert!(!is_newer("0.2.23", "junk"));
}

#[test]
fn a_pin_should_cap_the_version() {
    let pin = parse_version("0.2.22").unwrap();
    assert!(pin_allows(&parse_version("0.2.22").unwrap(), Some(&pin)));
    assert!(pin_allows(&parse_version("0.2.21").unwrap(), Some(&pin)));
    assert!(!pin_allows(&parse_version("0.2.23").unwrap(), Some(&pin)));
    assert!(!pin_allows(
        &parse_version("0.2.23-beta.1").unwrap(),
        Some(&pin)
    ));
    assert!(pin_allows(&parse_version("9.9.9").unwrap(), None));
}

// The feed files

#[test]
fn a_vpk_feed_should_parse() {
    let feed = parse_feed_file(&fixture("releases.win.json")).unwrap();
    assert_eq!(feed.assets.len(), 3);
    let first = &feed.assets[0];
    assert_eq!(first.package_id, "Yap.App");
    assert_eq!(first.version, "0.2.22");
    assert_eq!(first.kind, "Full");
    assert_eq!(first.file_name, "Yap.App-0.2.22-full.nupkg");
    assert_eq!(first.size, 37_373_156);
    assert_eq!(first.sha256.len(), 64);
    let (newest, version) = newest_full(&feed.assets).unwrap();
    assert_eq!(version.to_string(), "0.2.23");
    assert_eq!(newest.file_name, "Yap.App-0.2.23-full.nupkg");
    assert!(newest
        .notes_markdown
        .contains("Updates install from inside the app."));
    assert_eq!(feed.assets[2].kind, "Delta");
}

#[test]
fn a_feed_should_parse_without_optional_fields_and_in_camel_case() {
    let feed = parse_feed_file(
        r#"{"assets":[{"packageId":"Yap.App","version":"0.2.22","type":"Full","fileName":"a.nupkg"}]}"#,
    )
    .unwrap();
    assert_eq!(feed.assets[0].version, "0.2.22");
    assert_eq!(feed.assets[0].size, 0);
    assert!(parse_feed_file("{}").unwrap().assets.is_empty());
    assert!(parse_feed_file("<html>404</html>").is_err());
}

#[test]
fn beta_should_merge_the_stable_feed_and_skip_bad_versions() {
    let beta = parse_feed_file(&fixture("releases.beta.json"))
        .unwrap()
        .assets;
    let stable = parse_feed_file(&fixture("releases.win.json"))
        .unwrap()
        .assets;
    let merged: Vec<FeedAsset> = merge_feeds(&[beta.clone(), stable.clone()], None);
    assert_eq!(merged.len(), 4, "the broken entry is dropped");
    assert_eq!(newest_full(&merged).unwrap().1.to_string(), "0.2.24-beta.1");

    // The same file in both feeds counts once.
    let twice = merge_feeds(&[stable.clone(), stable.clone()], None);
    assert_eq!(twice.len(), 3);

    // A pin drops everything past it, deltas included.
    let pin = parse_version("0.2.22").unwrap();
    let pinned = merge_feeds(&[beta, stable], Some(&pin));
    assert_eq!(pinned.len(), 1);
    assert_eq!(newest_full(&pinned).unwrap().1.to_string(), "0.2.22");
}

#[test]
fn the_channels_should_name_velopack_feeds() {
    assert_eq!(Channel::default(), Channel::Stable);
    assert_eq!(Channel::Stable.velopack_name(), "win");
    assert_eq!(Channel::Beta.velopack_name(), "beta");
    assert_eq!(Channel::Stable.feeds(), ["win"]);
    assert_eq!(Channel::Beta.feeds(), ["beta", "win"]);
}

// Where the feed is

#[test]
fn the_feed_should_come_from_env_then_prefs_then_the_default() {
    assert_eq!(
        feed(None, None),
        (Feed::Github("115jon/yap".into()), FeedOrigin::Default)
    );
    assert!(DEFAULT_FEED_URL.starts_with("https://"));
    assert!(!DEFAULT_FEED_URL.contains(".site"));
    assert!(!DEFAULT_FEED_URL.contains("updates.115jon.com"));
    assert_eq!(
        feed(None, None).0.to_string(),
        "https://github.com/115jon/yap"
    );
    assert_eq!(
        feed(
            Some("file:///F:/ct/yapn/run/updater/feed"),
            Some("https://x.example/f")
        ),
        (
            Feed::Dir(PathBuf::from(r"F:\ct\yapn\run\updater\feed")),
            FeedOrigin::Env
        )
    );
    assert_eq!(
        feed(None, Some("https://x.example/f/")),
        (Feed::Http("https://x.example/f".into()), FeedOrigin::Prefs)
    );
    // One that does not parse is skipped.
    assert_eq!(feed(Some("feed"), None).1, FeedOrigin::Default);
}

#[test]
fn feed_locations_should_parse() {
    assert_eq!(
        parse_feed("http://127.0.0.1:8080/feed"),
        Some(Feed::Http("http://127.0.0.1:8080/feed".into()))
    );
    assert_eq!(
        parse_feed(r"F:\feed\"),
        Some(Feed::Dir(PathBuf::from(r"F:\feed")))
    );
    assert_eq!(
        parse_feed("F:/my feed"),
        Some(Feed::Dir(PathBuf::from(r"F:\my feed")))
    );
    assert_eq!(
        parse_feed("file:///F:/my%20feed/"),
        Some(Feed::Dir(PathBuf::from(r"F:\my feed")))
    );
    assert_eq!(
        parse_feed("file://nas/share/yap"),
        Some(Feed::Dir(PathBuf::from(r"\\nas\share\yap")))
    );
    // A repository reads its native releases; a release's download folder
    // is a plain web folder (one fixed release's feed).
    assert_eq!(
        parse_feed("https://github.com/115jon/yap"),
        Some(Feed::Github("115jon/yap".into()))
    );
    assert_eq!(
        parse_feed("https://GitHub.com/someone/fork.git/"),
        Some(Feed::Github("someone/fork".into()))
    );
    assert_eq!(
        parse_feed("https://github.com/115jon/yap/releases/download/desktop-native-v0.2.24"),
        Some(Feed::Http(
            "https://github.com/115jon/yap/releases/download/desktop-native-v0.2.24".into()
        ))
    );
    assert_eq!(
        parse_feed("https://github.com/115jon"),
        Some(Feed::Http("https://github.com/115jon".into()))
    );
    assert_eq!(
        parse_feed("https://example.com/115jon/yap"),
        Some(Feed::Http("https://example.com/115jon/yap".into()))
    );
    assert_eq!(parse_feed("relative/feed"), None);
    assert_eq!(parse_feed("https://"), None);
    assert_eq!(parse_feed("ftp://x/y"), None);
    assert_eq!(parse_feed("file:F:/x"), None);
    assert_eq!(parse_feed(""), None);
}

// GitHub Releases

fn github_fixture() -> Vec<yap_core::update::GithubRelease> {
    parse_github_releases(&fixture("github-releases.json")).expect("releases parse")
}

fn tags(releases: &[&yap_core::update::GithubRelease]) -> Vec<String> {
    releases.iter().map(|r| r.tag_name.clone()).collect()
}

#[test]
fn native_release_tags_should_carry_a_semver() {
    assert_eq!(RELEASE_TAG_PREFIX, "desktop-native-v");
    assert_eq!(NATIVE_MIN_MAJOR, 4);
    assert_eq!(
        release_tag_version("desktop-native-v0.2.24")
            .unwrap()
            .to_string(),
        "0.2.24"
    );
    assert_eq!(
        release_tag_version("desktop-native-v0.2.25-beta.1")
            .unwrap()
            .to_string(),
        "0.2.25-beta.1"
    );
    // The plain line is the native app's from 4.0.0.
    assert_eq!(release_tag_version("v4.0.0").unwrap().to_string(), "4.0.0");
    assert_eq!(
        release_tag_version("v4.1.0-beta.1").unwrap().to_string(),
        "4.1.0-beta.1"
    );
    assert_eq!(
        release_tag_version("v12.0.3").unwrap().to_string(),
        "12.0.3"
    );
    for tag in [
        "v3.13.0",
        "v3.14.0",
        "v3.99.99-beta.1",
        "v0.2.24",
        "v4",
        "v4.0",
        "V4.0.0",
        "4.0.0",
        "desktop-native-v3",
        "mobile-v0.5.1",
        "native-toolchain-qt6.11.3-yap-webrtc-a545de4",
        "desktop-native-vnext",
        "desktop-native-0.2.24",
        "cef-build-assets",
    ] {
        assert_eq!(release_tag_version(tag), None, "{tag}");
    }
}

#[test]
fn stable_should_read_only_published_native_stable_releases_newest_first() {
    let releases = github_fixture();
    let stable = native_releases(&releases, Channel::Stable);
    // No draft, no Tauri (v3.x)/mobile/toolchain release, no pre-release;
    // semver order (4.0.1 before 4.0.0 before 0.2.24 before 0.2.3), not the
    // API's. v4.0.1 is listed here and skipped for its feed later.
    assert_eq!(
        tags(&stable),
        [
            "v4.0.1",
            "v4.0.0",
            "desktop-native-v0.2.24",
            "desktop-native-v0.2.3"
        ]
    );
}

#[test]
fn the_tauri_v3_line_should_never_be_an_update_source() {
    let releases = github_fixture();
    // The fixture's v3.14.0 has latest.json and even a releases.win.json.
    for channel in [Channel::Stable, Channel::Beta] {
        let read = native_releases(&releases, channel);
        assert!(!tags(&read).contains(&"v3.14.0".to_string()), "{channel:?}");
        assert_eq!(github_asset_url(&read, "latest.json"), None, "{channel:?}");
    }
    let stable = native_releases(&releases, Channel::Stable);
    assert!(!github_feed_url(&stable, "win").unwrap().contains("v3.14.0"));
}

#[test]
fn beta_should_read_pre_releases_too() {
    let releases = github_fixture();
    let beta = native_releases(&releases, Channel::Beta);
    assert_eq!(
        tags(&beta),
        [
            "v4.1.0-beta.1",
            "v4.0.1",
            "v4.0.0",
            "desktop-native-v0.2.25-beta.1",
            "desktop-native-v0.2.24",
            "desktop-native-v0.2.3"
        ]
    );
}

#[test]
fn a_github_pre_release_or_beta_version_should_never_reach_stable() {
    let mut releases = github_fixture();
    // A stable-looking tag marked pre-release on GitHub, and a beta version
    // published as a full release.
    releases.push(yap_core::update::GithubRelease {
        tag_name: "desktop-native-v0.2.30".into(),
        prerelease: true,
        ..Default::default()
    });
    releases.push(yap_core::update::GithubRelease {
        tag_name: "desktop-native-v0.2.31-beta.2".into(),
        ..Default::default()
    });
    let stable = tags(&native_releases(&releases, Channel::Stable));
    assert!(!stable
        .iter()
        .any(|t| t.ends_with("0.2.30") || t.contains("beta")));
    let beta = tags(&native_releases(&releases, Channel::Beta));
    assert!(beta.contains(&"desktop-native-v0.2.31-beta.2".to_string()));
    assert!(beta.contains(&"desktop-native-v0.2.30".to_string()));
}

#[test]
fn the_feed_file_should_come_from_the_newest_release_that_has_it() {
    let releases = github_fixture();
    let stable = native_releases(&releases, Channel::Stable);
    // v4.0.1 is newer but has no feed (nothing uploaded yet): v4.0.0's is
    // read, not an older legacy release's.
    assert_eq!(
        github_feed_url(&stable, "win"),
        Some("https://github.com/115jon/yap/releases/download/v4.0.0/releases.win.json")
    );
    assert_eq!(github_feed_url(&stable, "beta"), None);
    let beta = native_releases(&releases, Channel::Beta);
    assert_eq!(
        github_feed_url(&beta, "beta"),
        Some("https://github.com/115jon/yap/releases/download/v4.1.0-beta.1/releases.beta.json")
    );
    // The beta channel's stable feed is still the newest stable release's.
    assert_eq!(
        github_feed_url(&beta, "win"),
        github_feed_url(&stable, "win")
    );
    // Nothing published: no feed (the source reports an empty one).
    assert_eq!(github_feed_url(&[], "win"), None);
}

#[test]
fn a_legacy_feed_should_serve_until_a_v4_release_carries_one() {
    let releases: Vec<_> = github_fixture()
        .into_iter()
        .filter(|r| !r.tag_name.starts_with('v'))
        .collect();
    let stable = native_releases(&releases, Channel::Stable);
    // Asset names match case-insensitively, as GitHub's do (the fixture's
    // 0.2.24 file is "Releases.Win.json").
    assert_eq!(
        github_feed_url(&stable, "win"),
        Some("https://github.com/115jon/yap/releases/download/desktop-native-v0.2.24/releases.win.json")
    );
    let beta = native_releases(&releases, Channel::Beta);
    assert_eq!(
        github_feed_url(&beta, "beta"),
        Some("https://github.com/115jon/yap/releases/download/desktop-native-v0.2.25-beta.1/releases.beta.json")
    );
}

#[test]
fn packages_should_download_from_the_release_that_has_them() {
    let releases = github_fixture();
    let stable = native_releases(&releases, Channel::Stable);
    assert_eq!(
        github_asset_url(&stable, "Yap.App-4.0.0-full.nupkg"),
        Some("https://github.com/115jon/yap/releases/download/v4.0.0/Yap.App-4.0.0-full.nupkg")
    );
    assert_eq!(
        github_asset_url(&stable, "Yap.App-0.2.24-delta.nupkg"),
        Some("https://github.com/115jon/yap/releases/download/desktop-native-v0.2.24/Yap.App-0.2.24-delta.nupkg")
    );
    assert_eq!(
        github_asset_url(&stable, "Yap.App-0.2.3-full.nupkg"),
        Some("https://github.com/115jon/yap/releases/download/desktop-native-v0.2.3/Yap.App-0.2.3-full.nupkg")
    );
    // A stable copy never downloads a beta's package.
    assert_eq!(
        github_asset_url(&stable, "Yap.App-0.2.25-beta.1-full.nupkg"),
        None
    );
    assert_eq!(
        github_asset_url(&stable, "Yap.App-4.1.0-beta.1-full.nupkg"),
        None
    );
    assert_eq!(github_asset_url(&stable, "Yap.App-9.9.9-full.nupkg"), None);
}

#[test]
fn the_release_listing_should_page_only_while_nothing_native_turned_up() {
    assert_eq!(GITHUB_PAGE_SIZE, 100);
    assert_eq!(
        github_releases_url("115jon/yap", 2),
        "https://api.github.com/repos/115jon/yap/releases?per_page=100&page=2"
    );
    assert!(github_wants_next_page(1, GITHUB_PAGE_SIZE, false));
    assert!(!github_wants_next_page(1, GITHUB_PAGE_SIZE, true));
    assert!(!github_wants_next_page(1, 59, false), "the last page");
    assert!(!github_wants_next_page(
        GITHUB_MAX_PAGES,
        GITHUB_PAGE_SIZE,
        false
    ));
}

#[test]
fn the_api_listing_should_parse_with_unknown_fields() {
    let releases = parse_github_releases(
        r#"[{"tag_name":"desktop-native-v0.2.24","draft":false,"prerelease":false,
            "html_url":"https://github.com/115jon/yap/releases/tag/desktop-native-v0.2.24",
            "assets":[{"name":"a","browser_download_url":"https://x/a","size":1}]}]"#,
    )
    .unwrap();
    assert_eq!(releases[0].asset_url("A"), Some("https://x/a"));
    assert!(parse_github_releases(r#"{"message":"API rate limit exceeded"}"#).is_err());
}

#[test]
fn a_rate_limited_check_should_read_as_unreachable() {
    assert_eq!(
        friendly_error("Http error: http status: 403 API rate limit exceeded"),
        "Yap couldn't reach the update server."
    );
}

// Which copies update

fn launch<'a>(exe: &'a str, profile: Option<&'a str>, test: bool) -> Launch<'a> {
    Launch { exe, profile, test }
}

#[test]
fn only_an_installed_default_profile_should_update() {
    let installed = |p: &str| p.eq_ignore_ascii_case(&format!(r"{ROOT}\Update.exe"));
    assert_eq!(
        eligibility(&launch(EXE, None, false), installed),
        Ok(ROOT.into())
    );

    // A test pack id updates too: it is a real Velopack layout.
    let test_root = r"C:\Users\u\AppData\Local\Yap.InstallTest";
    let test_exe = format!(r"{test_root}\current\Yap.exe");
    let test_installed = |p: &str| p.eq_ignore_ascii_case(&format!(r"{test_root}\Update.exe"));
    assert_eq!(
        eligibility(&launch(&test_exe, None, false), test_installed),
        Ok(test_root.into())
    );
}

#[test]
fn dev_builds_and_copies_should_not_update() {
    let nothing = |_: &str| false;
    // A build folder, a copy, a layout without Update.exe.
    for exe in [
        r"F:\ct\yapn\rel\Yap.exe",
        r"C:\Users\u\Desktop\Yap.exe",
        r"C:\Users\u\AppData\Local\Yap.App\current\Yap.exe",
    ] {
        assert_eq!(
            eligibility(&launch(exe, None, false), nothing),
            Err(Off::NotInstalled),
            "{exe}"
        );
        // The test hook never makes a copy update.
        assert_eq!(
            eligibility(&launch(exe, None, true), nothing),
            Err(Off::NotInstalled)
        );
    }
    // Update.exe next to the exe, not one level up.
    let beside = |p: &str| p.eq_ignore_ascii_case(r"C:\x\Update.exe");
    assert_eq!(
        eligibility(&launch(r"C:\x\Yap.exe", None, false), beside),
        Err(Off::NotInstalled)
    );
}

#[test]
fn portable_installs_should_not_update() {
    let portable = |p: &str| {
        p.eq_ignore_ascii_case(&format!(r"{ROOT}\Update.exe"))
            || p.eq_ignore_ascii_case(&format!(r"{ROOT}\.portable"))
    };
    assert_eq!(
        eligibility(&launch(EXE, None, false), portable),
        Err(Off::Portable)
    );
    assert_eq!(
        eligibility(&launch(EXE, None, true), portable),
        Err(Off::Portable)
    );
}

#[test]
fn named_profiles_and_the_dev_folder_should_not_update_unless_testing() {
    let installed = |p: &str| p.eq_ignore_ascii_case(&format!(r"{ROOT}\Update.exe"));
    assert_eq!(
        eligibility(&launch(EXE, Some("spike"), false), installed),
        Err(Off::NamedProfile)
    );
    let dev_root = r"F:\ct\yapn\run\updater\la\Yap.UpdateTest";
    let dev_exe = format!(r"{dev_root}\current\Yap.exe");
    let dev_installed = |p: &str| p.eq_ignore_ascii_case(&format!(r"{dev_root}\Update.exe"));
    assert_eq!(
        eligibility(&launch(&dev_exe, None, false), dev_installed),
        Err(Off::DevFolder)
    );
    // Forward slashes and case do not hide the dev folder.
    let dev_exe_slashes = "f:/CT/yapn/run/x/current/Yap.exe";
    assert_eq!(
        eligibility(&launch(dev_exe_slashes, None, false), |p: &str| p
            .eq_ignore_ascii_case(r"f:\CT\yapn\run\x\Update.exe")),
        Err(Off::DevFolder)
    );
    // YAP_UPDATE_TEST: the scratch root of an end-to-end run.
    assert_eq!(
        eligibility(&launch(&dev_exe, Some("spike"), true), dev_installed),
        Ok(dev_root.into())
    );
    // A folder that only starts like the dev folder is not it.
    let other = r"F:\ct\yapnx\Yap.App";
    let other_exe = format!(r"{other}\current\Yap.exe");
    assert_eq!(
        eligibility(&launch(&other_exe, None, false), |p: &str| p
            .eq_ignore_ascii_case(&format!(r"{other}\Update.exe"))),
        Ok(other.into())
    );
}

#[test]
fn every_off_reason_should_have_words() {
    for off in [
        Off::NotInstalled,
        Off::Portable,
        Off::NamedProfile,
        Off::DevFolder,
    ] {
        assert!(off.reason().ends_with('.'));
        assert!(!off.reason().contains("Velopack"));
    }
}

#[test]
fn start_should_apply_a_download_only_with_install_on_quit_and_no_pin() {
    let prefs = Prefs::default();
    assert!(prefs.install_on_quit, "on by default");
    assert!(apply_at_start(true, &prefs));
    assert!(!apply_at_start(false, &prefs));
    let off = Prefs {
        install_on_quit: false,
        ..Prefs::default()
    };
    assert!(!apply_at_start(true, &off));
    let pinned = Prefs {
        pinned_version: Some("0.2.22".into()),
        ..Prefs::default()
    };
    assert!(!apply_at_start(true, &pinned));
}

#[test]
fn the_restart_should_keep_the_profile() {
    assert!(restart_args(None).is_empty());
    assert_eq!(restart_args(Some("spike")), ["--profile", "spike"]);
}

// Preferences

#[test]
fn prefs_should_round_trip_and_default() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(load_prefs(dir.path()), Prefs::default());
    let prefs = Prefs {
        channel: Channel::Beta,
        install_on_quit: false,
        pinned_version: Some("0.2.22".into()),
        feed_url: Some("file:///F:/feed".into()),
    };
    save_prefs(dir.path(), &prefs).unwrap();
    assert_eq!(load_prefs(dir.path()), prefs);
    let text = std::fs::read_to_string(dir.path().join(PREFS_FILE)).unwrap();
    assert!(text.contains("\"channel\": \"beta\""), "{text}");
    assert!(text.contains("\"installOnQuit\": false"), "{text}");
    assert!(text.contains("\"pinnedVersion\": \"0.2.22\""), "{text}");
    assert_eq!(prefs.pin().unwrap().to_string(), "0.2.22");

    // A partial or broken file keeps the defaults for what it lacks.
    std::fs::write(dir.path().join(PREFS_FILE), r#"{"channel":"beta"}"#).unwrap();
    let partial = load_prefs(dir.path());
    assert_eq!(partial.channel, Channel::Beta);
    assert!(partial.install_on_quit);
    std::fs::write(dir.path().join(PREFS_FILE), "not json").unwrap();
    assert_eq!(load_prefs(dir.path()), Prefs::default());
    // A pin that does not parse pins nothing.
    let junk = Prefs {
        pinned_version: Some("soon".into()),
        ..Prefs::default()
    };
    assert_eq!(junk.pin(), None);
}

// The schedule

#[test]
fn timing_should_default_and_follow_the_test_hooks() {
    let t = Timing::default();
    assert_eq!(t.first, FIRST_CHECK);
    assert_eq!(t.every, CHECK_EVERY);
    assert_eq!(t.every, secs(6 * 60 * 60));
    assert_eq!(t.retry_first, RETRY_FIRST);
    assert_eq!(t.retry_max, RETRY_MAX);

    let fast = Timing::new(Some("30"), None);
    assert_eq!(fast.every, secs(30));
    assert_eq!(fast.first, secs(20));
    assert_eq!(fast.retry_first, secs(30));
    assert_eq!(fast.retry_max, secs(30));

    let faster = Timing::new(Some("5"), Some("2"));
    assert_eq!((faster.first, faster.every), (secs(2), secs(5)));
    let capped = Timing::new(Some("5"), Some("60"));
    assert_eq!(capped.first, secs(5), "never later than the interval");

    // Junk and zero keep the defaults.
    assert_eq!(Timing::new(Some("0"), Some("x")), Timing::default());
}

#[test]
fn backoff_should_double_and_cap() {
    let t = Timing::default();
    assert_eq!(t.backoff(1), secs(60));
    assert_eq!(t.backoff(2), secs(120));
    assert_eq!(t.backoff(3), secs(240));
    assert_eq!(t.backoff(6), secs(1920));
    assert_eq!(t.backoff(7), secs(3600));
    assert_eq!(t.backoff(50), secs(3600));
    assert_eq!(t.backoff(u32::MAX), secs(3600));
    let fast = Timing::new(Some("10"), None);
    assert_eq!(fast.backoff(1), secs(10));
    assert_eq!(fast.backoff(5), secs(10));
}

#[test]
fn checks_should_start_after_the_delay_then_repeat() {
    let t = Timing::default();
    let mut u = Updater::new(t, secs(0), None);
    assert_eq!(u.phase(), &Phase::Idle);
    assert!(!u.due(secs(19)));
    assert_eq!(u.until_due(secs(5)), Some(secs(15)));
    assert!(u.due(secs(20)));

    u.begin();
    assert_eq!(u.phase(), &Phase::Checking);
    assert_eq!(
        u.until_due(secs(21)),
        None,
        "nothing planned while one runs"
    );
    assert!(!u.request(secs(21)), "one check at a time");

    u.up_to_date(secs(25));
    assert_eq!(u.phase(), &Phase::UpToDate);
    assert_eq!(u.next_check(), Some(secs(25) + CHECK_EVERY));
    assert_eq!(u.view().phase, "upToDate");
}

#[test]
fn check_now_should_be_due_at_once() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    assert!(u.request(secs(3)));
    assert!(u.due(secs(3)));
    assert!(u.view().can_check);
    u.begin();
    assert!(!u.view().can_check);
}

#[test]
fn failures_should_back_off_and_reset_on_success() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    u.begin();
    u.failed("offline", secs(20));
    assert_eq!(u.failures(), 1);
    assert_eq!(u.next_check(), Some(secs(80)));
    assert_eq!(
        u.phase(),
        &Phase::Failed {
            message: "offline".into()
        }
    );
    let view = u.view();
    assert_eq!((view.phase, view.message.as_str()), ("error", "offline"));
    assert!(view.can_check, "Try again works at once");

    u.begin();
    u.failed("offline", secs(80));
    assert_eq!(u.next_check(), Some(secs(200)));
    u.begin();
    u.failed("offline", secs(200));
    assert_eq!(u.next_check(), Some(secs(440)));

    u.begin();
    u.up_to_date(secs(440));
    assert_eq!(u.failures(), 0);
    assert_eq!(u.next_check(), Some(secs(440) + CHECK_EVERY));
    u.begin();
    u.failed("offline", secs(1000));
    assert_eq!(u.next_check(), Some(secs(1060)), "the backoff starts over");
}

#[test]
fn a_download_should_report_progress_then_be_ready() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    u.begin();
    u.downloading("0.2.23", 0);
    u.downloading("0.2.23", 42);
    let view = u.view();
    assert_eq!(
        (view.phase, view.percent, view.version.as_str()),
        ("downloading", 42, "0.2.23")
    );
    assert!(!view.prompt);
    u.downloading("0.2.23", 140);
    assert_eq!(u.view().percent, 100, "clamped");

    u.downloaded("0.2.23", "  ## Notes\n", secs(60));
    let view = u.view();
    assert_eq!((view.phase, view.version.as_str()), ("ready", "0.2.23"));
    assert_eq!(view.notes, "## Notes");
    assert!(view.prompt && view.can_restart);
    assert_eq!(u.next_check(), Some(secs(60) + CHECK_EVERY));
}

#[test]
fn a_ready_update_should_survive_later_checks() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    u.begin();
    u.downloaded("0.2.23", "", secs(1));
    // A later check keeps showing it, and a failure does not hide it.
    u.begin();
    assert_eq!(u.view().phase, "ready");
    u.failed("offline", secs(2));
    assert_eq!(u.view().phase, "ready");
    u.begin();
    u.up_to_date(secs(3));
    assert_eq!(u.view().phase, "ready");
    // Progress for the ready version itself changes nothing.
    u.downloading("0.2.23", 50);
    assert_eq!(u.view().phase, "ready");
    // A newer one downloads over it.
    u.begin();
    u.downloading("0.2.24", 10);
    assert_eq!(u.view().phase, "downloading");
    u.downloaded("0.2.24", "", secs(4));
    assert_eq!(u.view().version, "0.2.24");
}

#[test]
fn the_restart_prompt_should_wait_for_the_call_to_end() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    u.set_in_call(true);
    u.begin();
    u.downloaded("0.2.23", "", secs(1));
    let view = u.view();
    assert_eq!(view.phase, "ready");
    assert!(view.in_call);
    assert!(!view.prompt, "no prompt during a call");
    assert!(!view.can_restart, "and no restart");
    assert!(!u.can_restart());

    u.set_in_call(false);
    assert!(u.view().prompt);
    assert!(u.can_restart());

    // A quit during a call still installs (the quit ends the call).
    u.set_in_call(true);
    assert!(u.apply_on_quit(true));
    assert!(!u.apply_on_quit(false));
}

#[test]
fn nothing_should_install_on_quit_without_a_download() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    assert!(!u.apply_on_quit(true));
    u.begin();
    u.downloading("0.2.23", 50);
    assert!(!u.apply_on_quit(true));
}

#[test]
fn applying_should_stop_everything() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    u.begin();
    u.downloaded("0.2.23", "", secs(1));
    u.applying();
    assert_eq!(u.view().phase, "applying");
    assert_eq!(u.next_check(), None);
    assert!(!u.request(secs(2)));
    assert!(!u.can_restart());
    assert!(!u.apply_on_quit(true), "never twice");
    u.reset(secs(3));
    assert_eq!(u.view().phase, "applying");
}

#[test]
fn an_updater_that_is_off_should_never_check() {
    let mut u = Updater::new(Timing::default(), secs(0), Some(Off::NamedProfile));
    assert_eq!(u.next_check(), None);
    assert!(!u.due(secs(1_000_000)));
    assert!(!u.request(secs(1)));
    u.reset(secs(1));
    let view = u.view();
    assert_eq!(view.phase, "off");
    assert_eq!(view.message, Off::NamedProfile.reason());
    assert!(!view.can_check && !view.prompt);
}

#[test]
fn a_channel_change_should_drop_the_ready_update_and_check_again() {
    let mut u = Updater::new(Timing::default(), secs(0), None);
    u.begin();
    u.downloaded("0.2.24-beta.1", "", secs(1));
    u.reset(secs(5));
    assert_eq!(u.view().phase, "idle");
    assert!(u.ready().is_none());
    assert!(u.due(secs(5)));
}

#[test]
fn errors_should_shorten_to_one_line() {
    assert_eq!(
        short_error("\n  Network error: dns\nmore"),
        "Network error: dns"
    );
    assert_eq!(short_error(""), "Unknown error");
    let long = "x".repeat(400);
    assert_eq!(short_error(&long).chars().count(), 161);
}

#[test]
fn errors_should_read_as_plain_sentences() {
    // What Velopack 1.3.39 reports, as logged in the end-to-end run.
    assert_eq!(
        friendly_error("Network error: Http error: io: No connection could be made because the target machine actively refused it. (os error 10061)"),
        "Yap couldn't reach the update server."
    );
    assert_eq!(
        friendly_error("IO error: The system cannot find the file specified. (os error 2)"),
        "The update feed wasn't found."
    );
    assert_eq!(
        friendly_error("Network error: Http error: http status: 404"),
        "The update feed wasn't found."
    );
    assert_eq!(
        friendly_error("Checksum did not match for x (expected a, actual b)"),
        "The download was damaged."
    );
    assert_eq!(
        friendly_error("Size did not match for x"),
        "The download was damaged."
    );
    assert_eq!(
        friendly_error("dns error: no such host"),
        "Yap couldn't reach the update server."
    );
    assert_eq!(
        friendly_error("the update check panicked"),
        "Something went wrong."
    );
    for text in ["", "x", "Network error"] {
        assert!(friendly_error(text).ends_with('.'));
    }
}

#[test]
fn notes_should_read_as_plain_text() {
    let md = "## What's new in 0.2.23\n\n- Yap updates itself.\n- **Restart now** in `Settings`.\n  * nested\nPlain line.\n";
    let want = "What's new in 0.2.23\n\n• Yap updates itself.\n• Restart now in Settings.\n  • nested\nPlain line.";
    assert_eq!(plain_notes(md), want);
    assert_eq!(plain_notes("  \n "), "");
}
