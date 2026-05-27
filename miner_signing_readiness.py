"""Default-off miner signing readiness bridge for Queue17.

This module is a local contract and verification adapter only. It binds the
miner Queue17 passport/session request shape to wallet-owned public-key
metadata without creating wallets, storing signing material, broadcasting
transactions, enabling rewards, or opening a payout executor.
"""

from __future__ import annotations

import base64
from dataclasses import asdict, dataclass
from datetime import datetime
import hashlib
import json
from typing import Any, Mapping

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey


POLICY_VERSION = "queue17-client-signing-passport-v1"
SESSION_REQUEST_SIGNATURE_DOMAIN = b"alice-mining-session-request-v1\n"
SESSION_SIGNATURE_SCHEME = "ed25519-session-request-sha256-v1"
ALGORITHM_ED25519 = "ed25519"
ALGORITHM_RVN_KAWPOW = "RVN_KAWPOW"
MODE_ALICE_REWARDED_MINING = "alice_rewarded_mining"
ENROLLMENT_SCOPE_INTERNAL_SHADOW_BETA = "internal_shadow_beta"

SESSION_REQUEST_FIELDS = (
    "passport_id",
    "device_id",
    "session_nonce",
    "requested_algorithm",
    "requested_pool_id",
    "timestamp",
    "policy_version",
)

SECRET_FIELD_MARKERS = (
    "private_key",
    "seed",
    "mnemonic",
    "wallet_seed",
    "signing_key",
    "secret",
)


class MinerSigningReadinessError(ValueError):
    pass


@dataclass(frozen=True)
class SessionRequestPayload:
    passport_id: str
    device_id: str
    session_nonce: str
    requested_algorithm: str
    requested_pool_id: str
    timestamp: str
    policy_version: str = POLICY_VERSION

    def validate(self) -> None:
        if not self.passport_id:
            raise MinerSigningReadinessError("missing_passport_id")
        if not self.device_id:
            raise MinerSigningReadinessError("missing_device_id")
        if not self.session_nonce:
            raise MinerSigningReadinessError("missing_session_nonce")
        if self.requested_algorithm != ALGORITHM_RVN_KAWPOW:
            raise MinerSigningReadinessError("unsupported_algorithm")
        if not self.requested_pool_id:
            raise MinerSigningReadinessError("missing_requested_pool_id")
        if not self.timestamp:
            raise MinerSigningReadinessError("missing_timestamp")
        if not self.policy_version:
            raise MinerSigningReadinessError("missing_policy_version")
        try:
            datetime.fromisoformat(self.timestamp.replace("Z", "+00:00"))
        except ValueError as exc:
            raise MinerSigningReadinessError("invalid_timestamp") from exc

    def to_dict(self) -> dict[str, str]:
        self.validate()
        return asdict(self)


@dataclass(frozen=True)
class PublicKeyBinding:
    passport_id: str
    device_id: str
    key_id: str
    algorithm: str
    public_key_pem: str
    credential_backend_ref: str
    policy_version: str = POLICY_VERSION
    enrollment_scope: str = ENROLLMENT_SCOPE_INTERNAL_SHADOW_BETA
    wallet_public_key_ref: str | None = None
    production_usable: bool = False

    def to_public_dict(self) -> dict[str, object]:
        self.validate()
        payload = asdict(self)
        return {key: value for key, value in payload.items() if value is not None}

    def validate(self) -> None:
        if not self.passport_id:
            raise MinerSigningReadinessError("missing_passport_id")
        if not self.device_id:
            raise MinerSigningReadinessError("missing_device_id")
        if not self.key_id:
            raise MinerSigningReadinessError("missing_key_id")
        if self.algorithm != ALGORITHM_ED25519:
            raise MinerSigningReadinessError("unsupported_key_algorithm")
        if self.enrollment_scope != ENROLLMENT_SCOPE_INTERNAL_SHADOW_BETA:
            raise MinerSigningReadinessError("unsupported_enrollment_scope")
        if not self.credential_backend_ref:
            raise MinerSigningReadinessError("missing_credential_backend_ref")
        if self.production_usable:
            raise MinerSigningReadinessError("production_signing_must_remain_disabled")
        _load_ed25519_public_key(self.public_key_pem)


@dataclass(frozen=True)
class SignatureEnvelope:
    key_id: str
    signature: str
    scheme: str = SESSION_SIGNATURE_SCHEME
    production_usable: bool = False

    def validate(self) -> None:
        if not self.key_id:
            raise MinerSigningReadinessError("missing_signature_key_id")
        if self.scheme != SESSION_SIGNATURE_SCHEME:
            raise MinerSigningReadinessError("unsupported_signature_scheme")
        if self.production_usable:
            raise MinerSigningReadinessError("production_signature_must_remain_disabled")
        _decode_signature(self.signature)


def _load_ed25519_public_key(public_key_pem: str) -> Ed25519PublicKey:
    try:
        loaded = serialization.load_pem_public_key(public_key_pem.encode("ascii"))
    except Exception as exc:
        raise MinerSigningReadinessError("invalid_public_key_pem") from exc
    if not isinstance(loaded, Ed25519PublicKey):
        raise MinerSigningReadinessError("unsupported_public_key_type")
    return loaded


def key_id_for_public_key_pem(public_key_pem: str) -> str:
    public_key = _load_ed25519_public_key(public_key_pem)
    raw = public_key.public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw,
    )
    return f"ed25519-{hashlib.sha256(raw).hexdigest()[:32]}"


def _decode_signature(signature: str) -> bytes:
    try:
        return base64.b64decode(signature.encode("ascii"), validate=True)
    except Exception as exc:
        raise MinerSigningReadinessError("invalid_signature_encoding") from exc


def _payload_data(payload: SessionRequestPayload | Mapping[str, Any]) -> dict[str, str]:
    data = payload.to_dict() if isinstance(payload, SessionRequestPayload) else dict(payload)
    missing = sorted(set(SESSION_REQUEST_FIELDS) - set(data))
    extra = sorted(set(data) - set(SESSION_REQUEST_FIELDS))
    if missing:
        raise MinerSigningReadinessError(f"missing_session_request_fields:{','.join(missing)}")
    if extra:
        raise MinerSigningReadinessError(f"unexpected_session_request_fields:{','.join(extra)}")
    normalized = {key: str(data[key]) for key in SESSION_REQUEST_FIELDS}
    SessionRequestPayload(**normalized).validate()
    return normalized


def canonical_session_payload_json(payload: SessionRequestPayload | Mapping[str, Any]) -> str:
    return json.dumps(
        _payload_data(payload),
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
    )


def canonical_session_payload_hash(payload: SessionRequestPayload | Mapping[str, Any]) -> str:
    return hashlib.sha256(canonical_session_payload_json(payload).encode("utf-8")).hexdigest()


def session_signature_message(payload_hash: str) -> bytes:
    if len(payload_hash) != 64:
        raise MinerSigningReadinessError("invalid_payload_hash")
    return SESSION_REQUEST_SIGNATURE_DOMAIN + payload_hash.encode("ascii")


def build_public_key_binding(
    *,
    passport_id: str,
    device_id: str,
    public_key_pem: str,
    credential_backend_ref: str,
    wallet_public_key_ref: str | None = None,
    policy_version: str = POLICY_VERSION,
) -> PublicKeyBinding:
    binding = PublicKeyBinding(
        passport_id=passport_id,
        device_id=device_id,
        key_id=key_id_for_public_key_pem(public_key_pem),
        algorithm=ALGORITHM_ED25519,
        public_key_pem=public_key_pem,
        credential_backend_ref=credential_backend_ref,
        policy_version=policy_version,
        wallet_public_key_ref=wallet_public_key_ref,
        production_usable=False,
    )
    binding.validate()
    return binding


def validate_public_bootstrap_metadata(metadata: Mapping[str, Any]) -> None:
    lowered_keys = {str(key).lower() for key in metadata}
    for marker in SECRET_FIELD_MARKERS:
        if marker in lowered_keys:
            raise MinerSigningReadinessError(f"secret_field_not_allowed:{marker}")


def verify_session_request_signature(
    *,
    payload: SessionRequestPayload | Mapping[str, Any],
    public_key: PublicKeyBinding,
    signature: SignatureEnvelope,
) -> bool:
    public_key.validate()
    signature.validate()
    if signature.key_id != public_key.key_id:
        return False
    payload_hash = canonical_session_payload_hash(payload)
    message = session_signature_message(payload_hash)
    loaded_key = _load_ed25519_public_key(public_key.public_key_pem)
    try:
        loaded_key.verify(_decode_signature(signature.signature), message)
    except InvalidSignature:
        return False
    return True


def closed_runtime_boundaries() -> dict[str, bool]:
    return {
        "direct_pool_mode_enabled": False,
        "live_reward_enabled": False,
        "payout_executor_enabled": False,
        "chain_transfer_enabled": False,
        "real_wallet_created": False,
        "signing_material_written_to_repo": False,
        "production_deploy_mutated": False,
    }


def production_secret_storage_boundary() -> dict[str, object]:
    return {
        "schema": "alice.wallet.miner_signing.secret_storage_boundary.v1",
        "allowed_reference_kinds": [
            "env-ref:ALICE_MINER_SIGNING_CREDENTIAL_REF",
            "os-keychain:Alice Mining Client/<passport_id>/<device_id>",
            "hardware-wallet:operator-approved-mining-identity",
        ],
        "repo_must_store_values": False,
        "logs_must_store_values": False,
        "docs_must_store_values": False,
        "public_bootstrap_contains": [
            "passport_id",
            "device_id",
            "key_id",
            "algorithm",
            "public_key_pem",
            "credential_backend_ref",
        ],
    }


def revocation_and_rotation_plan(*, active_key_id: str, next_key_id: str | None = None) -> dict[str, object]:
    if not active_key_id:
        raise MinerSigningReadinessError("missing_active_key_id")
    return {
        "schema": "alice.wallet.miner_signing.revocation_rotation.v1",
        "active_key_id": active_key_id,
        "next_key_id": next_key_id,
        "server_must_reject_revoked_key_ids": True,
        "operator_must_publish_public_replacement_before_cutover": True,
        "old_key_overlap_allowed_for_shadow_sessions_only": True,
        "old_key_must_not_issue_new_sessions_after_cutover": True,
        "client_must_fail_closed_on_revoked_or_unknown_key": True,
        "private_material_migration_in_repo_allowed": False,
    }


def readiness_contract_summary() -> dict[str, object]:
    return {
        "schema": "alice.wallet.miner_signing.readiness_bridge.v1",
        "state": "default_off_contract_only",
        "reusable_signing_sdk": "local verification/bootstrap contract only",
        "reusable_signing_cli": "not exposed; existing wallet CLI remains create/balance only",
        "session_request_fields": list(SESSION_REQUEST_FIELDS),
        "signature_domain": SESSION_REQUEST_SIGNATURE_DOMAIN.decode("ascii"),
        "signature_scheme": SESSION_SIGNATURE_SCHEME,
        "secret_storage_boundary": production_secret_storage_boundary(),
        "closed_runtime_boundaries": closed_runtime_boundaries(),
        "no_payout_executor": True,
        "no_live_reward": True,
        "no_chain_transfer": True,
    }
