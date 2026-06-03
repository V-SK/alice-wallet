# Installing Alice Wallet

Alice Wallet ships **without** paid OS-vendor code-signing certificates (no Apple
Developer ID, no Windows Authenticode). It is instead protected by an **ed25519
signature** over each release, verified by the app itself on update (see
[UPDATE-SCHEME.md](./UPDATE-SCHEME.md)). Because there is no vendor certificate,
the OS will show an "unidentified developer / unknown publisher" warning the
first time you run it. The steps below clear that warning **and** show you how to
verify the download yourself with the published checksums + signature.

> Verify before you run. The SHA-256 checksum proves the file wasn't corrupted
> or tampered with in transit; the ed25519 signature proves it came from the
> Alice release key. Do the [verification](#verifying-your-download) step first.

---

## macOS (Apple Silicon and Intel)

The download is `AliceWallet-macos-arm64.zip` (Apple Silicon) or
`AliceWallet-macos-x86_64.zip` (Intel). It contains `AliceWallet.app`.

**Recommended: install from Terminal** (this avoids the Gatekeeper quarantine
flag entirely, so the app opens normally):

```sh
cd ~/Downloads
# Unzip preserving bundle metadata.
ditto -x -k AliceWallet-macos-arm64.zip .
# Move it into Applications.
mv AliceWallet.app /Applications/
# Remove the quarantine attribute (this is what triggers the Gatekeeper block).
xattr -dr com.apple.quarantine /Applications/AliceWallet.app
open /Applications/AliceWallet.app
```

**If you double-click instead** and see *"AliceWallet.app cannot be opened
because it is from an unidentified developer"*:

- Right-click (or Control-click) the app → **Open** → **Open** in the dialog, or
- **System Settings → Privacy & Security**, scroll to the message about
  AliceWallet, and click **Open Anyway**.

To clear quarantine manually at any time:

```sh
xattr -dr com.apple.quarantine /Applications/AliceWallet.app
```

The app carries an **ad-hoc** code signature (no Apple certificate). That is
expected and is what lets it run on Apple Silicon after quarantine is cleared.
You can inspect it:

```sh
codesign -dv /Applications/AliceWallet.app      # shows "Signature=adhoc"
```

---

## Windows (10 / 11, x86_64)

The download is `AliceWallet-windows-x86_64.zip` containing `AliceWallet.exe`.

1. **Unblock the zip before extracting** (clears the "downloaded from the
   internet" mark on the contents): right-click the `.zip` → **Properties** →
   check **Unblock** at the bottom → **OK**. Then extract.
   Or in PowerShell:

   ```powershell
   Unblock-File .\AliceWallet-windows-x86_64.zip
   Expand-Archive .\AliceWallet-windows-x86_64.zip -DestinationPath .
   ```

2. **Run.** Windows SmartScreen may show *"Windows protected your PC"*. Click
   **More info** → **Run anyway**.

3. If a file was extracted while still blocked, you can unblock the exe directly:

   ```powershell
   Unblock-File .\AliceWallet\AliceWallet.exe
   ```

There is no Authenticode certificate; the SmartScreen prompt is expected. Verify
the SHA-256 (below) to confirm the download is authentic.

---

## Linux (x86_64)

The download is `AliceWallet-linux-x86_64.tar.gz`.

```sh
tar -xzf AliceWallet-linux-x86_64.tar.gz
cd AliceWallet
chmod +x AliceWallet
./AliceWallet
```

If your desktop environment does not launch it from a double-click, run it from
a terminal as above. (A `.desktop` entry may also be included.)

GUI runtime libraries you may need on a minimal system: GTK3, libxkbcommon, and
the usual X11/Wayland client libs (e.g. on Debian/Ubuntu:
`sudo apt-get install libgtk-3-0 libxkbcommon0`).

---

## Verifying your download

Two published files accompany every release:

- **`SHA256SUMS`** — the SHA-256 of every artifact.
- **`SHA256SUMS.sig`** / **`latest.json.sig`** — detached ed25519 signatures.

### Step 1 — checksum (all platforms)

macOS / Linux:

```sh
# Compare against the line for your file in SHA256SUMS.
shasum -a 256 AliceWallet-macos-arm64.zip        # macOS
sha256sum   AliceWallet-linux-x86_64.tar.gz      # Linux

# Or verify everything listed at once (run in the folder with SHA256SUMS):
sha256sum -c SHA256SUMS        # Linux
shasum -a 256 -c SHA256SUMS    # macOS
```

Windows (PowerShell):

```powershell
Get-FileHash .\AliceWallet-windows-x86_64.zip -Algorithm SHA256
# Compare the printed hash to the matching line in SHA256SUMS.
```

If the hash does not match the value in `SHA256SUMS`, **stop** — the file is
corrupt or tampered with. Do not run it.

### Step 2 — signature (proves it's from the Alice release key)

The app verifies `latest.json.sig` automatically on every update, so manual
signature checking is optional. To verify by hand, you need the Alice release
**public key** (the same 32 bytes embedded in the app as `RELEASE_PUBKEY_B64`):

```
8P+XmZZFEsUHLmqeB62Xqr5GnwW5K9vf2sQHvRzfi5k=
```

Wrap it into a PEM public key, then verify the **raw** ed25519 signature over the
file bytes (this matches how the release was signed,
`openssl pkeyutl -sign -rawin`):

```sh
# 1. Build a PEM SubjectPublicKeyInfo from the raw 32-byte ed25519 public key.
#    (The 12-byte prefix below is the fixed ed25519 SPKI header.)
{ printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'; \
  printf '%s' '8P+XmZZFEsUHLmqeB62Xqr5GnwW5K9vf2sQHvRzfi5k=' | base64 -d; } \
  | openssl pkey -pubin -inform DER -out alice-update.pub.pem

# 2. Decode the detached base64 signature to raw bytes.
base64 -d < SHA256SUMS.sig > SHA256SUMS.sig.bin   # GNU base64
#   macOS: use `base64 -D < SHA256SUMS.sig > SHA256SUMS.sig.bin`

# 3. Verify (raw ed25519 over the file bytes). Prints "Signature Verified Successfully".
openssl pkeyutl -verify -pubin -inkey alice-update.pub.pem -rawin \
    -in SHA256SUMS -sigfile SHA256SUMS.sig.bin
```

The same procedure verifies `latest.json` against `latest.json.sig`. A failed
verification means the file is not from the Alice release key — do not trust it.

---

## What the app does for you afterward

Once installed, Alice Wallet checks for updates on launch and periodically. When
a new version is available it shows a prompt with the version and release notes —
it **never** updates silently. On **Apply** it downloads the new build, verifies
its signature and checksum, installs it, and relaunches; if the new build fails
to start, it automatically restores the previous working version. Your wallet
keys and data are stored separately and are never touched by an update.
