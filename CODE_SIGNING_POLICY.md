# Code Signing Policy

Yap's Windows release binaries are Authenticode-signed with
[Azure Artifact Signing](https://learn.microsoft.com/azure/artifact-signing/)
(formerly Trusted Signing). The Public Trust certificate is issued to the
maintainer as an individual developer after Microsoft identity validation, so the
signer is `Jonathan Martinez` (Chicago, Illinois, US). Certificates are
short-lived and renewed by the service. Every signature carries an RFC 3161
timestamp from `timestamp.acs.microsoft.com`, so it stays valid after the
certificate that made it expires.

## Signing roles

| Role      | Responsibility                                           | Member                              |
| --------- | -------------------------------------------------------- | ----------------------------------- |
| Committer | Maintains the source code and release configuration      | [115jon](https://github.com/115jon) |
| Reviewer  | Reviews changes from contributors who are not committers | [115jon](https://github.com/115jon) |
| Approver  | Approves each release by merging the release PR          | [115jon](https://github.com/115jon) |

The three roles are currently held by the same maintainer. Contributions from
other people still require maintainer review before merge. Automated tools may
assist review but do not hold a signing role.

## Signing scope

From 4.0.0 the Windows app is the native client (`desktop-native/`). Its
release workflow (`.github/workflows/desktop-native-release.yml`) signs:

| File                                                               | Origin                               |
| ------------------------------------------------------------------ | ------------------------------------ |
| `Yap.exe` (the app)                                                | this repository (`desktop-native/`)  |
| `Update.exe`, the root `Yap.exe` stub, and `Yap.App-win-Setup.exe` | Velopack (`vpk`), signed when packed |

`YapSetup.exe` on a 4.x release is a byte-for-byte copy of
`Yap.App-win-Setup.exe`. It never signs or re-signs the OBS Studio capture
files (`graphics-hook*.dll`, `inject-helper*.exe`, `get-graphics-offsets*.exe`)
or Microsoft's ONNX Runtime: they ship as their vendors released them, with
their vendors' signatures. `desktop-native/tools/package.ps1` fails the build
if the OBS files' hashes differ from
`desktop-native/packaging/obs-signed-hook.lock.json`.

The Tauri app's releases (3.x, built by `desktop-release.yml`, now on the
`archive/tauri-desktop` branch) signed its app, launcher stub, `YapSetup.exe`
and the project's custom CEF binaries the same way.

## Release controls

Each release follows this sequence:

1. The maintainer merges the release-please PR on `main`. Release-please tags the
   release and calls the native release workflow on a GitHub-hosted runner.
2. The workflow signs in to Azure with a GitHub OIDC token. The Entra app
   registration trusts only this repository's `main` branch and `v*` tags, and
   holds only the Artifact Signing Certificate Profile Signer role. No Azure
   secret is stored in GitHub. The build job, which runs third-party build
   scripts, gets neither the OIDC token nor a write token.
3. CI signs `Yap.exe`, then Velopack signs `Update.exe`, the stub and
   `Setup.exe` as it packs them.
4. `desktop-native/tools/verify-authenticode.ps1` runs `signtool verify /pa` on every
   signed file and checks the signer name and timestamp. Any failure stops the
   release.
5. Only then does CI upload the packages and publish the release.

## Privacy policy

Yap's privacy policy is available at [PRIVACY.md](PRIVACY.md) and will be
published at <https://yap.115jon.com/privacy>. The installer must present this
policy and required choices before a first installation that enables automatic
data transfers.

## Verification and incident response

Check a download with `signtool verify /pa Yap.App-win-Setup.exe`, or in the file's
Properties → Digital Signatures tab. The signer must be `Jonathan Martinez`.

Report suspected malicious artifacts, signing-policy violations, or certificate
concerns to <115jon@proton.me> and through the project's security-reporting
channel. The maintainer will investigate, preserve relevant release evidence,
and ask Microsoft to revoke the affected certificate if a signature was misused.
