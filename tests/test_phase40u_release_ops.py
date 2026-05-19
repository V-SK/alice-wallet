import json
import tempfile
import unittest
from pathlib import Path

import release_ops


class Phase40UReleaseOpsTests(unittest.TestCase):
    def test_release_manifest_contains_required_default_off_fields(self):
        manifest = release_ops.build_release_manifest(
            source_commit="2c5289398940c44cf30230e54d36d5d3ca748d69",
            app_version="0.1.0",
            platform="macos-arm64",
        )

        self.assertEqual(manifest["app_name"], "Alice Wallet")
        self.assertEqual(manifest["app_version"], "0.1.0")
        self.assertEqual(manifest["platform"], "macos-arm64")
        self.assertEqual(manifest["artifact_path"], "UNSIGNED_UNPUBLISHED_CANDIDATE_PLACEHOLDER")
        self.assertEqual(manifest["artifact_sha256"], "UNSIGNED_UNPUBLISHED_CANDIDATE_PLACEHOLDER")
        self.assertEqual(manifest["source_commit"], "2c5289398940c44cf30230e54d36d5d3ca748d69")
        self.assertEqual(manifest["build_profile"], "release-candidate-readiness")
        self.assertEqual(manifest["release_state"], "default_off_unsigned_unpublished")

        closed = manifest["closed_execution"]
        for key in [
            "signing_notarization_uploaded",
            "release_uploaded",
            "public_distribution",
            "updater_executed",
            "hf_mutation",
            "storage_box_mutation",
            "website_published",
        ]:
            self.assertIs(closed[key], False)

        release_ops.validate_release_ops_packet(manifest)

    def test_manifest_rejects_opened_distribution_or_signing(self):
        manifest = release_ops.build_release_manifest(
            source_commit="2c5289398940c44cf30230e54d36d5d3ca748d69",
            app_version="0.1.0",
            platform="macos-arm64",
        )

        for key in [
            "signing_notarization_uploaded",
            "release_uploaded",
            "public_distribution",
            "updater_executed",
            "hf_mutation",
            "storage_box_mutation",
            "website_published",
        ]:
            opened = json.loads(json.dumps(manifest))
            opened["closed_execution"][key] = True
            with self.subTest(key=key):
                with self.assertRaises(ValueError):
                    release_ops.validate_release_ops_packet(opened)

    def test_sensitive_and_raw_runtime_fields_are_rejected(self):
        manifest = release_ops.build_release_manifest(
            source_commit="2c5289398940c44cf30230e54d36d5d3ca748d69",
            app_version="0.1.0",
            platform="macos-arm64",
        )

        for forbidden_key in [
            "seed",
            "private_key",
            "mnemonic",
            "password",
            "token",
            "secret",
            "signing_material",
            "wallet_path",
            "rpc_endpoint",
            "pool_endpoint",
            "command",
            "stdout",
            "stderr",
        ]:
            unsafe = json.loads(json.dumps(manifest))
            unsafe[forbidden_key] = "redacted-placeholder"
            with self.subTest(forbidden_key=forbidden_key):
                with self.assertRaises(ValueError):
                    release_ops.validate_release_ops_packet(unsafe)

    def test_distribution_handoffs_are_descriptor_only(self):
        source_commit = "2c5289398940c44cf30230e54d36d5d3ca748d69"
        hf = release_ops.build_hf_distribution_handoff(source_commit)
        storage = release_ops.build_storage_archive_handoff(source_commit)
        website = release_ops.build_website_download_metadata_handoff(source_commit)

        self.assertEqual(hf["backend"], "HF")
        self.assertEqual(hf["mode"], "descriptor_only_no_upload")
        self.assertFalse(hf["upload_executed"])
        self.assertFalse(hf["mutation_allowed"])

        self.assertEqual(storage["backend"], "/mnt/storage")
        self.assertEqual(storage["mode"], "descriptor_only_no_write")
        self.assertFalse(storage["archive_write_executed"])
        self.assertFalse(storage["mutation_allowed"])

        self.assertEqual(website["mode"], "handoff_only_no_website_repo_edit")
        self.assertFalse(website["website_repo_modified"])
        self.assertFalse(website["public_download_enabled"])

        for packet in [hf, storage, website]:
            release_ops.validate_release_ops_packet(packet)

    def test_release_copy_only_allows_unsigned_unpublished_candidate_language(self):
        safe_copy = (
            "Unsigned unpublished wallet release-candidate readiness packet. "
            "Default-off handoff only; owner approval is still required."
        )
        release_ops.validate_release_copy(safe_copy)

        for unsafe in [
            "public launch is approved",
            "payout is guaranteed",
            "settlement authority is open",
            "release upload completed",
            "notarization submitted",
        ]:
            with self.subTest(unsafe=unsafe):
                with self.assertRaises(ValueError):
                    release_ops.validate_release_copy(unsafe)

    def test_leak_audit_records_counts_and_paths_without_matched_text(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            safe = root / "safe.json"
            unsafe = root / "unsafe.json"
            safe.write_text('{"state":"default_off_unsigned_unpublished"}\n', encoding="utf-8")
            unsafe.write_text('{"forbidden":"wallet_path"}\n', encoding="utf-8")

            audit = release_ops.build_recovery_material_leak_audit([safe, unsafe])

        self.assertEqual(audit["scan_policy"], "counts_and_paths_only_no_match_content")
        self.assertEqual(audit["matches"], 1)
        self.assertIn(str(unsafe), audit["paths_with_matches"])
        self.assertNotIn("wallet_path", json.dumps(audit))

    def test_artifact_writer_outputs_descriptor_packets_and_sha256_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_dir = Path(tmp)
            artifacts = release_ops.write_phase40u_artifacts(
                out_dir=out_dir,
                source_commit="2c5289398940c44cf30230e54d36d5d3ca748d69",
                app_version="0.1.0",
            )

            expected = {
                "release_manifest_candidate.json",
                "hf_distribution_handoff.json",
                "storage_archive_handoff.json",
                "website_download_metadata_handoff.json",
                "recovery_material_leak_audit.json",
                "phase40u_wallet_release_ops_summary.json",
                "PHASE40U_WALLET_RELEASE_OPS_2026-05-19.md",
            }
            self.assertEqual(expected, set(artifacts.keys()))
            for name, meta in artifacts.items():
                self.assertTrue((out_dir / name).exists())
                self.assertRegex(meta["sha256"], r"^[0-9a-f]{64}$")


if __name__ == "__main__":
    unittest.main()
