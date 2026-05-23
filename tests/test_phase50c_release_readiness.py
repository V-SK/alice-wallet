import json
import tempfile
import unittest
from pathlib import Path

import phase50c_release_readiness as phase50c


SOURCE_COMMIT = "25f274dec8d462cea799d1f43e9771a2bd6d8085"
PHASE40U_COMMIT = "ae1ffdba9a1f88193b85bc18023095d65550d6c9"
CHANGED_FILES = ["gui/src/app.rs", "gui/src/chain.rs"]


class Phase50CWalletReleaseReadinessTests(unittest.TestCase):
    def test_contract_binding_preserves_phase50_release_contract_boundaries(self):
        binding = phase50c.build_contract_binding(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0-phase50c",
            phase40u_commit=PHASE40U_COMMIT,
            changed_since_phase40u=CHANGED_FILES,
        )

        self.assertEqual(binding["phase50_contract_id"], phase50c.PHASE50_CONTRACT_ID)
        self.assertEqual(binding["wallet_update_mode"], "optional_prompt_with_signature_verification")
        self.assertEqual(binding["public_distribution_backend"], "HF")
        self.assertEqual(binding["cold_archive_backend"], "/mnt/storage")
        self.assertFalse(binding["oss_or_aliyun_allowed"])
        self.assertTrue(binding["signed_manifest_required_before_release"])
        self.assertTrue(binding["rollback_package_required_before_release"])
        for value in binding["closed_boundaries"].values():
            self.assertIs(value, False)
        phase50c.validate_phase50c_packet(binding)

    def test_phase40u_reuse_evidence_binds_current_safety_successor_commit(self):
        evidence = phase50c.build_phase40u_reuse_evidence(
            phase40u_commit=PHASE40U_COMMIT,
            current_wallet_commit=SOURCE_COMMIT,
            changed_since_phase40u=CHANGED_FILES,
        )

        self.assertEqual(evidence["phase40u_state"], phase50c.PHASE40U_STATE)
        self.assertEqual(evidence["current_wallet_commit"], SOURCE_COMMIT)
        self.assertEqual(evidence["changed_since_phase40u"], sorted(CHANGED_FILES))
        self.assertTrue(evidence["release_ops_module_inherited"])
        self.assertTrue(evidence["rebuild_required_before_public_release"])

    def test_signed_distribution_matrix_is_descriptor_only(self):
        matrix = phase50c.build_signed_distribution_matrix(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0-phase50c",
        )

        self.assertEqual(matrix["hf_distribution"]["backend"], "HF")
        self.assertEqual(matrix["hf_distribution"]["mode"], "descriptor_only_no_upload")
        self.assertEqual(matrix["cold_archive"]["backend"], "/mnt/storage")
        self.assertEqual(matrix["cold_archive"]["mode"], "descriptor_only_no_write")
        self.assertEqual({item["platform"] for item in matrix["platforms"]}, {"macos-arm64", "windows-x64"})
        for value in matrix["closed_boundaries"].values():
            self.assertIs(value, False)

    def test_update_and_rollback_rehearsals_fail_closed_without_execution(self):
        update = phase50c.build_update_manifest_rehearsal(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0-phase50c",
        )
        rollback = phase50c.build_rollback_rehearsal(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0-phase50c",
        )

        self.assertTrue(update["manifest_signature_required_before_release"])
        self.assertTrue(update["bad_signature_fail_closed"])
        self.assertTrue(update["bad_hash_fail_closed"])
        self.assertTrue(rollback["rollback_package_required_before_release"])
        self.assertFalse(rollback["rollback_execution_performed"])
        self.assertFalse(rollback["destructive_cleanup_allowed"])
        phase50c.validate_phase50c_packet(update)
        phase50c.validate_phase50c_packet(rollback)

    def test_validator_rejects_secret_fields_open_flags_and_runtime_values(self):
        packet = phase50c.build_signed_distribution_matrix(
            source_commit=SOURCE_COMMIT,
            app_version="0.1.0-phase50c",
        )

        unsafe = json.loads(json.dumps(packet))
        unsafe["signing_key"] = "placeholder"
        with self.assertRaises(ValueError):
            phase50c.validate_phase50c_packet(unsafe)

        unsafe = json.loads(json.dumps(packet))
        unsafe["closed_boundaries"]["actual_signing_performed"] = True
        with self.assertRaises(ValueError):
            phase50c.validate_phase50c_packet(unsafe)

        unsafe = json.loads(json.dumps(packet))
        unsafe["raw_url"] = "https://example.invalid/artifact"
        with self.assertRaises(ValueError):
            phase50c.validate_phase50c_packet(unsafe)

    def test_artifact_writer_outputs_phase50c_packets_and_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_dir = Path(tmp)
            artifacts = phase50c.write_phase50c_artifacts(
                out_dir=out_dir,
                source_commit=SOURCE_COMMIT,
                app_version="0.1.0-phase50c",
                phase40u_commit=PHASE40U_COMMIT,
                changed_since_phase40u=CHANGED_FILES,
            )

            expected = {
                "phase50c_release_contract_binding.json",
                "phase50c_signed_distribution_matrix.json",
                "phase50c_update_manifest_rehearsal.json",
                "phase50c_rollback_rehearsal.json",
                "phase50c_release_leak_recheck.json",
                "phase50c_static_scan_summary.json",
                "phase50c_wallet_release_readiness_summary.json",
                "PHASE50C_WALLET_SIGNED_DISTRIBUTION_READINESS.md",
            }
            self.assertEqual(expected, set(artifacts))
            for name, digest in artifacts.items():
                self.assertTrue((out_dir / name).exists())
                self.assertRegex(digest, r"^[0-9a-f]{64}$")

            summary = json.loads((out_dir / "phase50c_wallet_release_readiness_summary.json").read_text())
            self.assertEqual(summary["source_commit"], SOURCE_COMMIT)
            self.assertEqual(summary["phase40u_commit"], PHASE40U_COMMIT)
            for value in summary["closed_boundaries"].values():
                self.assertIs(value, False)


if __name__ == "__main__":
    unittest.main()
