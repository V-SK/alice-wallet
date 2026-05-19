from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


class Phase40GuiSourceTests(unittest.TestCase):
    def test_old_wallet_surfaces_are_not_in_normal_navigation(self):
        shell = read("gui/src/ui/shell.rs")
        dashboard = read("gui/src/ui/dashboard.rs")
        ui_mod = read("gui/src/ui/mod.rs")

        self.assertNotIn("Page::Stake", shell)
        self.assertNotIn("Page::Stake", dashboard)
        self.assertNotIn("pub mod stake", ui_mod)
        self.assertIn("Page::Receive", shell)
        self.assertIn("Page::Send", shell)
        self.assertIn("Page::Mining", shell)
        self.assertIn("Page::Accounts", shell)
        self.assertIn("Page::AddressBook", shell)
        self.assertIn("pub mod receive", ui_mod)
        self.assertIn("pub mod send", ui_mod)
        self.assertIn("pub mod mining", ui_mod)
        self.assertIn("pub mod accounts", ui_mod)
        self.assertIn("pub mod address_book", ui_mod)

    def test_raw_connection_and_local_file_paths_are_not_displayed(self):
        checked = "\n".join(
            read(path)
            for path in [
                "gui/src/ui/shell.rs",
                "gui/src/ui/dashboard.rs",
                "gui/src/ui/receive.rs",
                "gui/src/ui/send.rs",
                "gui/src/ui/mining.rs",
                "gui/src/ui/accounts.rs",
                "gui/src/ui/address_book.rs",
                "gui/src/ui/history_view.rs",
                "gui/src/ui/settings.rs",
                "gui/src/ui/unlock.rs",
            ]
        )
        self.assertNotIn("app.settings.rpc_url", checked)
        self.assertNotIn("settings_rpc_draft", checked)
        self.assertNotIn("wallet_path.display", checked)
        self.assertNotIn("detected_wallet_path.display", checked)
        self.assertNotIn("Save RPC", checked)

    def test_safe_send_review_has_no_execution_path(self):
        send_ui = read("gui/src/ui/send.rs")
        app = read("gui/src/app.rs")
        chain = read("gui/src/chain.rs")

        self.assertIn("parse_token_amount", send_ui)
        self.assertIn("validate_address", send_ui)
        self.assertIn("send_review_ready", app)
        self.assertIn("PRODUCTION_TRANSFER_ALLOWED: bool = false", chain)
        for forbidden in [
            "AsyncAction::Send",
            "AsyncAction::Transfer",
            "create_signed",
            "sign_and_submit",
            "compose_extrinsic",
            "broadcast",
            "submit_extrinsic",
        ]:
            self.assertNotIn(forbidden, send_ui)
            self.assertNotIn(forbidden, app)

    def test_receive_addressbook_do_not_expose_recovery_material(self):
        checked = "\n".join(
            read(path)
            for path in [
                "gui/src/ui/receive.rs",
                "gui/src/ui/address_book.rs",
            ]
        )
        for forbidden in [
            "mnemonic",
            "seed",
            "private",
            "wallet_path",
            "detected_wallet_path",
            "cache",
            "settings.rpc_url",
            "command",
        ]:
            self.assertNotIn(forbidden, checked)

        accounts = read("gui/src/ui/accounts.rs")
        self.assertIn("accounts.private_key_export", accounts)
        self.assertIn("reveal_private_key_export", accounts)
        self.assertIn("clear_private_key_export", accounts)
        self.assertNotIn("wallet_path", accounts)
        self.assertNotIn("detected_wallet_path", accounts)

    def test_wallet_mining_route_is_xmr_only_and_default_off(self):
        miner = read("gui/src/miner.rs")
        shell = read("gui/src/ui/shell.rs")
        ui_mod = read("gui/src/ui/mod.rs")

        self.assertIn("WalletMiningRouteKind::WalletXmr", miner)
        self.assertIn("pub const MINING_EXECUTION_ALLOWED: bool = false", miner)
        self.assertIn("pub const CUSTOM_POOL_ALLOWED: bool = false", miner)
        self.assertIn("pub const LTC_DOGE_ALLOWED: bool = false", miner)
        self.assertIn("pub const AI_JOBS_ALLOWED: bool = false", miner)
        self.assertIn("pub const POOL_CONFIG_VISIBLE: bool = false", miner)
        self.assertIn("Page::Mining", shell)
        self.assertIn("pub mod mining", ui_mod)
        self.assertNotIn("MinerProfile::Pool", miner)
        self.assertNotIn("build_miner_command", miner)
        self.assertNotIn("MinerCommand", miner)
        self.assertNotIn("endpoint", miner)
        self.assertNotIn("extra_args", miner)

    def test_wallet_mining_ui_hides_pool_and_execution_details(self):
        checked = "\n".join(
            read(path)
            for path in [
                "gui/src/ui/mining.rs",
                "gui/src/i18n.rs",
            ]
        )
        for forbidden in [
            "stra" + "tum",
            "foundation",
            "api token",
            "secret" + "_ref",
            "raw " + "command",
            "pool " + "endpoint",
            "wallet_path",
            "settings.rpc_url",
        ]:
            self.assertNotIn(forbidden, checked)

    def test_rewards_display_fail_closed_fields_exist(self):
        miner = read("gui/src/miner.rs")
        mining_ui = read("gui/src/ui/mining.rs")
        i18n = read("gui/src/i18n.rs")

        for field in [
            "estimated_rewards",
            "confirmed_rewards",
            "pending_rewards",
            "held_rewards",
            "released_rewards",
            "accepted_shares",
            "rejected_shares",
            "evidence_status",
            "evidence_freshness_seconds",
            "daily_window",
            "last_updated_at",
        ]:
            self.assertIn(field, miner)
            self.assertIn(field, mining_ui)

        self.assertIn("stale_evidence_does_not_become_confirmed", miner)
        self.assertIn("missing_evidence_is_pending_not_confirmed", miner)
        self.assertIn("预估奖励约每分钟更新", i18n)
        self.assertIn("daily after accepted-share evidence", i18n)

    def test_history_sanitizes_transaction_identifiers(self):
        history_ui = read("gui/src/ui/history_view.rs")
        self.assertIn("short_tx_id", history_ui)
        self.assertIn("hist.status_confirmed", read("gui/src/i18n.rs"))
        self.assertNotIn("RichText::new(&rec.hash)", history_ui)
        self.assertNotIn("wallet_path", history_ui)
        self.assertNotIn("settings.rpc_url", history_ui)

    def test_receive_sync_warning_is_productized(self):
        receive_ui = read("gui/src/ui/receive.rs")
        i18n = read("gui/src/i18n.rs")
        self.assertIn("NodeSyncState::Synced", receive_ui)
        self.assertIn("receive.sync_warning", receive_ui)
        self.assertIn("余额和历史可能仍在更新", i18n)

    def test_private_key_import_surface_is_confined_to_auth_import(self):
        import_ui = read("gui/src/ui/import.rs")
        app = read("gui/src/app.rs")
        accounts = read("gui/src/ui/accounts.rs")
        i18n = read("gui/src/i18n.rs")

        self.assertIn("ImportMethod::PrivateKey", import_ui)
        self.assertIn("ImportSeedHex", import_ui)
        self.assertIn("ImportSeedHex", app)
        self.assertIn("private_key_input", app)
        self.assertIn("clear_private_key_input", app)
        self.assertIn("auth.private_key_safety", import_ui)
        self.assertIn("accounts.private_key_export", accounts)
        self.assertIn("reveal_private_key_export", accounts)
        self.assertIn("auth.import_method_private_key", i18n)

        ordinary_ui = "\n".join(
            read(path)
            for path in [
                "gui/src/ui/shell.rs",
                "gui/src/ui/dashboard.rs",
                "gui/src/ui/receive.rs",
                "gui/src/ui/send.rs",
                "gui/src/ui/mining.rs",
                "gui/src/ui/address_book.rs",
                "gui/src/ui/history_view.rs",
                "gui/src/ui/settings.rs",
            ]
        )
        self.assertNotIn("private_key_input", ordinary_ui)
        self.assertNotIn("private_key_export", ordinary_ui)

    def test_recovery_material_is_confined_to_auth_backup_import_paths(self):
        ordinary_ui = "\n".join(
            read(path)
            for path in [
                "gui/src/ui/shell.rs",
                "gui/src/ui/dashboard.rs",
                "gui/src/ui/receive.rs",
                "gui/src/ui/send.rs",
                "gui/src/ui/mining.rs",
                "gui/src/ui/address_book.rs",
                "gui/src/ui/history_view.rs",
                "gui/src/ui/settings.rs",
            ]
        )
        for forbidden in [
            "mnemonic_backup",
            "mnemonic_words",
            "backup_quiz",
            "encrypted_mnemonic",
            "nonce_mnemonic",
            "copy_sensitive",
            "seed_hex",
            "wallet_path.display",
        ]:
            self.assertNotIn(forbidden, ordinary_ui)

        backup_ui = read("gui/src/ui/backup.rs")
        app = read("gui/src/app.rs")
        accounts = read("gui/src/ui/accounts.rs")
        self.assertIn("copy_sensitive", backup_ui)
        self.assertIn("clear_mnemonic_backup", backup_ui)
        self.assertIn("clear_mnemonic_backup", app)
        self.assertIn("copy_sensitive", accounts)
        self.assertIn("clear_private_key_export", accounts)

    def test_qa_backup_route_does_not_render_placeholder_recovery_words(self):
        app = read("gui/src/app.rs")
        backup_ui = read("gui/src/ui/backup.rs")
        i18n = read("gui/src/i18n.rs")

        self.assertIn("qa_redacted_preview", backup_ui)
        self.assertIn("app.qa_mock_mode && app.mnemonic_backup.is_empty()", backup_ui)
        self.assertIn("NO RECOVERY PHRASE LOADED", backup_ui)
        self.assertIn("Recovery phrase is not loaded in QA", i18n)
        self.assertNotIn("qa-redacted-", app)

    def test_import_errors_and_backup_toasts_do_not_expose_parser_or_paths(self):
        import_ui = read("gui/src/ui/import.rs")
        app = read("gui/src/app.rs")

        self.assertIn("auth.invalid_phrase_count", import_ui)
        self.assertIn("auth.invalid_mnemonic", import_ui)
        self.assertNotIn("Mnemonic must be", import_ui)
        self.assertNotIn("format!(\"{}: {}\"", import_ui)
        self.assertNotIn("Previous wallet moved to", app)
        self.assertNotIn("path.display()", app)

    def test_settings_security_copy_is_productized_and_sanitized(self):
        settings = read("gui/src/ui/settings.rs")
        i18n = read("gui/src/i18n.rs")

        for key in [
            "set.autolock",
            "set.autolock_label",
            "set.autolock_hint",
            "set.security",
            "set.lock_now",
        ]:
            self.assertIn(key, settings)
        self.assertNotIn("settings.rpc_url", settings)
        self.assertNotIn("wallet_path", settings)
        self.assertNotIn("Save failed\", e", settings)
        self.assertIn("自动锁定", i18n)
        self.assertIn("立即锁定钱包", i18n)

    def test_full_wallet_product_navigation_is_present(self):
        shell = read("gui/src/ui/shell.rs")
        for page in [
            "Page::Dashboard",
            "Page::Receive",
            "Page::Send",
            "Page::Mining",
            "Page::History",
            "Page::Accounts",
            "Page::AddressBook",
            "Page::Settings",
        ]:
            self.assertIn(page, shell)

    def test_node_sync_has_fail_closed_fields_and_product_copy(self):
        chain = read("gui/src/chain.rs")
        dashboard = read("gui/src/ui/dashboard.rs")
        i18n = read("gui/src/i18n.rs")

        for field in [
            "sync_mode",
            "status",
            "current_height",
            "target_height",
            "remaining_blocks",
            "progress_percent",
            "peers_count",
            "network_status",
            "last_updated_at",
            "freshness_seconds",
            "fail_closed_reason",
        ]:
            self.assertIn(field, chain)

        self.assertIn("missing_target_height", chain)
        self.assertIn("stale_node_evidence", chain)
        self.assertIn("node_offline", chain)
        self.assertIn("sync.reason_stale", dashboard)
        self.assertIn("同步中", i18n)
        self.assertIn("已同步", i18n)
        self.assertIn("连接中断", i18n)
        self.assertIn("数据过期", i18n)
        self.assertIn("正在连接节点", i18n)

    def test_product_ui_copy_avoids_engineering_terms(self):
        checked = "\n".join(
            read(path)
            for path in [
                "gui/src/ui/shell.rs",
                "gui/src/ui/dashboard.rs",
                "gui/src/ui/receive.rs",
                "gui/src/ui/send.rs",
                "gui/src/ui/mining.rs",
                "gui/src/ui/accounts.rs",
                "gui/src/ui/address_book.rs",
                "gui/src/ui/history_view.rs",
                "gui/src/ui/settings.rs",
                "gui/src/ui/unlock.rs",
                "gui/src/i18n.rs",
            ]
        )
        forbidden = [
            "dry-run",
            "manifest",
            "production gate",
            "hash policy",
            "human approval",
            "provider",
            "测试版",
            "gover" + "nance",
            "De" + "Fi",
            "approval " + "grant",
            "payout " + "authority",
            "secret" + "_ref",
            "hash policy",
        ]
        for term in forbidden:
            self.assertNotIn(term, checked)

    def test_macos_app_icon_bundle_resources_are_configured(self):
        main = read("gui/src/main.rs")
        bundle_script = read("gui/scripts/build_qa_app_bundle.sh")
        icon_script = read("gui/scripts/build_macos_icon.sh")

        self.assertIn("with_icon(icon)", main)
        self.assertIn("CFBundleIconFile", bundle_script)
        self.assertIn("AliceWallet.icns", bundle_script)
        self.assertIn("iconutil -c icns", icon_script)
        self.assertTrue((ROOT / "gui/assets/macos/AliceWallet.icns").exists())


if __name__ == "__main__":
    unittest.main()
