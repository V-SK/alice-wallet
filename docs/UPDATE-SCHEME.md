# Alice unsigned-distribution + ed25519 auto-update scheme

This document specifies how Alice desktop apps ship **without** Apple/Microsoft
code-signing certificates yet still auto-update **securely**. The wallet
implements it in [`gui/src/update.rs`](../gui/src/update.rs); the miner/worker
client (#28–32) reuses the same scheme with a Tauri-updater front-end (see
[Reuse for the miner client](#reuse-for-the-miner-client)).

The **only** trust anchor is an ed25519 release key we control. There are no
OS-vendor certificates anywhere in the chain. Every update is gated on a
detached ed25519 signature over a release manifest, verified against a public
key **embedded in the binary**. This is "fail closed": a build that embeds key A
cannot be tricked into trusting a manifest signed by any other key.

---

## 1. Keys

| | |
|---|---|
| Algorithm | ed25519 (raw, **not** pre-hashed — the signed message is the exact file bytes) |
| Private key | held **offline** at `~/.alice-release/alice-update-ed25519.key`; never in the repo, the binary, or CI |
| Public key | the 32 raw bytes, base64, embedded as `RELEASE_PUBKEY_B64` in `gui/src/update.rs` |
| Current pubkey | `8P+XmZZFEsUHLmqeB62Xqr5GnwW5K9vf2sQHvRzfi5k=` |

Generating the keypair (one-time, offline):

```sh
# Private key (PKCS#8 PEM) — keep OFFLINE, back up securely.
openssl genpkey -algorithm ed25519 -out ~/.alice-release/alice-update-ed25519.key

# Raw 32-byte public key -> base64 (this is RELEASE_PUBKEY_B64).
openssl pkey -in ~/.alice-release/alice-update-ed25519.key -pubout -outform DER \
  | tail -c 32 | base64
```

**Rotation is a breaking change by design.** A wallet build embeds exactly one
public key; to rotate, ship a new build embedding the new key *before* signing
any manifest with the new private key. Old builds will (correctly) refuse the
new key and must be upgraded out-of-band.

---

## 2. Manifest format (`latest.json`)

A single JSON object. The signature is computed over the **exact bytes** of this
file as published (see [§3](#3-signing)). Keep the serialization stable.

```json
{
  "schema": 1,
  "product": "alice-wallet",
  "version": "1.4.0",
  "min_supported": "1.0.0",
  "released": "2026-06-02T00:00:00Z",
  "notes": "Human-readable release notes shown in the in-app update prompt.",
  "artifacts": [
    {
      "platform": "macos-arm64",
      "url": "https://<release-host>/AliceWallet-macos-arm64.zip",
      "sha256": "<lowercase hex sha-256 of the artifact bytes>",
      "size": 12345678
    },
    { "platform": "linux-x86_64",  "url": "…", "sha256": "…", "size": 0 },
    { "platform": "windows-x86_64","url": "…", "sha256": "…", "size": 0 }
  ]
}
```

Field semantics (enforced in `update.rs`):

- **`schema`** — manifest schema version. A client refuses any manifest whose
  `schema` is **higher** than it understands (`SUPPORTED_SCHEMA`, currently `1`):
  fail closed rather than guess.
- **`product`** — must equal the client's `PRODUCT` string (`alice-wallet` for
  the wallet; the miner uses its own). A mismatched product is rejected even
  though the signature is valid — one signing key can serve multiple products
  without their manifests being interchangeable.
- **`version`** — the latest version (semver, no leading `v`). The client only
  offers it if it is **strictly newer** than the running version (no-downgrade).
- **`min_supported`** — the oldest version still allowed to run. If the running
  version is below it, the client **hard-blocks** with an upgrade CTA
  (`CheckOutcome::Unsupported`).
- **`released`** — RFC3339 timestamp, informational (shown in the prompt).
- **`notes`** — shown verbatim in the non-silent update prompt.
- **`artifacts[]`** — one entry per platform key
  (`macos-arm64`, `macos-x86_64`, `linux-x86_64`, `windows-x86_64`). The client
  selects the entry matching its own platform; if none exists it shows an
  "update available, download manually" notice instead of an in-app update.

The detached signature is published next to the manifest as **`latest.json.sig`**
— base64 of the 64-byte raw ed25519 signature.

---

## 3. Signing (offline)

Done on a trusted, offline machine that holds the private key. `scripts/release.sh`
prints these exact commands and only runs them with `--sign` (and never in CI):

```sh
KEY=~/.alice-release/alice-update-ed25519.key

# Sign the manifest BYTES (raw ed25519) -> base64 detached signature.
openssl pkeyutl -sign -inkey "$KEY" -rawin -in latest.json -out latest.json.sig.bin
base64 < latest.json.sig.bin | tr -d '\n' > latest.json.sig

# Recommended: also sign SHA256SUMS the same way.
openssl pkeyutl -sign -inkey "$KEY" -rawin -in SHA256SUMS -out SHA256SUMS.sig.bin
base64 < SHA256SUMS.sig.bin | tr -d '\n' > SHA256SUMS.sig
```

`-rawin` is the critical flag: it signs the message bytes directly with ed25519
(ed25519 hashes internally with SHA-512). The wallet verifies the same way —
`vk.verify(manifest_bytes, sig)` — so the signer and verifier agree byte-for-byte.

---

## 4. Verification + update flow (client side)

Implemented in `gui/src/update.rs`; the GUI wiring is in `gui/src/app.rs`
(dedicated updater thread) and `gui/src/ui/update_prompt.rs` (the prompt).

1. **Fetch** `latest.json` and `latest.json.sig` from the update URL
   (`DEFAULT_UPDATE_URL`, overridable with `ALICE_WALLET_UPDATE_URL`). Both
   reads are size-capped; a blocking rustls `ureq` client is used off the UI/
   subxt threads.
2. **Verify the signature first** — `verify_with_embedded_key(bytes, sig)` —
   **before parsing or trusting a single field**. A bad/absent signature aborts.
3. **Parse** the verified bytes, enforcing `schema <= SUPPORTED_SCHEMA` and
   `product == PRODUCT`.
4. **Decide** (`evaluate`):
   - running `< min_supported` → `Unsupported` (hard block + upgrade CTA);
   - `version` not strictly newer → `UpToDate`;
   - newer + artifact for this platform → `UpdateAvailable`;
   - newer + no artifact for this platform → `UpdateAvailableNoArtifact`.
5. **Prompt — never silent.** The wallet shows the new version + notes + an
   **Apply** button. (The miner client may apply silently; the wallet never does.)
6. On **Apply**:
   1. **Download** the artifact and verify **size + SHA-256** against the
      manifest entry **before** anything touches it (`download_and_verify`).
   2. **Stage** under a temp dir that is provably **not** the wallet data dir,
      extract with the OS-native tool (`ditto -x -k` / `tar`), and re-verify the
      staged bytes from disk (TOCTOU guard).
   3. **macOS:** ad-hoc codesign the new bundle **inner Mach-O first, then the
      bundle** (no `--deep`) — see [`scripts/adhoc_sign_macos.sh`](../gui/scripts/adhoc_sign_macos.sh).
   4. **Atomic swap** the new app over the live one, preserving the old version
      as **last-known-good** (`<app>.lkg`).
   5. **Arm a first-launch health gate** keyed to the new version, then relaunch.
7. **First-launch health gate** (`register_launch`): the freshly-installed build
   gets exactly one chance. If it reaches a healthy UI it commits (drops the
   backup). If it crashes on launch and is relaunched, the gate **rolls back** to
   last-known-good automatically.

### Key-preservation invariant

The updater writes **only** to the application location. `assert_not_in_data_dir`
checks every install/swap target against `config::wallet_data_root()` and refuses
if the target is the data dir or anything beneath it. The keystore
(`wallet.json`) and profile index (`profiles.json`) live in the data dir and are
**never** read, moved, or rewritten by an update. This is covered by tests
(`app_swap_leaves_keystore_files_untouched`, `apply_refuses_to_target_a_data_dir_app`).

If a future release ever needs to change an on-disk format, it must use
`migrate_keystore_in_place`, which **backs up the original bytes before writing**
the migrated ones (tested by `keystore_migration_backs_up_before_rewriting`).

---

## 5. Why ad-hoc signing (and not `--deep`)

We ship **unsigned** in the OS-vendor sense (no paid Developer ID / no
Authenticode cert). On Apple Silicon, the kernel still refuses to execute an
**unsigned** arm64 Mach-O, so we apply an **ad-hoc** signature (`codesign -s -`),
which carries no certificate but makes the binary loadable once the user clears
quarantine (see [INSTALL.md](./INSTALL.md)).

We sign **inner Mach-O first, then the bundle**, rather than `codesign --deep`,
because `--deep` is deprecated and does not reliably seal nested helper binaries
(e.g. the embedded node) before the outer bundle. The in-app updater
(`update::adhoc_codesign`) and `scripts/adhoc_sign_macos.sh` implement the same
ordering so an in-app update and a CI build produce equivalent results.

Linux and Windows have no equivalent load-time signature requirement; the
ed25519 manifest signature + SHA-256 is the integrity guarantee on all platforms.

---

## 6. Reuse for the miner client (#28–32, Tauri-updater)

The manifest format, the embedded-pubkey trust model, the no-downgrade /
`min_supported` rules, and the SHA-256 artifact gate are **product-agnostic**.
To reuse with Tauri's updater:

- Keep the **same `latest.json` shape** and the **same offline signing**
  (`openssl pkeyutl -sign -rawin`). Tauri's updater verifies a detached
  signature over the artifact/manifest with an embedded public key — the same
  primitive (ed25519, raw) — so one offline key + one signing step can serve
  both products.
- Use a **distinct `product`** string (e.g. `alice-miner`) so wallet and miner
  manifests are not interchangeable even though they may share a signing key.
- The miner client **may** silent-apply (it holds no custody). The wallet must
  not — that asymmetry lives in the UI layer, not the verification core.
- The verification/decision core in `update.rs`
  (`verify_with_embedded_key`, `parse_verified_manifest`, `is_newer`,
  `evaluate`, `verify_artifact_integrity`, `assert_not_in_data_dir`,
  `apply_update`, `register_launch`) is the reusable kernel; only the front-end
  (egui prompt vs. Tauri-updater events) differs.

---

## 7. Open items for V

- **Pin the release host.** `DEFAULT_UPDATE_URL` in `update.rs` and `--base-url`
  in `release.sh` currently point at a placeholder
  (`github.com/aliceprotocol/alice-wallet`). Set them to the real public
  releases repo/CDN before the first signed release.
- **First offline signing.** Generate (or reuse) the offline ed25519 key,
  confirm its base64 public key matches the embedded `RELEASE_PUBKEY_B64`, then
  run `scripts/release.sh --version <X> --sign --publish`.
