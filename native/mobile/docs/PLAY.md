# Google Play: the owner's steps

The build side is done: every release attaches `yap-qt-<x.y.z>.aab`, an
Android App Bundle with both ABIs (arm64-v8a, x86_64) under one version code,
signed with the release key, checked with bundletool and for 16 KB pages
(README, "App Bundle" and "Releases"). Nothing here has been done in a Play
Console yet. These steps need the owner's Google account, the release key
and decisions only the owner can make.

## 1. Developer account

- Create (or use) a Google Play developer account and verify its identity.
- A personal account made after November 2023 has to run a closed test with
  at least 12 testers for 14 days before it can apply for production. An
  organisation account does not, but it needs a D-U-N-S number.

## 2. App and Play App Signing

- Create the app: default language, name "Yap", app (not game), free.
  The package name is `dev.yap.app`, and it can never change.
- Play App Signing is required for new apps. Choose how to set it up when
  you upload the first AAB:
  - **Recommended: use the existing release key as the app signing key**
    ("Use existing app signing key from Java keystore", with Google's PEPK
    tool, run on your own machine against the release keystore). Play then
    signs installs with the certificate the GitHub APKs already carry
    (SHA-256 `75:E0:D8:E0:...:6E:FF:B8:9C`, in `qt-android-release.yml`),
    so a phone can move between the GitHub APK and the Play install without
    reinstalling, and existing testers keep their data.
  - Or let Google generate the app signing key. That is simpler, but Play
    installs and GitHub APKs then have different certificates: neither
    updates the other, and a tester has to uninstall to switch.
- The upload key. CI signs the AAB with the release key
  (`android-release` environment). If that key also becomes the app signing
  key, Play can take it as the upload key too, but Google recommends a
  separate upload key: Google can reset a lost or leaked upload key, not a
  signing key. To use one:
  1. generate it (`keytool -genkeypair -keyalg RSA -keysize 4096
-validity 10000 -alias upload ...`) and keep a backup offline;
  2. register its certificate in the Console (Setup > App signing);
  3. add it to the `android-release` environment as new secrets and make
     the sign job sign the AAB with it and check its certificate. The APKs
     stay signed with the release key.
- Download the app signing certificate's SHA-256 from Setup > App signing.
  If it is not the release certificate, add it to the web app's
  `assetlinks.json` (App Links for `dev.yap.app`) beside the current one.

## 3. First upload

- Testing > Internal testing: create a release and upload the newest
  release's `yap-qt-<x.y.z>.aab` from GitHub. Version codes are
  `30000000 + minutes since 2026-01-01 UTC` and rise with every build, so
  any later AAB is accepted as an update.
- Review the pre-launch report and Android vitals once it is processed.
- The AAB carries the symbol tables of the app's library and libc++
  (`BUNDLE-METADATA/.../debugsymbols`), so Play can symbolize native
  crashes in Android vitals.

## 4. Store listing and policy forms

- Main store listing: short and full description, the 512 x 512 icon, a
  1024 x 500 feature graphic, and phone screenshots (crop the status bar,
  and show only test accounts).
- Privacy policy: a stable public URL. The in-app link points at
  `PRIVACY.md` on GitHub; a page on the web app is better.
- Data safety: account id, username, messages, photos and files, audio
  during calls (not stored), the FCM token, the device name (sessions), and
  no crash data yet. No ads, no sale of data, encrypted in transit, and
  deletion in the app and on the web.
- Account deletion: the web URL where an account can be deleted (the web
  app's Settings).
- App access: reviewers cannot sign in without help (device-code sign-in).
  Give a review account and instructions.
- Content rating questionnaire, target audience, the ads declaration (no
  ads), and the news and government app questions (no).
- Sensitive permissions: `USE_FULL_SCREEN_INTENT` (the calling-app
  declaration) and `FOREGROUND_SERVICE_MICROPHONE` (a foreground service
  declaration with a short video of a call). Add `FOREGROUND_SERVICE_CAMERA`
  when video calls ship.

## 5. Closed test and production

- Closed testing: add testers by email list or Google group (12 or more for
  14 days on a new personal account), then apply for production access.
- Production: a staged rollout (for example 20%) for the first release.
