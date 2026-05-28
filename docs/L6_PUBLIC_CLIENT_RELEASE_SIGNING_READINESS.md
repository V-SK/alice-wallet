# L6 Public Client Release Signing Readiness

Date: 2026-05-27

Scope: `alice-wallet` public Mac/Windows client release-signing readiness only.
This is a descriptor and validator packet. It does not sign, notarize, staple,
upload to HF, publish a release, load credentials, or touch other Alice repos.

## Audited Inputs

- `/Users/ssv/Documents/AGENTS.md`
- `/Users/ssv/Documents/ALICE_OPERATOR_BOOTSTRAP.md`
- `/Users/ssv/Documents/codex_machine_aliases.md`
- `README.md`
- `.github/workflows/release.yml`
- `gui/Cargo.toml`
- `release_ops.py`
- `phase50c_release_readiness.py`
- `tests/test_phase50c_release_readiness.py`

No exported public `.app`, `.dmg`, `.pkg`, `.exe`, or installer artifact was
inspected in this pass. The readiness answer is therefore based on settings,
metadata shape, and descriptor-only validators.

## Distribution Goal

The target is a future public Alice Wallet desktop client release:

- macOS: Developer ID Application signed app, Developer ID Installer signed
  package when a `.pkg` is shipped, hardened runtime, reviewed entitlements,
  notarization, stapling, and Gatekeeper validation.
- Windows: Authenticode signed executable and installer with RFC3161 timestamp,
  local signature verification, and SmartScreen residual-risk review.
- Manifest: signed release manifest with SHA-256, byte size, version, channel,
  source commit, and HF-only distribution metadata for every shipped artifact.

Public release remains blocked. No public release may happen until packages are
signed, notarized where applicable, hashes are frozen, and HF metadata is
approved.

## Current Settings Inspection

`.github/workflows/release.yml` can build Linux, Windows, and macOS artifacts.
The macOS path currently creates an unsigned release-style app bundle and uses
ad-hoc signing only. That does not satisfy public Developer ID distribution.

The workflow also has a tag-triggered GitHub Release job. L6 public client
distribution must not use that as the public release gate. The approved public
distribution metadata in this readiness packet is HF-only, with `/mnt/storage`
remaining cold archive only.

## macOS Prerequisites

Required before public release:

- Developer ID Application identity for `AliceWallet.app`.
- Developer ID Installer identity if a `.pkg` is shipped.
- Hardened runtime via `codesign --options runtime`.
- Reviewed entitlements plist with no `get-task-allow`.
- Notarization accepted by Apple for the shipped `.dmg` or `.pkg`.
- Stapled ticket validation.
- Gatekeeper assessment on a clean machine.

Print-only signing plan:

```text
codesign --force --options runtime --timestamp --entitlements release/macos/AliceWallet.entitlements --sign "Developer ID Application: [TEAM_NAME] ([TEAM_ID])" AliceWallet.app
productsign --sign "Developer ID Installer: [TEAM_NAME] ([TEAM_ID])" AliceWallet.pkg AliceWallet-signed.pkg
xcrun notarytool submit AliceWallet.dmg --keychain-profile [NOTARY_PROFILE_REF] --wait
xcrun stapler staple AliceWallet.dmg
```

Validation commands after a real signed artifact exists:

```text
codesign --verify --strict --deep --verbose=4 AliceWallet.app
codesign -dvvv --entitlements :- AliceWallet.app
spctl --assess --type execute --verbose=4 AliceWallet.app
xcrun stapler validate AliceWallet.dmg
pkgutil --check-signature AliceWallet.pkg
spctl --assess --type install --verbose=4 AliceWallet.pkg
```

## Windows Prerequisites

Required before public release:

- Authenticode code-signing certificate reference owned by the release operator.
- RFC3161 timestamp service reference.
- Signed executable and signed installer.
- Local verification of both signatures and timestamps.
- SmartScreen residual-risk review. SmartScreen reputation may still warn even
  after a valid signature and timestamp; this is not a substitute for signature
  validation.

Print-only signing plan:

```text
signtool sign /fd SHA256 /tr [RFC3161_TIMESTAMP_URL_REF] /td SHA256 /sha1 [CERT_THUMBPRINT_REF] AliceWallet.exe
signtool sign /fd SHA256 /tr [RFC3161_TIMESTAMP_URL_REF] /td SHA256 /sha1 [CERT_THUMBPRINT_REF] AliceWalletSetup.exe
```

Validation commands after real signed artifacts exist:

```text
signtool verify /pa /tw /v AliceWallet.exe
signtool verify /pa /tw /v AliceWalletSetup.exe
powershell -NoProfile -Command "Get-AuthenticodeSignature .\AliceWallet.exe | Format-List"
powershell -NoProfile -Command "Get-AuthenticodeSignature .\AliceWalletSetup.exe | Format-List"
```

## Manifest Gate

`l6_public_release_signing_readiness.py` validates metadata only. A release-ready
manifest must include:

- schema `alice.wallet.l6.signed_release_manifest.v1`;
- app name, source commit, version, and channel;
- `public_distribution_backend=HF`;
- `oss_or_aliyun_allowed=false`;
- one package entry for each shipped Mac/Windows artifact;
- SHA-256, byte size, version, channel, and HF path per package;
- `hash_frozen=true` per package and `hashes_frozen=true` at manifest level;
- `hf_metadata_approved=true`;
- `signing_evidence` per package using schema
  `alice.wallet.l6.package_signing_evidence.v1`;
- manifest signature metadata.

Per-package signing evidence must be redacted metadata only. It records an
operator evidence reference plus SHA-256 hashes of validation logs, not raw
certificates, private keys, Apple credentials, Microsoft credentials, command
stdout, or local machine paths.

Required evidence checks:

- macOS `.dmg`: strict codesign verify, entitlement dump review, hardened
  runtime review, accepted notarization result, stapler validation, and
  Gatekeeper execute assessment.
- macOS `.pkg`: app strict codesign verify, entitlement dump review, hardened
  runtime review, package signature verification, install assessment, accepted
  notarization result, stapler validation, and installed-app Gatekeeper
  assessment.
- Windows executable and installer: `signtool verify /pa /tw`, PowerShell
  Authenticode status `Valid`, timestamp verification, and publisher identity
  review.

The validator intentionally does not load private signing material. Missing Mac
Developer ID refs, Mac notary profile refs, Windows Authenticode refs, Windows
timestamp refs, or manifest-signing key refs fail closed.

## Ready for Owner Signing Checklist

`l6_owner_signing_checklist.json` is generated for owner review. It remains
`owner_signing_environment_ready=false` and `public_release_ready=false` until a
real owner signing environment supplies:

- Mac Developer ID Application, Developer ID Installer, and notary profile refs.
- Windows Authenticode certificate and timestamp service refs.
- Release manifest key/signature refs.
- Per-package verification evidence bundle refs and redacted validation log
  SHA-256 values.
- HF repo/path approval for every shipped artifact.

This checklist is a handoff contract for the owner signing environment. It is
not authority to sign, notarize, upload, publish, or open downloads.

Example metadata-only validation:

```text
python3 l6_public_release_signing_readiness.py --validate-manifest tests/fixtures/l6_signed_manifest_metadata.valid.json --require-release-ready
python3 l6_public_release_signing_readiness.py --validate-manifest tests/fixtures/l6_signed_manifest_metadata.valid.json --credential-status tests/fixtures/l6_missing_credentials.fail_closed.json --require-release-ready
```

The second command is expected to exit non-zero because credentials are missing.

## Closed Boundaries

- No real Apple credentials.
- No real Microsoft credentials.
- No certs or private keys.
- No keychain, credential-store, or HSM access.
- No signing command execution.
- No notarization submission.
- No stapling execution.
- No HF upload.
- No GitHub Release publication.
- No website/Vercel mutation.
- No public release.
