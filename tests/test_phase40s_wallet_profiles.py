from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


class Phase40SWalletProfileSourceTests(unittest.TestCase):
    def test_profile_manager_module_and_safe_metadata_are_present(self):
        profiles = read("gui/src/wallet_profiles.rs")
        main = read("gui/src/main.rs")

        self.assertIn("mod wallet_profiles;", main)
        self.assertIn("WalletProfileManager", profiles)
        self.assertIn("WalletProfileMetadata", profiles)
        self.assertIn("safe_profiles", profiles)
        self.assertIn("profile_id", profiles)
        self.assertIn("node_profile_label", profiles)
        self.assertNotIn("PathBuf", profiles.split("pub struct WalletProfileMetadata", 1)[1].split("}", 1)[0])

    def test_create_import_and_switching_fail_closed_behaviors_are_tested(self):
        profiles = read("gui/src/wallet_profiles.rs")
        app = read("gui/src/app.rs")

        self.assertIn("duplicate_profile_registration_is_rejected", profiles)
        self.assertIn("import_reservation_uses_unique_profile_storage", profiles)
        self.assertIn("switching_active_profile_clears_in_memory_secrets", app)
        self.assertIn("select_wallet_profile", app)
        self.assertIn("clear_active_wallet_secret_state", app)

    def test_signing_and_label_metadata_boundaries_are_present(self):
        profiles = read("gui/src/wallet_profiles.rs")

        self.assertIn("WalletProfileAccess::ReadOnly", profiles)
        self.assertIn("WalletProfileAccess::DisplayOnly", profiles)
        self.assertIn("can_sign", profiles)
        self.assertIn("locked_read_only_and_display_only_profiles_cannot_sign", profiles)
        self.assertIn("labels_and_local_records_reject_recovery_material", profiles)
        self.assertIn("AddressBookRecord", profiles)
        self.assertIn("ReceiveRequestRecord", profiles)

    def test_recent_metadata_and_mining_identity_use_active_profile(self):
        profiles = read("gui/src/wallet_profiles.rs")
        app = read("gui/src/app.rs")
        mining = read("gui/src/ui/mining.rs")

        self.assertIn("recent_metadata_rejects_sensitive_material", profiles)
        self.assertIn("mining_reward_identity_uses_selected_profile_address", profiles)
        self.assertIn("selected_reward_identity", app)
        self.assertIn("selected_reward_identity()", mining)

    def test_no_monero_wallet_probe_or_raw_path_display_is_introduced(self):
        monero_probe_checked = "\n".join(
            read(path)
            for path in [
                "gui/src/wallet_profiles.rs",
                "gui/src/app.rs",
                "gui/src/ui/unlock.rs",
                "gui/src/ui/accounts.rs",
                "gui/src/ui/address_book.rs",
                "gui/src/ui/receive.rs",
            ]
        )
        for forbidden in [
            "monero-wallet-gui",
            "/Monero",
            "Monero/wallets",
            ".bitmonero",
        ]:
            self.assertNotIn(forbidden, monero_probe_checked)

        display_checked = "\n".join(
            read(path)
            for path in [
                "gui/src/wallet_profiles.rs",
                "gui/src/ui/unlock.rs",
                "gui/src/ui/accounts.rs",
                "gui/src/ui/address_book.rs",
                "gui/src/ui/receive.rs",
            ]
        )
        for forbidden in [
            "wallet_path.display",
            "detected_wallet_path.display",
            "settings.rpc_url",
            "stdout",
            "stderr",
        ]:
            self.assertNotIn(forbidden, display_checked)

    def test_qa_mock_mode_has_two_display_only_profiles_without_loading_settings(self):
        profiles = read("gui/src/wallet_profiles.rs")
        app = read("gui/src/app.rs")

        self.assertIn("qa_mock_profiles", profiles)
        self.assertIn("QA mock warm wallet", profiles)
        self.assertIn("QA mock cold wallet", profiles)
        self.assertIn("qa_mock_mode_exposes_two_display_only_profiles", app)
        self.assertIn("Settings::default()", app)
        qa_branch = app.split("let settings = if qa_mock_mode", 1)[1].split("} else {", 1)[0]
        self.assertIn("Settings::default()", qa_branch)
        self.assertNotIn("Settings::load()", qa_branch)


if __name__ == "__main__":
    unittest.main()
