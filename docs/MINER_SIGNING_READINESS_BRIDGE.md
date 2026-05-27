# Miner Signing Readiness Bridge

Date: 2026-05-27

Scope: WALLET0 wallet/signing production-readiness bridge for Queue17 miner
passport/session signing alignment. This is a local, default-off contract only.
It does not create a real wallet, sign production artifacts, enable production
Alice rewarding, open payout execution, broadcast transfers, mutate deployment,
or touch `alice_live`.

## Audited Inputs

- `/Users/ssv/Documents/AGENTS.md`
- `/Users/ssv/Documents/ALICE_OPERATOR_BOOTSTRAP.md`
- `/Users/ssv/Documents/codex_machine_aliases.md`
- `/Users/ssv/Documents/alice-acp/docs/alice-launch-readiness-cross-audit.md`
- `/Users/ssv/Documents/alice-miner-test/docs/queue17-client-signing-passport.md`
- `/Users/ssv/Documents/alice-miner-test/miner/mining_internal/session_signing.py`
- `/Users/ssv/Documents/alice-miner-test/miner/mining_internal/credential_store.py`
- `/Users/ssv/Documents/alice-wallet/cli.py`
- `/Users/ssv/Documents/alice-wallet/phase50c_release_readiness.py`
- `/Users/ssv/Documents/alice-wallet/gui/src/wallet_profiles.rs`

## Current Wallet Signing Answer

`alice-wallet` did not expose a production-ready miner passport/session signing
SDK or CLI before this bridge. Existing wallet CLI commands remain limited to
local wallet creation and read-only balance lookup. Existing release-readiness
helpers are descriptor-only and explicitly avoid actual signing, upload,
notarization, updater execution, public release, production mutation, real
wallet data reads, and transfer broadcast.

This change adds `miner_signing_readiness.py` as a reusable local contract and
verification adapter. It is not a production signer. It gives wallet/signing and
miner Queue17 a shared shape for:

- public passport-to-public-key binding;
- strict Queue17 session request canonicalization;
- Ed25519 signature verification against exported public key metadata;
- default-off storage and runtime boundaries;
- revocation and key-rotation policy metadata.

No new CLI command was added because the current CLI test contract intentionally
keeps the wallet CLI surface narrow.

## Passport Bootstrap Public-Key Binding

The bootstrap record is public metadata only:

- `passport_id`
- `device_id`
- `key_id`
- `algorithm`
- `public_key_pem`
- `credential_backend_ref`
- optional `wallet_public_key_ref`
- `policy_version`
- `enrollment_scope`
- `production_usable=false`

The miner passport binds to the signing public key by `passport_id`, `device_id`,
and deterministic `key_id` derived from the Ed25519 public key bytes. The repo
must not store private signing material, wallet seed material, mnemonic words,
or any credential value. `credential_backend_ref` is a reference path only, not
a secret value.

## Session Request Signature Verification Contract

The canonical Queue17 session request has exactly these fields:

```text
passport_id
device_id
session_nonce
requested_algorithm
requested_pool_id
timestamp
policy_version
```

The only accepted requested algorithm is `RVN_KAWPOW`.

The signed message is domain-separated:

```text
alice-mining-session-request-v1
<canonical_session_payload_hash>
```

Verification must:

- recompute the canonical payload hash;
- reject missing or unexpected fields;
- reject non-`RVN_KAWPOW` algorithms;
- require signature `key_id` to match the exported public key binding;
- verify Ed25519 over the domain-separated message;
- fail closed on malformed public keys, malformed signatures, unknown keys,
  revoked keys, stale timestamps, or replayed nonces at the server/session edge.

Replay enforcement remains server/session-edge responsibility; this wallet repo
bridge verifies the cryptographic contract and metadata boundary only.

## Production Secret Storage Boundary

Production signing material may only be referenced by approved storage paths:

- `env-ref:ALICE_MINER_SIGNING_CREDENTIAL_REF`
- `os-keychain:Alice Mining Client/<passport_id>/<device_id>`
- `hardware-wallet:operator-approved-mining-identity`

The repo, docs, logs, release packets, test fixtures, and handoffs must store
only references and public exports. They must not store credential values.

## Revocation And Key Rotation

Before public shadow beta or production use, the server/session edge must own a
revocation list keyed by `key_id` and passport/device binding. Rotation plan:

1. Operator enrolls a replacement public key export for the same passport/device.
2. Server publishes or loads the replacement public key before cutover.
3. Old and new keys may overlap only for shadow sessions during a bounded window.
4. After cutover, old key IDs must not issue new sessions.
5. Clients and server fail closed on revoked, unknown, mismatched, malformed, or
   expired keys.
6. No private material migration is performed inside this repo.

## Closed Runtime Boundaries

This bridge keeps these boundaries closed:

- Direct Pool Mode: closed.
- Live reward: closed.
- Payout executor: closed.
- Chain transfer: closed.
- Real wallet generation: closed.
- Production deployment mutation: closed.
- Writing signing material into repo/docs/logs: closed.

The bridge does not change production deploy, wallet release readiness, miner
packaging, `alice_live`, ACP source, pool endpoints, or reward accounting.
