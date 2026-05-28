import json
import tempfile
import unittest
from pathlib import Path

import l6_public_release_signing_readiness as l6


FIXTURES = Path(__file__).parent / "fixtures"
SOURCE_COMMIT = "c6139639abed0000000000000000000000000000"


def load_fixture(name: str) -> dict:
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


class L6PublicReleaseSigningReadinessTests(unittest.TestCase):
    def test_macos_readiness_covers_developer_id_notarization_and_gatekeeper(self):
        packet = l6.build_macos_signing_readiness(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0",
            channel="stable",
        )

        self.assertTrue(packet["developer_id_application"]["required"])
        self.assertTrue(packet["developer_id_installer"]["required_for_pkg"])
        self.assertTrue(packet["hardened_runtime"]["required"])
        self.assertTrue(packet["entitlements"]["required"])
        self.assertTrue(packet["notarization"]["required_for_public_distribution"])
        self.assertTrue(packet["stapling"]["required_after_accepted_notarization"])
        self.assertIn("spctl --assess --type execute", "\n".join(packet["validation_commands"]))
        for value in packet["closed_boundaries"].values():
            self.assertIs(value, False)

    def test_windows_readiness_covers_authenticode_timestamp_and_smartscreen(self):
        packet = l6.build_windows_signing_readiness(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0",
            channel="stable",
        )

        self.assertTrue(packet["authenticode"]["required"])
        self.assertTrue(packet["timestamping"]["required"])
        self.assertTrue(packet["verification"]["installer_and_exe_required"])
        commands = "\n".join(packet["verification"]["validation_commands"])
        self.assertIn("signtool verify /pa /tw /v AliceWallet.exe", commands)
        self.assertIn("signtool verify /pa /tw /v AliceWalletSetup.exe", commands)
        self.assertIn("SmartScreen reputation", packet["smart_screen"]["residual_risk"])
        for value in packet["closed_boundaries"].values():
            self.assertIs(value, False)

    def test_signed_manifest_fixture_validates_release_metadata_shape(self):
        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")

        l6.validate_signed_manifest_metadata(manifest, require_release_ready=True)
        self.assertEqual(manifest["public_distribution_backend"], "HF")
        self.assertTrue(manifest["hashes_frozen"])
        self.assertTrue(manifest["hf_metadata_approved"])
        self.assertEqual({item["channel"] for item in manifest["packages"]}, {"stable"})
        self.assertEqual(
            {(item["platform"], item["artifact_kind"]) for item in manifest["packages"]},
            {
                ("macos-arm64", "dmg"),
                ("macos-arm64", "pkg"),
                ("windows-x64", "exe"),
                ("windows-x64", "installer"),
            },
        )
        for item in manifest["packages"]:
            self.assertEqual(
                item["signing_evidence"]["schema"],
                "alice.wallet.l6.package_signing_evidence.v1",
            )
            self.assertTrue(item["signing_evidence"]["operator_evidence_ref"])
            self.assertTrue(
                all(check["passed"] for check in item["signing_evidence"]["verification_checks"])
            )

    def test_missing_credentials_fail_closed_even_with_valid_manifest_metadata(self):
        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")
        credentials = load_fixture("l6_missing_credentials.fail_closed.json")

        result = l6.evaluate_public_release_gate(
            manifest=manifest,
            credential_status=credentials,
        )

        self.assertFalse(result["metadata_ready"])
        self.assertFalse(result["release_execution_allowed"])
        self.assertIn("missing_macos_developer_id_application_ref", result["blockers"])
        self.assertIn("missing_macos_developer_id_installer_ref", result["blockers"])
        self.assertIn("missing_macos_notary_profile_ref", result["blockers"])
        self.assertIn("missing_windows_authenticode_certificate_ref", result["blockers"])
        self.assertIn("missing_windows_timestamp_service_ref", result["blockers"])
        self.assertIn("missing_release_manifest_key_ref", result["blockers"])

    def test_manifest_validation_rejects_non_hf_and_unfrozen_release_hashes(self):
        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")
        manifest["packages"][0]["hf_backend"] = "OSS"
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_signed_manifest_metadata(manifest, require_release_ready=True)

        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")
        manifest["packages"][0]["sha256"] = "not-a-sha"
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_signed_manifest_metadata(manifest, require_release_ready=True)

        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")
        manifest["packages"][0]["hash_frozen"] = False
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_signed_manifest_metadata(manifest, require_release_ready=True)

    def test_release_ready_manifest_requires_per_package_signing_evidence(self):
        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")
        manifest["packages"][0]["signing_evidence"]["verification_checks"][0]["passed"] = False
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_signed_manifest_metadata(manifest, require_release_ready=True)

        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")
        manifest["packages"][2]["signing_evidence"]["operator_evidence_ref"] = l6.PLACEHOLDER
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_signed_manifest_metadata(manifest, require_release_ready=True)

        manifest = load_fixture("l6_signed_manifest_metadata.valid.json")
        manifest["packages"][3]["signing_evidence"]["verification_checks"][0]["evidence_log_sha256"] = "not-a-sha"
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_signed_manifest_metadata(manifest, require_release_ready=True)

    def test_validator_rejects_secret_fields_and_open_execution_flags(self):
        packet = l6.build_distribution_goal(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0",
            channel="stable",
        )

        unsafe = json.loads(json.dumps(packet))
        unsafe["apple_password"] = "redacted-placeholder"
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_l6_packet(unsafe)

        unsafe = json.loads(json.dumps(packet))
        unsafe["closed_boundaries"]["public_release_opened"] = True
        with self.assertRaises(l6.L6ReleaseSigningReadinessError):
            l6.validate_l6_packet(unsafe)

    def test_artifact_writer_outputs_l6_packet_and_fail_closed_gate(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_dir = Path(tmp)
            artifacts = l6.write_l6_artifacts(
                out_dir=out_dir,
                source_commit=SOURCE_COMMIT,
                app_version="0.1.0",
                channel="stable",
            )

            expected = {
                "l6_distribution_goal.json",
                "l6_macos_signing_readiness.json",
                "l6_windows_signing_readiness.json",
                "l6_signed_manifest_template.json",
                "l6_missing_credentials_fail_closed.json",
                "l6_public_release_gate_fail_closed.json",
                "l6_owner_signing_checklist.json",
                "l6_release_signing_readiness_summary.json",
                "L6_PUBLIC_CLIENT_RELEASE_SIGNING_READINESS.md",
            }
            self.assertEqual(expected, set(artifacts))
            for name, digest in artifacts.items():
                self.assertTrue((out_dir / name).exists())
                self.assertRegex(digest, r"^[0-9a-f]{64}$")

            gate = json.loads((out_dir / "l6_public_release_gate_fail_closed.json").read_text())
            self.assertFalse(gate["metadata_ready"])
            self.assertFalse(gate["release_execution_allowed"])

            checklist = json.loads((out_dir / "l6_owner_signing_checklist.json").read_text())
            self.assertTrue(checklist["descriptor_ready_for_owner_review"])
            self.assertFalse(checklist["owner_signing_environment_ready"])
            self.assertFalse(checklist["public_release_ready"])


if __name__ == "__main__":
    unittest.main()
