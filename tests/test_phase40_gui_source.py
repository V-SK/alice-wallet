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
        self.assertNotIn("Page::Send", shell)
        self.assertNotIn("Page::Stake", dashboard)
        self.assertNotIn("Page::Send", dashboard)
        self.assertNotIn("app.page = Page::Send", dashboard)
        self.assertNotIn("pub mod stake", ui_mod)
        self.assertNotIn("pub mod send", ui_mod)

    def test_raw_connection_and_local_file_paths_are_not_displayed(self):
        checked = "\n".join(
            read(path)
            for path in [
                "gui/src/ui/shell.rs",
                "gui/src/ui/dashboard.rs",
                "gui/src/ui/settings.rs",
                "gui/src/ui/unlock.rs",
            ]
        )
        self.assertNotIn("app.settings.rpc_url", checked)
        self.assertNotIn("settings_rpc_draft", checked)
        self.assertNotIn("wallet_path.display", checked)
        self.assertNotIn("detected_wallet_path.display", checked)
        self.assertNotIn("Save RPC", checked)

    def test_raw_seed_import_surface_is_absent_from_gui(self):
        import_ui = read("gui/src/ui/import.rs")
        app = read("gui/src/app.rs")
        self.assertNotIn("SeedHex", import_ui)
        self.assertNotIn("seed_hex_input", import_ui)
        self.assertNotIn("ImportSeedHex", import_ui)
        self.assertNotIn("SeedHex", app)
        self.assertNotIn("seed_hex_input", app)

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
        ]
        for term in forbidden:
            self.assertNotIn(term, checked)


if __name__ == "__main__":
    unittest.main()
