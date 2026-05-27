import base64
from datetime import datetime, timezone
import unittest

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

import miner_signing_readiness as bridge


OBSERVED = datetime(2026, 5, 27, 12, 0, tzinfo=timezone.utc)


def public_key_pem_for_tests() -> tuple[Ed25519PrivateKey, str]:
    signer = Ed25519PrivateKey.generate()
    public_key_pem = signer.public_key().public_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    ).decode("ascii")
    return signer, public_key_pem


def payload(**overrides: str) -> bridge.SessionRequestPayload:
    data = {
        "passport_id": "passport-q17-worker-a",
        "device_id": "device-q17-worker-a",
        "session_nonce": "nonce-1",
        "requested_algorithm": bridge.ALGORITHM_RVN_KAWPOW,
        "requested_pool_id": "shadow-rvn-pool",
        "timestamp": OBSERVED.isoformat().replace("+00:00", "Z"),
        "policy_version": bridge.POLICY_VERSION,
    }
    data.update(overrides)
    return bridge.SessionRequestPayload(**data)


class MinerSigningReadinessBridgeTests(unittest.TestCase):
    def test_readiness_contract_is_default_off_and_does_not_expand_cli(self):
        summary = bridge.readiness_contract_summary()

        self.assertEqual(summary["state"], "default_off_contract_only")
        self.assertEqual(summary["reusable_signing_sdk"], "local verification/bootstrap contract only")
        self.assertIn("not exposed", summary["reusable_signing_cli"])
        self.assertEqual(summary["session_request_fields"], list(bridge.SESSION_REQUEST_FIELDS))
        self.assertTrue(summary["no_payout_executor"])
        self.assertTrue(summary["no_live_reward"])
        self.assertTrue(summary["no_chain_transfer"])
        for value in summary["closed_runtime_boundaries"].values():
            self.assertIs(value, False)

    def test_passport_bootstrap_binds_public_key_without_secret_fields(self):
        _, public_key_pem = public_key_pem_for_tests()
        binding = bridge.build_public_key_binding(
            passport_id="passport-q17-worker-a",
            device_id="device-q17-worker-a",
            public_key_pem=public_key_pem,
            credential_backend_ref="os-keychain:Alice Mining Client/passport-q17-worker-a/device-q17-worker-a",
            wallet_public_key_ref="wallet-public-key-ref:owner-approved-profile",
        )

        public_metadata = binding.to_public_dict()
        self.assertEqual(public_metadata["passport_id"], "passport-q17-worker-a")
        self.assertEqual(public_metadata["device_id"], "device-q17-worker-a")
        self.assertEqual(public_metadata["algorithm"], "ed25519")
        self.assertIn("BEGIN PUBLIC KEY", public_metadata["public_key_pem"])
        self.assertFalse(public_metadata["production_usable"])
        bridge.validate_public_bootstrap_metadata(public_metadata)

        with self.assertRaisesRegex(bridge.MinerSigningReadinessError, "secret_field_not_allowed"):
            bridge.validate_public_bootstrap_metadata({"private_key": "not allowed"})

    def test_session_request_payload_matches_queue17_contract(self):
        request = payload()
        shuffled = {
            "timestamp": request.timestamp,
            "requested_pool_id": request.requested_pool_id,
            "passport_id": request.passport_id,
            "policy_version": request.policy_version,
            "session_nonce": request.session_nonce,
            "device_id": request.device_id,
            "requested_algorithm": request.requested_algorithm,
        }

        self.assertEqual(tuple(request.to_dict()), bridge.SESSION_REQUEST_FIELDS)
        self.assertEqual(
            bridge.canonical_session_payload_hash(request),
            bridge.canonical_session_payload_hash(shuffled),
        )
        self.assertNotEqual(
            bridge.canonical_session_payload_hash(request),
            bridge.canonical_session_payload_hash(payload(session_nonce="nonce-2")),
        )

        unsafe = request.to_dict()
        unsafe["payout_address"] = "miner-override"
        with self.assertRaisesRegex(bridge.MinerSigningReadinessError, "unexpected_session_request_fields"):
            bridge.canonical_session_payload_hash(unsafe)

        with self.assertRaisesRegex(bridge.MinerSigningReadinessError, "unsupported_algorithm"):
            payload(requested_algorithm="DIRECT_POOL").validate()

    def test_session_signature_verification_contract_accepts_only_bound_public_key(self):
        signer, public_key_pem = public_key_pem_for_tests()
        binding = bridge.build_public_key_binding(
            passport_id="passport-q17-worker-a",
            device_id="device-q17-worker-a",
            public_key_pem=public_key_pem,
            credential_backend_ref="env-ref:ALICE_MINER_SIGNING_CREDENTIAL_REF",
        )
        request = payload()
        request_hash = bridge.canonical_session_payload_hash(request)
        signature = base64.b64encode(
            signer.sign(bridge.session_signature_message(request_hash))
        ).decode("ascii")

        envelope = bridge.SignatureEnvelope(
            key_id=binding.key_id,
            signature=signature,
        )
        self.assertTrue(
            bridge.verify_session_request_signature(
                payload=request,
                public_key=binding,
                signature=envelope,
            )
        )
        self.assertFalse(
            bridge.verify_session_request_signature(
                payload=payload(session_nonce="nonce-2"),
                public_key=binding,
                signature=envelope,
            )
        )

        mismatched = bridge.SignatureEnvelope(key_id="ed25519-mismatch", signature=signature)
        self.assertFalse(
            bridge.verify_session_request_signature(
                payload=request,
                public_key=binding,
                signature=mismatched,
            )
        )

    def test_storage_boundary_and_rotation_plan_are_reference_only(self):
        boundary = bridge.production_secret_storage_boundary()
        self.assertIn("env-ref:ALICE_MINER_SIGNING_CREDENTIAL_REF", boundary["allowed_reference_kinds"])
        self.assertIn("os-keychain:Alice Mining Client/<passport_id>/<device_id>", boundary["allowed_reference_kinds"])
        self.assertIn("hardware-wallet:operator-approved-mining-identity", boundary["allowed_reference_kinds"])
        self.assertFalse(boundary["repo_must_store_values"])
        self.assertFalse(boundary["logs_must_store_values"])
        self.assertFalse(boundary["docs_must_store_values"])

        plan = bridge.revocation_and_rotation_plan(
            active_key_id="ed25519-active",
            next_key_id="ed25519-next",
        )
        self.assertTrue(plan["server_must_reject_revoked_key_ids"])
        self.assertTrue(plan["operator_must_publish_public_replacement_before_cutover"])
        self.assertTrue(plan["client_must_fail_closed_on_revoked_or_unknown_key"])
        self.assertFalse(plan["private_material_migration_in_repo_allowed"])


if __name__ == "__main__":
    unittest.main()
