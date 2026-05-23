"""Phase50-C wallet signed-distribution readiness hardening.

This module is descriptor-only. It binds the current wallet safety branch to
the Phase50 release-engineering contract without signing, notarizing, uploading,
executing an updater, publishing downloads, or reading real wallet data.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

import release_ops


STATE = "phase50_c_closed_wallet_signed_distribution_readiness_hardening_default_off"
PHASE50_CONTRACT_ID = "alice.phase50-release-engineering-contract.v1"
PHASE40U_STATE = release_ops.STATE
APP_NAME = release_ops.APP_NAME
HF_BACKEND = release_ops.HF_BACKEND
STORAGE_BACKEND = release_ops.STORAGE_BACKEND
PLACEHOLDER = "PHASE50C_DESCRIPTOR_ONLY_PLACEHOLDER_NO_ARTIFACT"

FORBIDDEN_FIELD_NAMES = release_ops.FORBIDDEN_FIELD_NAMES | {
    "recovery_material",
    "wallet_seed",
    "signing_key",
    "notarization_credential",
    "authenticode_certificate",
}

FORBIDDEN_TRUE_FLAGS = {
    "signing_material_accessed",
    "actual_signing_performed",
    "notarization_submitted",
    "windows_signing_performed",
    "hf_upload_performed",
    "storage_box_write_performed",
    "vercel_mutation_performed",
    "updater_execution_performed",
    "website_publication_performed",
    "public_download_opened",
    "internal_test_opened",
    "public_release_opened",
    "production_mutation_performed",
    "real_wallet_data_read",
    "real_seed_imported",
    "real_private_key_exported",
    "real_transaction_broadcast",
}

FORBIDDEN_VALUE_FRAGMENTS = release_ops.FORBIDDEN_VALUE_FRAGMENTS + (
    "release upload completed",
    "public download enabled",
    "notarization submitted",
    "signed distribution live",
    "public launch approved",
)

LEAK_MARKERS = release_ops.LEAK_AUDIT_MARKERS + (
    "recovery_material=",
    "wallet_seed=",
    "signing_key=",
    "notarization_credential=",
    "authenticode_certificate=",
)


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


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


def closed_boundaries() -> dict[str, bool]:
    return {
        "signing_material_accessed": False,
        "actual_signing_performed": False,
        "notarization_submitted": False,
        "windows_signing_performed": False,
        "hf_upload_performed": False,
        "storage_box_write_performed": False,
        "vercel_mutation_performed": False,
        "updater_execution_performed": False,
        "website_publication_performed": False,
        "public_download_opened": False,
        "internal_test_opened": False,
        "public_release_opened": False,
        "production_mutation_performed": False,
        "real_wallet_data_read": False,
        "real_seed_imported": False,
        "real_private_key_exported": False,
        "real_transaction_broadcast": False,
    }


def validate_phase50c_packet(packet: dict[str, Any]) -> None:
    for path, value in _walk_packet(packet):
        key = path.rsplit(".", 1)[-1].lower()
        if key in FORBIDDEN_FIELD_NAMES:
            raise ValueError(f"forbidden field in Phase50-C packet: {path}")
        if key in FORBIDDEN_TRUE_FLAGS and value is not False:
            raise ValueError(f"forbidden Phase50-C execution flag is open: {path}")
        if isinstance(value, str):
            lowered = value.lower()
            for fragment in FORBIDDEN_VALUE_FRAGMENTS:
                if fragment.lower() in lowered:
                    raise ValueError(f"forbidden runtime/release value in Phase50-C packet: {path}")


def build_phase40u_reuse_evidence(
    *,
    phase40u_commit: str,
    current_wallet_commit: str,
    changed_since_phase40u: list[str],
) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.phase50c.phase40u_reuse_evidence.v1",
        "state": STATE,
        "phase40u_state": PHASE40U_STATE,
        "phase40u_commit": phase40u_commit,
        "current_wallet_commit": current_wallet_commit,
        "changed_since_phase40u": sorted(changed_since_phase40u),
        "release_ops_module_inherited": True,
        "current_safety_branch_is_successor": True,
        "rebuild_required_before_public_release": True,
        "reason": "Phase46-E only changes wallet safety surfaces after Phase40U; Phase50-C regenerates descriptors for current HEAD.",
    }
    validate_phase50c_packet(packet)
    return packet


def build_contract_binding(
    *,
    source_commit: str,
    app_version: str,
    phase40u_commit: str,
    changed_since_phase40u: list[str],
) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.phase50c.release_contract_binding.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "phase50_contract_id": PHASE50_CONTRACT_ID,
        "app_name": APP_NAME,
        "app_version": app_version,
        "source_commit": source_commit,
        "wallet_update_mode": "optional_prompt_with_signature_verification",
        "worker_miner_mandatory_update_not_applicable": True,
        "signed_manifest_required_before_release": True,
        "package_sha256_required_before_release": True,
        "package_size_required_before_release": True,
        "platform_channel_binding_required": True,
        "rollback_package_required_before_release": True,
        "redacted_update_logs_required": True,
        "public_distribution_backend": HF_BACKEND,
        "cold_archive_backend": STORAGE_BACKEND,
        "oss_or_aliyun_allowed": False,
        "phase40u_reuse_evidence": build_phase40u_reuse_evidence(
            phase40u_commit=phase40u_commit,
            current_wallet_commit=source_commit,
            changed_since_phase40u=changed_since_phase40u,
        ),
        "closed_boundaries": closed_boundaries(),
    }
    validate_phase50c_packet(packet)
    return packet


def build_signed_distribution_matrix(*, source_commit: str, app_version: str) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.phase50c.signed_distribution_matrix.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "platforms": [
            {
                "platform": "macos-arm64",
                "artifact_kind": "dmg_or_app_bundle_candidate",
                "signing_status": "not_started_descriptor_only",
                "notarization_status": "not_started_descriptor_only",
                "hardened_runtime_review_required": True,
                "entitlements_review_required": True,
                "gatekeeper_smoke_required": True,
            },
            {
                "platform": "windows-x64",
                "artifact_kind": "installer_candidate",
                "signing_status": "not_started_descriptor_only",
                "timestamping_status": "not_started_descriptor_only",
                "smart_screen_smoke_required": True,
                "windows_host_smoke_required": True,
            },
        ],
        "hf_distribution": {
            "backend": HF_BACKEND,
            "mode": "descriptor_only_no_upload",
            "artifact_ref": PLACEHOLDER,
            "artifact_sha256": PLACEHOLDER,
        },
        "cold_archive": {
            "backend": STORAGE_BACKEND,
            "mode": "descriptor_only_no_write",
            "artifact_ref": PLACEHOLDER,
            "artifact_sha256": PLACEHOLDER,
        },
        "closed_boundaries": closed_boundaries(),
    }
    validate_phase50c_packet(packet)
    return packet


def build_update_manifest_rehearsal(*, source_commit: str, app_version: str) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.phase50c.update_manifest_rehearsal.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "wallet_update_policy": "optional_user_prompt",
        "manifest_signature_required_before_release": True,
        "update_public_key_ref": "PHASE50C_PUBLIC_KEY_REF_PLACEHOLDER_NO_KEY_MATERIAL",
        "package_sha256": PLACEHOLDER,
        "package_size_bytes": 0,
        "platform_channel_binding_required": True,
        "bad_signature_fail_closed": True,
        "bad_hash_fail_closed": True,
        "rollback_metadata_required": True,
        "redacted_logs_required": True,
        "closed_boundaries": closed_boundaries(),
    }
    validate_phase50c_packet(packet)
    return packet


def build_rollback_rehearsal(*, source_commit: str, app_version: str) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.phase50c.rollback_rehearsal.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "rollback_package_required_before_release": True,
        "rollback_package_ref": PLACEHOLDER,
        "rollback_package_sha256": PLACEHOLDER,
        "old_evidence_reward_exclusion_not_applicable_to_wallet": True,
        "rollback_execution_performed": False,
        "destructive_cleanup_allowed": False,
        "closed_boundaries": closed_boundaries(),
    }
    validate_phase50c_packet(packet)
    return packet


def build_leak_recheck(paths: Iterable[Path]) -> dict[str, Any]:
    checked: list[str] = []
    matched: list[str] = []
    for path in paths:
        checked.append(path.name)
        text = path.read_text(encoding="utf-8", errors="replace").lower()
        if any(marker.lower() in text for marker in LEAK_MARKERS):
            matched.append(path.name)
    packet = {
        "schema": "alice.wallet.phase50c.release_leak_recheck.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "scan_policy": "filenames_and_counts_only_no_match_content",
        "files_checked": checked,
        "matches": len(matched),
        "files_with_matches": matched,
        "match_content_recorded": False,
        "result": "pass" if not matched else "review_required",
    }
    validate_phase50c_packet(packet)
    return packet


def build_static_scan_summary(packets: dict[str, dict[str, Any]]) -> dict[str, Any]:
    encoded = json.dumps(packets, sort_keys=True).lower()
    summary = {
        "schema": "alice.wallet.phase50c.static_scan_summary.v1",
        "state": STATE,
        "secret_or_signing_value_detected": any(
            marker in encoded
            for marker in (
                "seed=",
                "mnemonic=",
                "private_key=",
                "password=",
                "token=",
                "signing_key=",
                "notarization_credential=",
            )
        ),
        "raw_endpoint_detected": any(marker in encoded for marker in ("http://", "https://", "ws://", "wss://", "stratum")),
        "release_execution_opened": any(marker in encoded for marker in ("public_release_opened\":true", "actual_signing_performed\":true", "hf_upload_performed\":true")),
        "result": "pass",
    }
    if summary["secret_or_signing_value_detected"] or summary["raw_endpoint_detected"] or summary["release_execution_opened"]:
        summary["result"] = "review_required"
    validate_phase50c_packet(summary)
    return summary


def _write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _write_markdown(path: Path, *, source_commit: str, app_version: str, artifact_meta: dict[str, str]) -> None:
    lines = [
        "# Alice Phase50-C Wallet Signed-Distribution Readiness",
        "",
        f"State: `{STATE}`",
        f"Wallet source commit: `{source_commit}`",
        f"App version: `{app_version}`",
        f"Contract: `{PHASE50_CONTRACT_ID}`",
        "",
        "This packet is descriptor-only. It binds the current wallet safety branch to",
        "the Phase50 release-engineering contract without signing, notarizing, uploading,",
        "executing an updater, publishing downloads, or reading real wallet data.",
        "",
        "## Still Closed",
        "",
        "- Actual app signing.",
        "- Notarization submission.",
        "- Windows signing.",
        "- HF upload.",
        "- Storage Box write.",
        "- Vercel mutation.",
        "- Updater execution.",
        "- Website publication or public download.",
        "- Internal test and public release.",
        "- Real wallet data, recovery material, transaction broadcast, or production mutation.",
        "",
        "## Artifacts",
        "",
    ]
    for name, digest in sorted(artifact_meta.items()):
        lines.append(f"- `{name}` SHA-256 `{digest}`")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_phase50c_artifacts(
    *,
    out_dir: Path,
    source_commit: str,
    app_version: str,
    phase40u_commit: str,
    changed_since_phase40u: list[str],
) -> dict[str, str]:
    out_dir.mkdir(parents=True, exist_ok=True)
    payloads = {
        "phase50c_release_contract_binding.json": build_contract_binding(
            source_commit=source_commit,
            app_version=app_version,
            phase40u_commit=phase40u_commit,
            changed_since_phase40u=changed_since_phase40u,
        ),
        "phase50c_signed_distribution_matrix.json": build_signed_distribution_matrix(
            source_commit=source_commit,
            app_version=app_version,
        ),
        "phase50c_update_manifest_rehearsal.json": build_update_manifest_rehearsal(
            source_commit=source_commit,
            app_version=app_version,
        ),
        "phase50c_rollback_rehearsal.json": build_rollback_rehearsal(
            source_commit=source_commit,
            app_version=app_version,
        ),
    }
    for name, payload in payloads.items():
        _write_json(out_dir / name, payload)

    leak_audit = build_leak_recheck(out_dir / name for name in payloads)
    _write_json(out_dir / "phase50c_release_leak_recheck.json", leak_audit)

    scan_inputs = {name: json.loads((out_dir / name).read_text(encoding="utf-8")) for name in payloads}
    scan_inputs["phase50c_release_leak_recheck.json"] = leak_audit
    static_scan = build_static_scan_summary(scan_inputs)
    _write_json(out_dir / "phase50c_static_scan_summary.json", static_scan)

    artifact_meta: dict[str, str] = {}
    for path in sorted(out_dir.glob("*.json")):
        artifact_meta[path.name] = sha256_file(path)

    summary = {
        "schema": "alice.wallet.phase50c.release_readiness_summary.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "phase50_contract_id": PHASE50_CONTRACT_ID,
        "phase40u_commit": phase40u_commit,
        "changed_since_phase40u": sorted(changed_since_phase40u),
        "artifact_sha256": artifact_meta,
        "closed_boundaries": closed_boundaries(),
        "next_lane": "Phase50-D - Signed Updater Fail-Closed Implementation / Preflight",
    }
    validate_phase50c_packet(summary)
    _write_json(out_dir / "phase50c_wallet_release_readiness_summary.json", summary)
    artifact_meta["phase50c_wallet_release_readiness_summary.json"] = sha256_file(
        out_dir / "phase50c_wallet_release_readiness_summary.json"
    )

    md_path = out_dir / "PHASE50C_WALLET_SIGNED_DISTRIBUTION_READINESS.md"
    _write_markdown(
        md_path,
        source_commit=source_commit,
        app_version=app_version,
        artifact_meta=artifact_meta,
    )
    artifact_meta[md_path.name] = sha256_file(md_path)
    return artifact_meta


def main() -> None:
    parser = argparse.ArgumentParser(description="Write Phase50-C wallet release-readiness artifacts.")
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--app-version", required=True)
    parser.add_argument("--phase40u-commit", required=True)
    parser.add_argument("--changed-since-phase40u", nargs="*", default=[])
    args = parser.parse_args()
    artifacts = write_phase50c_artifacts(
        out_dir=args.out_dir,
        source_commit=args.source_commit,
        app_version=args.app_version,
        phase40u_commit=args.phase40u_commit,
        changed_since_phase40u=args.changed_since_phase40u,
    )
    print(json.dumps(artifacts, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
