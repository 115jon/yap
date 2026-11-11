# CI debug keystore (public, not for release)

`debug.keystore` signs the Qt prototype's CI APKs (`qt-android` in
`.github/workflows/desktop-native-mobile.yml`), so a tester can install one
run's APK over another's. It is deliberately public: store and key password
`android`, alias `androiddebugkey`, like the Android SDK's own debug key.

Anyone can sign an APK with it. Never use it for a build that leaves test
devices; a release key lives outside the repository.
