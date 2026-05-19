import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GUI_SRC = ROOT / "gui" / "src"


def read_source(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def function_body(source: str, name: str) -> str:
    match = re.search(rf"(?:pub\s+)?fn\s+{re.escape(name)}\b", source)
    if not match:
        return ""
    start = match.start()
    brace = source.find("{", match.end())
    if brace < 0:
        return ""
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    return ""


class Phase40TOwnerNoMockSourceTests(unittest.TestCase):
    def test_no_mock_data_root_override_is_shared_by_wallet_config_profiles_and_history(self):
        config = read_source("gui/src/config.rs")
        crypto = read_source("gui/src/crypto.rs")
        profiles = read_source("gui/src/wallet_profiles.rs")
        history = read_source("gui/src/history.rs")

        self.assertIn("ALICE_WALLET_DATA_ROOT", config)
        self.assertIn("wallet_data_root", config)
        self.assertIn("wallet_data_root_is_overridden", config)
        self.assertIn("wallet_data_root().join(\"config.json\")", config)
        self.assertIn("config::wallet_data_root().join(\"wallet.json\")", crypto)
        self.assertIn("config::wallet_data_root_is_overridden()", crypto)
        self.assertIn("return primary", function_body(crypto, "detect_wallet_path"))
        self.assertIn("crate::config::wallet_data_root()", profiles)
        self.assertIn("crate::config::wallet_data_root().join(\"history.json\")", history)

    def test_private_key_export_requires_reauth_and_clears_secret_state(self):
        app = read_source("gui/src/app.rs")
        reveal_body = function_body(app, "reveal_private_key_export")

        self.assertIn("private_key_export_password", app)
        self.assertIn("clear_private_key_export_password", app)
        self.assertIn("crypto::unlock_wallet(payload, &password)", reveal_body)
        self.assertNotIn("self.secrets.as_ref()", reveal_body)
        self.assertIn("self.clear_private_key_export()", app)
        self.assertIn("self.clear_private_key_export_password()", app)

        accounts = read_source("gui/src/ui/accounts.rs")
        self.assertIn("TextEdit::singleline(&mut app.private_key_export_password)", accounts)
        self.assertIn("reveal_private_key_export", accounts)

    def test_page_switch_lock_and_profile_switch_clear_private_key_export(self):
        app = read_source("gui/src/app.rs")
        shell = read_source("gui/src/ui/shell.rs")

        set_page_body = function_body(app, "set_page")
        self.assertIn("Page::Accounts", set_page_body)
        self.assertIn("clear_private_key_export", set_page_body)
        self.assertIn("set_page(Page::", shell)
        self.assertNotIn("app.page = Page::", shell)

        self.assertIn("clear_active_wallet_secret_state", app)
        clear_body = function_body(app, "clear_active_wallet_secret_state")
        self.assertIn("clear_private_key_export", clear_body)
        self.assertIn("clear_private_key_export_password", clear_body)

    def test_evidence_redaction_hides_recovery_material_in_no_mock_screenshots(self):
        app = read_source("gui/src/app.rs")
        backup = read_source("gui/src/ui/backup.rs")

        self.assertIn("ALICE_WALLET_EVIDENCE_REDACT_SECRETS", app)
        self.assertIn("evidence_redact_secrets", app)
        self.assertIn("app.evidence_redact_secrets", backup)
        self.assertIn("recovery_hidden_for_evidence", backup)
        self.assertNotIn("ALICE_WALLET_QA_MOCK=1", backup)

    def test_network_disabled_mode_fails_closed_without_rpc_gateway_request(self):
        app = read_source("gui/src/app.rs")
        chain = read_source("gui/src/chain.rs")

        self.assertIn("ALICE_WALLET_NETWORK_DISABLED", app)
        self.assertIn("network_disabled", app)
        start_refresh_body = function_body(app, "start_refresh")
        self.assertIn("owner_test_network_disabled", start_refresh_body)
        self.assertIn("NodeSyncSnapshot::unavailable", start_refresh_body)
        self.assertIn("return;", start_refresh_body)
        self.assertIn("pub fn unavailable", chain)

    def test_no_mock_evidence_route_requires_isolated_redacted_network_disabled(self):
        app = read_source("gui/src/app.rs")
        evidence_body = function_body(app, "enable_phase40t_evidence_mode")

        self.assertIn("ALICE_WALLET_PHASE40T_EVIDENCE_PAGE", app)
        self.assertIn("phase40t_evidence_page_from_env", app)
        self.assertIn("!self.network_disabled", evidence_body)
        self.assertIn("!self.evidence_redact_secrets", evidence_body)
        self.assertIn("wallet_data_root_is_overridden", evidence_body)
        self.assertIn("owner_test_network_disabled", evidence_body)
        self.assertIn("self.qa_mock_mode", evidence_body)
        self.assertNotIn("ALICE_WALLET_QA_MOCK", evidence_body)

    def test_owner_test_source_does_not_open_release_or_mining_boundaries(self):
        combined = "\n".join(
            path.read_text(encoding="utf-8")
            for path in [
                GUI_SRC / "app.rs",
                GUI_SRC / "miner.rs",
                GUI_SRC / "ui" / "mining.rs",
                GUI_SRC / "main.rs",
            ]
        )

        forbidden_enabled = [
            "SIGNING_ALLOWED: bool = true",
            "BROADCAST_ALLOWED: bool = true",
            "MINING_EXECUTION_ALLOWED: bool = true",
            "CUSTOM_POOL_ALLOWED: bool = true",
            "LTC_DOGE_ALLOWED: bool = true",
            "RELEASE_UPLOAD_ALLOWED: bool = true",
            "NOTARIZATION_ALLOWED: bool = true",
        ]
        for marker in forbidden_enabled:
            self.assertNotIn(marker, combined)

        self.assertIn("MINING_EXECUTION_ALLOWED: bool = false", combined)
        self.assertIn("CUSTOM_POOL_ALLOWED: bool = false", combined)
        self.assertIn("LTC_DOGE_ALLOWED: bool = false", combined)


if __name__ == "__main__":
    unittest.main()
