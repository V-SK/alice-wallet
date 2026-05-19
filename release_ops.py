"""Phase40U release-ops readiness descriptors.

This module builds unsigned, unpublished release-readiness packets only. It does
not sign, notarize, upload, execute an updater, modify the website repo, or touch
any real wallet data.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


STATE = "phase40u_closed_wallet_release_ops_readiness_default_off"
APP_NAME = "Alice Wallet"
DEFAULT_PLATFORM = "macos-arm64"
PLACEHOLDER = "UNSIGNED_UNPUBLISHED_CANDIDATE_PLACEHOLDER"
HF_BACKEND = "HF"
STORAGE_BACKEND = "/mnt/storage"

FORBIDDEN_FIELD_NAMES = {
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
}

FORBIDDEN_OPEN_FLAGS = {
    "signing_notarization_uploaded",
    "release_uploaded",
    "public_distribution",
    "updater_executed",
    "hf_mutation",
    "storage_box_mutation",
    "website_published",
}

FORBIDDEN_VALUE_FRAGMENTS = (
    "http://",
    "https://",
    "wss://",
    "ws://",
    "stratum",
    "/Users/",
    "/home/",
    "/root/",
    "C:\\",
)

FORBIDDEN_RELEASE_COPY = (
    "public launch is approved",
    "public launch approved",
    "payout is guaranteed",
    "reward is guaranteed",
    "settlement authority is open",
    "release upload completed",
    "notarization submitted",
    "signed distribution live",
    "public download enabled",
)

LEAK_AUDIT_MARKERS = (
    "wallet_path",
    "rpc_endpoint",
    "pool_endpoint",
    "seed=",
    "mnemonic=",
    "password=",
    "token=",
    "private_key=",
    "signing_material",
    "stdout",
    "stderr",
)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _walk_packet(obj: Any, path: str = "$") -> Iterable[tuple[str, Any]]:
    if isinstance(obj, dict):
        for key, value in obj.items():
            current = f"{path}.{key}"
            yield current, value
            yield from _walk_packet(value, current)
    elif isinstance(obj, list):
        for index, value in enumerate(obj):
            yield from _walk_packet(value, f"{path}[{index}]")


def validate_release_copy(copy: str) -> None:
    lowered = copy.lower()
    for fragment in FORBIDDEN_RELEASE_COPY:
        if fragment in lowered:
            raise ValueError(f"release copy opens forbidden claim: {fragment}")
    required = ("unsigned", "unpublished", "candidate", "default-off")
    if not all(term in lowered for term in required):
        raise ValueError("release copy must state unsigned, unpublished, candidate, and default-off")


def validate_release_ops_packet(packet: dict[str, Any]) -> None:
    for path, value in _walk_packet(packet):
        key = path.rsplit(".", 1)[-1].lower()
        if key in FORBIDDEN_FIELD_NAMES:
            raise ValueError(f"forbidden field in release packet: {path}")
        if key in FORBIDDEN_OPEN_FLAGS and value is not False:
            raise ValueError(f"forbidden release execution flag is open: {path}")
        if isinstance(value, str):
            lowered = value.lower()
            for fragment in FORBIDDEN_VALUE_FRAGMENTS:
                if fragment.lower() in lowered:
                    raise ValueError(f"forbidden raw runtime value in release packet: {path}")


def build_closed_execution() -> dict[str, bool]:
    return {
        "signing_notarization_uploaded": False,
        "release_uploaded": False,
        "public_distribution": False,
        "updater_executed": False,
        "hf_mutation": False,
        "storage_box_mutation": False,
        "website_published": False,
        "real_mining_pool_gateway_api": False,
        "broadcast_or_funds_tx": False,
        "payout_mint_settlement": False,
        "chain_tx_or_runtime_upgrade": False,
        "production_mutation": False,
    }


def build_release_manifest(
    *,
    source_commit: str,
    app_version: str,
    platform: str = DEFAULT_PLATFORM,
) -> dict[str, Any]:
    manifest = {
        "schema": "alice.wallet.release_manifest_candidate.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "app_name": APP_NAME,
        "app_version": app_version,
        "platform": platform,
        "artifact_path": PLACEHOLDER,
        "artifact_sha256": PLACEHOLDER,
        "source_commit": source_commit,
        "build_profile": "release-candidate-readiness",
        "release_state": "default_off_unsigned_unpublished",
        "bundle_metadata": {
            "bundle_display_name": APP_NAME,
            "bundle_identifier": "org.aliceprotocol.wallet",
            "bundle_short_version": app_version,
            "bundle_version": app_version,
            "icon_resource": "AliceWallet.icns",
            "minimum_macos": "13.0",
        },
        "macos_rehearsal": {
            "mode": "checklist_only_no_certificate_use",
            "archive_validation": "pending_unsigned_local_candidate",
            "codesign_invocation_executed": False,
            "notarization_submission_executed": False,
            "hardened_runtime_review_required": True,
            "nested_code_review_required": True,
            "entitlements_review_required": True,
        },
        "windows_rehearsal": {
            "mode": "checklist_only_no_certificate_use",
            "installer_descriptor": "pending_unsigned_candidate",
            "authenticode_invocation_executed": False,
            "timestamping_invocation_executed": False,
        },
        "integrity": {
            "source_commit_bound": True,
            "checksum_algorithm": "sha256",
            "checksum_status": "placeholder_until_unsigned_candidate_exists",
        },
        "material_handling": {
            "real_wallet_data_included": False,
            "local_profile_data_included": False,
            "runtime_logs_included": False,
            "sensitive_material_values_included": False,
        },
        "closed_execution": build_closed_execution(),
    }
    validate_release_ops_packet(manifest)
    return manifest


def build_hf_distribution_handoff(source_commit: str) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.hf_distribution_handoff.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "backend": HF_BACKEND,
        "mode": "descriptor_only_no_upload",
        "source_commit": source_commit,
        "artifact_ref": PLACEHOLDER,
        "artifact_sha256": PLACEHOLDER,
        "upload_executed": False,
        "mutation_allowed": False,
        "closed_execution": build_closed_execution(),
    }
    validate_release_ops_packet(packet)
    return packet


def build_storage_archive_handoff(source_commit: str) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.storage_archive_handoff.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "backend": STORAGE_BACKEND,
        "mode": "descriptor_only_no_write",
        "source_commit": source_commit,
        "archive_ref": PLACEHOLDER,
        "artifact_sha256": PLACEHOLDER,
        "archive_write_executed": False,
        "mutation_allowed": False,
        "closed_execution": build_closed_execution(),
    }
    validate_release_ops_packet(packet)
    return packet


def build_website_download_metadata_handoff(source_commit: str) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.website_download_metadata_handoff.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "mode": "handoff_only_no_website_repo_edit",
        "source_commit": source_commit,
        "display_name": APP_NAME,
        "candidate_label": "Unsigned unpublished wallet candidate",
        "artifact_ref": PLACEHOLDER,
        "artifact_sha256": PLACEHOLDER,
        "website_repo_modified": False,
        "public_download_enabled": False,
        "closed_execution": build_closed_execution(),
    }
    validate_release_copy(packet["candidate_label"].lower() + " default-off")
    validate_release_ops_packet(packet)
    return packet


def build_recovery_material_leak_audit(paths: Iterable[Path]) -> dict[str, Any]:
    checked: list[str] = []
    matched: list[str] = []
    for path in paths:
        checked.append(str(path))
        text = path.read_text(encoding="utf-8", errors="replace").lower()
        if any(marker in text for marker in LEAK_AUDIT_MARKERS):
            matched.append(str(path))
    return {
        "schema": "alice.wallet.recovery_material_leak_audit.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "scan_policy": "counts_and_paths_only_no_match_content",
        "paths_checked": checked,
        "matches": len(matched),
        "paths_with_matches": matched,
        "match_content_recorded": False,
        "result": "pass" if not matched else "review_required",
    }


def _write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _write_markdown(path: Path, *, source_commit: str, app_version: str, artifact_meta: dict[str, dict[str, str]]) -> None:
    lines = [
        "# Alice Phase40U Wallet Release Ops Readiness",
        "",
        "Date: 2026-05-19",
        f"State: `{STATE}`",
        f"Wallet feature commit: `{source_commit}`",
        f"App version: `{app_version}`",
        "",
        "Phase40U is an unsigned, unpublished release-ops readiness packet. It records",
        "manifest, checksum, signing rehearsal, distribution handoff, archive handoff,",
        "website metadata handoff, and leak-audit evidence without performing release",
        "execution.",
        "",
        "## Still Closed",
        "",
        "- Actual app signing.",
        "- Notarization submission.",
        "- Windows signing.",
        "- Release upload.",
        "- Updater execution.",
        "- Website publication or public download.",
        "- HF, Storage Box, or Vercel mutation.",
        "- Real mining, pool, gateway, or API connection.",
        "- Broadcast, payout, mint, settlement, chain transaction, runtime upgrade, deploy, or production mutation.",
        "",
        "## Artifacts",
        "",
    ]
    for name, meta in sorted(artifact_meta.items()):
        lines.append(f"- `{name}`")
        lines.append(f"  - SHA-256: `{meta['sha256']}`")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_phase40u_artifacts(
    *,
    out_dir: Path,
    source_commit: str,
    app_version: str,
    platform: str = DEFAULT_PLATFORM,
) -> dict[str, dict[str, str]]:
    out_dir.mkdir(parents=True, exist_ok=True)

    payloads = {
        "release_manifest_candidate.json": build_release_manifest(
            source_commit=source_commit,
            app_version=app_version,
            platform=platform,
        ),
        "hf_distribution_handoff.json": build_hf_distribution_handoff(source_commit),
        "storage_archive_handoff.json": build_storage_archive_handoff(source_commit),
        "website_download_metadata_handoff.json": build_website_download_metadata_handoff(source_commit),
    }
    for name, payload in payloads.items():
        _write_json(out_dir / name, payload)

    audit = build_recovery_material_leak_audit(
        out_dir / name for name in payloads.keys()
    )
    _write_json(out_dir / "recovery_material_leak_audit.json", audit)

    artifact_meta: dict[str, dict[str, str]] = {}
    for path in sorted(out_dir.glob("*.json")):
        if path.name.startswith("._"):
            continue
        artifact_meta[path.name] = {"path": str(path), "sha256": sha256_file(path)}

    summary = {
        "schema": "alice.wallet.phase40u_release_ops_summary.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "ready_after_phase40u": "unsigned_unpublished_wallet_release_candidate_readiness_packet",
        "release_manifest_candidate": artifact_meta["release_manifest_candidate.json"],
        "hf_distribution_handoff": artifact_meta["hf_distribution_handoff.json"],
        "storage_archive_handoff": artifact_meta["storage_archive_handoff.json"],
        "website_download_metadata_handoff": artifact_meta["website_download_metadata_handoff.json"],
        "recovery_material_leak_audit": artifact_meta["recovery_material_leak_audit.json"],
        "closed_execution": build_closed_execution(),
        "still_not_opened": [
            "actual app signing",
            "notarization submission",
            "Windows signing",
            "release upload",
            "updater execution",
            "website publication",
            "public download",
            "public launch",
            "HF mutation",
            "Storage Box mutation",
            "Vercel mutation",
            "real mining/pool/gateway/API",
            "broadcast",
            "payout/mint/settlement",
            "chain tx/runtime upgrade",
            "production mutation",
        ],
        "next_recommended_lane": "Phase42R server residual ops cleanup or Worker/Miner public package release lane, depending owner priority",
    }
    _write_json(out_dir / "phase40u_wallet_release_ops_summary.json", summary)
    artifact_meta["phase40u_wallet_release_ops_summary.json"] = {
        "path": str(out_dir / "phase40u_wallet_release_ops_summary.json"),
        "sha256": sha256_file(out_dir / "phase40u_wallet_release_ops_summary.json"),
    }

    md_path = out_dir / "PHASE40U_WALLET_RELEASE_OPS_2026-05-19.md"
    _write_markdown(
        md_path,
        source_commit=source_commit,
        app_version=app_version,
        artifact_meta=artifact_meta,
    )
    artifact_meta[md_path.name] = {"path": str(md_path), "sha256": sha256_file(md_path)}
    return artifact_meta


def main() -> None:
    parser = argparse.ArgumentParser(description="Write Phase40U release-ops readiness artifacts.")
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--app-version", required=True)
    parser.add_argument("--platform", default=DEFAULT_PLATFORM)
    args = parser.parse_args()
    artifacts = write_phase40u_artifacts(
        out_dir=args.out_dir,
        source_commit=args.source_commit,
        app_version=args.app_version,
        platform=args.platform,
    )
    print(json.dumps(artifacts, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
