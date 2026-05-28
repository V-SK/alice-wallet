"""L6 public-client release signing readiness descriptors.

This module prepares and validates release-signing readiness metadata only. It
does not discover certificates, load keychains, sign binaries, submit
notarization, staple tickets, upload to HF, publish releases, or authorize public
distribution.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

import release_ops


STATE = "l6_public_client_release_signing_readiness_descriptor_only_fail_closed"
APP_NAME = release_ops.APP_NAME
HF_BACKEND = release_ops.HF_BACKEND
STORAGE_BACKEND = release_ops.STORAGE_BACKEND
PLACEHOLDER = "L6_DESCRIPTOR_ONLY_PLACEHOLDER_NO_ARTIFACT"
ALLOWED_CHANNELS = {"alpha", "beta", "stable"}
SIGNED_MANIFEST_SCHEMA = "alice.wallet.l6.signed_release_manifest.v1"
CREDENTIAL_STATUS_SCHEMA = "alice.wallet.l6.credential_status.v1"
PACKAGE_SIGNING_EVIDENCE_SCHEMA = "alice.wallet.l6.package_signing_evidence.v1"

FORBIDDEN_FIELD_NAMES = release_ops.FORBIDDEN_FIELD_NAMES | {
    "apple_id",
    "apple_password",
    "asc_api_key",
    "asc_issuer_id",
    "p12_password",
    "certificate_password",
    "certificate_private_key",
    "notary_password",
    "notary_secret",
    "signtool_password",
    "release_private_key",
    "client_secret",
}

FORBIDDEN_TRUE_FLAGS = {
    "actual_certificate_material_loaded",
    "actual_notary_secret_loaded",
    "actual_private_key_loaded",
    "macos_signing_executed",
    "macos_installer_signing_executed",
    "notarization_submitted",
    "stapling_executed",
    "gatekeeper_public_release_passed",
    "windows_authenticode_executed",
    "timestamping_executed",
    "manifest_signature_executed",
    "hf_upload_performed",
    "storage_box_write_performed",
    "website_publication_performed",
    "public_download_opened",
    "public_release_opened",
    "release_execution_allowed",
    "credential_material_included",
    "private_key_material_included",
    "secret_material_included",
}

FORBIDDEN_VALUE_FRAGMENTS = release_ops.FORBIDDEN_VALUE_FRAGMENTS + (
    "public launch approved",
    "public release approved",
    "release upload completed",
    "notarization submitted",
    "signed distribution live",
    "apple app-specific password",
    "private key value",
)

REQUIRED_CREDENTIAL_REFS = {
    ("macos", "developer_id_application_ref"): "missing_macos_developer_id_application_ref",
    ("macos", "developer_id_installer_ref"): "missing_macos_developer_id_installer_ref",
    ("macos", "notary_profile_ref"): "missing_macos_notary_profile_ref",
    ("windows", "authenticode_certificate_ref"): "missing_windows_authenticode_certificate_ref",
    ("windows", "timestamp_service_ref"): "missing_windows_timestamp_service_ref",
    ("manifest", "release_manifest_key_ref"): "missing_release_manifest_key_ref",
}

REQUIRED_PACKAGE_EVIDENCE_CHECKS = {
    ("macos-arm64", "dmg"): {
        "codesign_verify_strict",
        "codesign_entitlements_dump",
        "hardened_runtime_review",
        "notarization_accepted",
        "stapler_validate",
        "gatekeeper_assess_execute",
    },
    ("macos-arm64", "pkg"): {
        "codesign_verify_strict",
        "codesign_entitlements_dump",
        "gatekeeper_assess_execute",
        "hardened_runtime_review",
        "pkgutil_check_signature",
        "spctl_assess_install",
        "notarization_accepted",
        "stapler_validate",
    },
    ("windows-x64", "exe"): {
        "signtool_verify_pa_tw",
        "powershell_authenticode_status",
        "timestamp_verified",
        "publisher_identity_review",
    },
    ("windows-x64", "installer"): {
        "signtool_verify_pa_tw",
        "powershell_authenticode_status",
        "timestamp_verified",
        "publisher_identity_review",
    },
}

EVIDENCE_CHECK_PLANS = {
    "codesign_verify_strict": "codesign --verify --strict --deep --verbose=4 AliceWallet.app",
    "codesign_entitlements_dump": "codesign -dvvv --entitlements :- AliceWallet.app",
    "gatekeeper_assess_execute": "spctl --assess --type execute --verbose=4 AliceWallet.app",
    "hardened_runtime_review": "confirm hardened runtime is present in codesign details",
    "notarization_accepted": "confirm notarytool result was Accepted for the shipped artifact",
    "pkgutil_check_signature": "pkgutil --check-signature AliceWallet.pkg",
    "spctl_assess_install": "spctl --assess --type install --verbose=4 AliceWallet.pkg",
    "stapler_validate": "xcrun stapler validate shipped macOS artifact",
    "signtool_verify_pa_tw": "signtool verify /pa /tw /v shipped Windows artifact",
    "powershell_authenticode_status": "Get-AuthenticodeSignature shows Status Valid for shipped Windows artifact",
    "timestamp_verified": "signtool /tw timestamp verification passes for shipped Windows artifact",
    "publisher_identity_review": "owner verifies expected publisher identity from Authenticode output",
}


class L6ReleaseSigningReadinessError(ValueError):
    """Raised when descriptor-only release metadata is unsafe or incomplete."""


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


def _is_present_ref(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    stripped = value.strip()
    return bool(stripped) and PLACEHOLDER not in stripped and stripped.upper() != "NONE"


def _is_sha256(value: Any) -> bool:
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def _evidence_check_ids_for(package: dict[str, Any]) -> set[str]:
    key = (str(package.get("platform")), str(package.get("artifact_kind")))
    return REQUIRED_PACKAGE_EVIDENCE_CHECKS.get(key, set())


def build_package_signing_evidence_template(*, platform: str, artifact_kind: str) -> dict[str, Any]:
    required_checks = REQUIRED_PACKAGE_EVIDENCE_CHECKS.get((platform, artifact_kind), set())
    checks = [
        {
            "id": check_id,
            "required": True,
            "passed": False,
            "operator_validation": EVIDENCE_CHECK_PLANS[check_id],
            "evidence_log_sha256": PLACEHOLDER,
        }
        for check_id in sorted(required_checks)
    ]
    return {
        "schema": PACKAGE_SIGNING_EVIDENCE_SCHEMA,
        "evidence_state": "missing_real_owner_signing_evidence",
        "operator_evidence_ref": PLACEHOLDER,
        "credential_material_included": False,
        "private_key_material_included": False,
        "secret_material_included": False,
        "verification_checks": checks,
    }


def _validate_package_signing_evidence(
    package: dict[str, Any],
    *,
    index: int,
    require_release_ready: bool,
) -> None:
    evidence = package.get("signing_evidence")
    if not isinstance(evidence, dict):
        raise L6ReleaseSigningReadinessError(f"missing_package_signing_evidence:{index}")
    if evidence.get("schema") != PACKAGE_SIGNING_EVIDENCE_SCHEMA:
        raise L6ReleaseSigningReadinessError(f"invalid_package_signing_evidence_schema:{index}")

    checks = evidence.get("verification_checks")
    if not isinstance(checks, list) or not checks:
        raise L6ReleaseSigningReadinessError(f"missing_package_signing_evidence_checks:{index}")

    by_id: dict[str, dict[str, Any]] = {}
    for check in checks:
        if not isinstance(check, dict) or not isinstance(check.get("id"), str):
            raise L6ReleaseSigningReadinessError(f"invalid_package_signing_evidence_check:{index}")
        check_id = check["id"]
        if check_id in by_id:
            raise L6ReleaseSigningReadinessError(f"duplicate_package_signing_evidence_check:{index}:{check_id}")
        by_id[check_id] = check

    expected_checks = _evidence_check_ids_for(package)
    if not expected_checks:
        raise L6ReleaseSigningReadinessError(f"unsupported_package_signing_evidence_target:{index}")
    missing = sorted(expected_checks - set(by_id))
    if missing:
        raise L6ReleaseSigningReadinessError(f"missing_package_signing_evidence_check:{index}:{','.join(missing)}")

    if require_release_ready:
        if not _is_present_ref(evidence.get("operator_evidence_ref")):
            raise L6ReleaseSigningReadinessError(f"missing_package_signing_evidence_ref:{index}")
        for check_id in sorted(expected_checks):
            check = by_id[check_id]
            if check.get("required") is not True:
                raise L6ReleaseSigningReadinessError(f"package_signing_evidence_check_not_required:{index}:{check_id}")
            if check.get("passed") is not True:
                raise L6ReleaseSigningReadinessError(f"package_signing_evidence_check_not_passed:{index}:{check_id}")
            if not _is_sha256(check.get("evidence_log_sha256")):
                raise L6ReleaseSigningReadinessError(f"package_signing_evidence_log_sha_invalid:{index}:{check_id}")


def closed_boundaries() -> dict[str, bool]:
    return {
        "actual_certificate_material_loaded": False,
        "actual_notary_secret_loaded": False,
        "actual_private_key_loaded": False,
        "macos_signing_executed": False,
        "macos_installer_signing_executed": False,
        "notarization_submitted": False,
        "stapling_executed": False,
        "gatekeeper_public_release_passed": False,
        "windows_authenticode_executed": False,
        "timestamping_executed": False,
        "manifest_signature_executed": False,
        "hf_upload_performed": False,
        "storage_box_write_performed": False,
        "website_publication_performed": False,
        "public_download_opened": False,
        "public_release_opened": False,
        "release_execution_allowed": False,
    }


def validate_l6_packet(packet: dict[str, Any]) -> None:
    for path, value in _walk_packet(packet):
        key = path.rsplit(".", 1)[-1].lower()
        if key in FORBIDDEN_FIELD_NAMES:
            raise L6ReleaseSigningReadinessError(f"forbidden credential field in L6 packet: {path}")
        if key in FORBIDDEN_TRUE_FLAGS and value is not False:
            raise L6ReleaseSigningReadinessError(f"forbidden release execution flag is open: {path}")
        if isinstance(value, str):
            lowered = value.lower()
            for fragment in FORBIDDEN_VALUE_FRAGMENTS:
                if fragment.lower() in lowered:
                    raise L6ReleaseSigningReadinessError(f"forbidden runtime or release value in L6 packet: {path}")


def build_distribution_goal(*, source_commit: str, app_version: str, channel: str) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.l6.distribution_goal.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "distribution_goal": "public_client_release_signing_readiness_without_execution",
        "app_name": APP_NAME,
        "app_version": app_version,
        "channel": channel,
        "source_commit": source_commit,
        "actual_exported_artifact_inspected": False,
        "settings_inspected": [
            ".github/workflows/release.yml",
            "gui/Cargo.toml",
            "phase50c_release_readiness.py",
            "release_ops.py",
        ],
        "inference_note": (
            "No exported public artifacts are inspected here; this packet validates "
            "readiness settings and required metadata shape only."
        ),
        "public_release_blocker": (
            "No public release until packages are signed, notarized where applicable, "
            "hashes are frozen, and HF metadata is approved."
        ),
        "closed_boundaries": closed_boundaries(),
    }
    validate_l6_packet(packet)
    return packet


def build_macos_signing_readiness(
    *,
    source_commit: str,
    app_version: str,
    channel: str,
) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.l6.macos_signing_readiness.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "channel": channel,
        "bundle_identifier": "org.aliceprotocol.wallet",
        "distribution_goal": "Developer ID public distribution readiness",
        "developer_id_application": {
            "required": True,
            "identity_ref_required": True,
            "real_identity_loaded": False,
            "signing_executed": False,
            "print_only_plan": (
                "codesign --force --options runtime --timestamp --entitlements "
                'release/macos/AliceWallet.entitlements --sign "Developer ID '
                'Application: [TEAM_NAME] ([TEAM_ID])" AliceWallet.app'
            ),
        },
        "developer_id_installer": {
            "required_for_pkg": True,
            "identity_ref_required": True,
            "real_identity_loaded": False,
            "signing_executed": False,
            "print_only_plan": (
                'productsign --sign "Developer ID Installer: [TEAM_NAME] '
                '([TEAM_ID])" AliceWallet.pkg AliceWallet-signed.pkg'
            ),
        },
        "hardened_runtime": {
            "required": True,
            "codesign_option": "--options runtime",
            "current_workflow_configured": False,
            "validation_required_before_release": True,
        },
        "entitlements": {
            "required": True,
            "plist_ref": "release/macos/AliceWallet.entitlements",
            "review_status": "missing_or_unverified",
            "must_not_enable_get_task_allow": True,
            "must_justify_network_or_file_entitlements": True,
        },
        "notarization": {
            "required_for_public_distribution": True,
            "notary_profile_ref_required": True,
            "submission_executed": False,
            "print_only_plan": "xcrun notarytool submit AliceWallet.dmg --keychain-profile [NOTARY_PROFILE_REF] --wait",
        },
        "stapling": {
            "required_after_accepted_notarization": True,
            "stapling_executed": False,
            "print_only_plan": "xcrun stapler staple AliceWallet.dmg",
        },
        "validation_commands": [
            "codesign --verify --strict --deep --verbose=4 AliceWallet.app",
            "codesign -dvvv --entitlements :- AliceWallet.app",
            "spctl --assess --type execute --verbose=4 AliceWallet.app",
            "xcrun stapler validate AliceWallet.dmg",
            "pkgutil --check-signature AliceWallet.pkg",
            "spctl --assess --type install --verbose=4 AliceWallet.pkg",
        ],
        "closed_boundaries": closed_boundaries(),
    }
    validate_l6_packet(packet)
    return packet


def build_windows_signing_readiness(
    *,
    source_commit: str,
    app_version: str,
    channel: str,
) -> dict[str, Any]:
    packet = {
        "schema": "alice.wallet.l6.windows_signing_readiness.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "channel": channel,
        "distribution_goal": "Windows Authenticode public distribution readiness",
        "authenticode": {
            "required": True,
            "certificate_ref_required": True,
            "certificate_material_loaded": False,
            "signing_executed": False,
            "print_only_plan_exe": (
                "signtool sign /fd SHA256 /tr [RFC3161_TIMESTAMP_URL_REF] "
                "/td SHA256 /sha1 [CERT_THUMBPRINT_REF] AliceWallet.exe"
            ),
            "print_only_plan_installer": (
                "signtool sign /fd SHA256 /tr [RFC3161_TIMESTAMP_URL_REF] "
                "/td SHA256 /sha1 [CERT_THUMBPRINT_REF] AliceWalletSetup.exe"
            ),
        },
        "timestamping": {
            "required": True,
            "timestamp_service_ref_required": True,
            "timestamping_executed": False,
            "timestamp_verification_required": True,
        },
        "verification": {
            "installer_and_exe_required": True,
            "validation_commands": [
                "signtool verify /pa /tw /v AliceWallet.exe",
                "signtool verify /pa /tw /v AliceWalletSetup.exe",
                "powershell -NoProfile -Command \"Get-AuthenticodeSignature .\\AliceWallet.exe | Format-List\"",
                "powershell -NoProfile -Command \"Get-AuthenticodeSignature .\\AliceWalletSetup.exe | Format-List\"",
            ],
        },
        "smart_screen": {
            "residual_risk": "SmartScreen reputation can still warn after valid Authenticode signing and timestamping.",
            "owner_acceptance_required": True,
            "not_a_substitute_for_signature_validation": True,
        },
        "closed_boundaries": closed_boundaries(),
    }
    validate_l6_packet(packet)
    return packet


def build_signed_manifest_template(*, source_commit: str, app_version: str, channel: str) -> dict[str, Any]:
    package_templates = [
        ("macos-arm64", "dmg", "AliceWallet-macos-arm64.dmg"),
        ("macos-arm64", "pkg", "AliceWallet-macos-arm64.pkg"),
        ("windows-x64", "exe", "AliceWallet-windows-x64.exe"),
        ("windows-x64", "installer", "AliceWalletSetup-windows-x64.exe"),
    ]
    packages = [
        {
            "platform": platform,
            "artifact_kind": artifact_kind,
            "file_name": file_name,
            "version": app_version,
            "channel": channel,
            "sha256": PLACEHOLDER,
            "size_bytes": 0,
            "hash_frozen": False,
            "hf_backend": HF_BACKEND,
            "hf_repo": "ALICE_WALLET_HF_REPO_REF",
            "hf_path": f"wallet/{channel}/{app_version}/{file_name}",
            "hf_metadata_approved": False,
            "signing_evidence": build_package_signing_evidence_template(
                platform=platform,
                artifact_kind=artifact_kind,
            ),
        }
        for platform, artifact_kind, file_name in package_templates
    ]
    manifest = {
        "schema": SIGNED_MANIFEST_SCHEMA,
        "state": STATE,
        "generated_at_utc": utc_now(),
        "app_name": APP_NAME,
        "source_commit": source_commit,
        "app_version": app_version,
        "channel": channel,
        "public_distribution_backend": HF_BACKEND,
        "oss_or_aliyun_allowed": False,
        "hashes_frozen": False,
        "hf_metadata_approved": False,
        "manifest_signature": {
            "signature_required": True,
            "signature_present": False,
            "signature_algorithm": "ed25519-release-manifest-public-key",
            "signature_ref": PLACEHOLDER,
            "credential_material_loaded": False,
            "verification_required_before_release": True,
        },
        "packages": packages,
        "public_release_blocker": (
            "No public release until packages are signed, notarized where applicable, "
            "hashes are frozen, and HF metadata is approved."
        ),
        "closed_boundaries": closed_boundaries(),
    }
    validate_signed_manifest_metadata(manifest, require_release_ready=False)
    return manifest


def validate_signed_manifest_metadata(manifest: dict[str, Any], *, require_release_ready: bool = False) -> None:
    validate_l6_packet(manifest)
    if manifest.get("schema") != SIGNED_MANIFEST_SCHEMA:
        raise L6ReleaseSigningReadinessError("invalid_signed_manifest_schema")
    if manifest.get("app_name") != APP_NAME:
        raise L6ReleaseSigningReadinessError("invalid_app_name")
    app_version = manifest.get("app_version")
    if not isinstance(app_version, str) or not app_version.strip():
        raise L6ReleaseSigningReadinessError("missing_app_version")
    channel = manifest.get("channel")
    if channel not in ALLOWED_CHANNELS:
        raise L6ReleaseSigningReadinessError("invalid_release_channel")
    if manifest.get("public_distribution_backend") != HF_BACKEND:
        raise L6ReleaseSigningReadinessError("public_distribution_backend_must_be_HF")
    if manifest.get("oss_or_aliyun_allowed") is not False:
        raise L6ReleaseSigningReadinessError("oss_or_aliyun_must_remain_disabled")

    signature = manifest.get("manifest_signature")
    if not isinstance(signature, dict):
        raise L6ReleaseSigningReadinessError("missing_manifest_signature_block")
    if signature.get("signature_required") is not True:
        raise L6ReleaseSigningReadinessError("manifest_signature_must_be_required")
    if signature.get("credential_material_loaded") is not False:
        raise L6ReleaseSigningReadinessError("manifest_validator_must_not_load_private_key_material")
    if require_release_ready and signature.get("signature_present") is not True:
        raise L6ReleaseSigningReadinessError("manifest_signature_missing")

    packages = manifest.get("packages")
    if not isinstance(packages, list) or not packages:
        raise L6ReleaseSigningReadinessError("missing_manifest_packages")
    for index, package in enumerate(packages):
        if not isinstance(package, dict):
            raise L6ReleaseSigningReadinessError(f"invalid_package_entry:{index}")
        for field in (
            "platform",
            "artifact_kind",
            "file_name",
            "version",
            "channel",
            "sha256",
            "size_bytes",
            "hash_frozen",
            "hf_backend",
            "hf_repo",
            "hf_path",
            "hf_metadata_approved",
            "signing_evidence",
        ):
            if field not in package:
                raise L6ReleaseSigningReadinessError(f"missing_package_field:{index}:{field}")
        if package["version"] != app_version:
            raise L6ReleaseSigningReadinessError(f"package_version_mismatch:{index}")
        if package["channel"] != channel:
            raise L6ReleaseSigningReadinessError(f"package_channel_mismatch:{index}")
        if package["hf_backend"] != HF_BACKEND:
            raise L6ReleaseSigningReadinessError(f"package_hf_backend_must_be_HF:{index}")
        if require_release_ready or package["hash_frozen"] is True:
            if package["hash_frozen"] is not True:
                raise L6ReleaseSigningReadinessError(f"package_hash_not_frozen:{index}")
            if not _is_sha256(package["sha256"]):
                raise L6ReleaseSigningReadinessError(f"package_sha256_invalid:{index}")
            if not isinstance(package["size_bytes"], int) or package["size_bytes"] <= 0:
                raise L6ReleaseSigningReadinessError(f"package_size_invalid:{index}")
        if require_release_ready and package["hf_metadata_approved"] is not True:
            raise L6ReleaseSigningReadinessError(f"package_hf_metadata_not_approved:{index}")
        _validate_package_signing_evidence(
            package,
            index=index,
            require_release_ready=require_release_ready,
        )

    if require_release_ready:
        if manifest.get("hashes_frozen") is not True:
            raise L6ReleaseSigningReadinessError("manifest_hashes_not_frozen")
        if manifest.get("hf_metadata_approved") is not True:
            raise L6ReleaseSigningReadinessError("manifest_hf_metadata_not_approved")


def evaluate_public_release_gate(
    *,
    manifest: dict[str, Any],
    credential_status: dict[str, Any],
) -> dict[str, Any]:
    blockers: list[str] = []
    try:
        validate_signed_manifest_metadata(manifest, require_release_ready=True)
    except L6ReleaseSigningReadinessError as exc:
        blockers.append(str(exc))

    validate_l6_packet(credential_status)
    if credential_status.get("schema") != CREDENTIAL_STATUS_SCHEMA:
        blockers.append("invalid_credential_status_schema")
    for (section, key), blocker in sorted(REQUIRED_CREDENTIAL_REFS.items(), key=lambda item: item[1]):
        section_data = credential_status.get(section)
        if not isinstance(section_data, dict) or not _is_present_ref(section_data.get(key)):
            blockers.append(blocker)

    for path, value in _walk_packet(credential_status):
        key = path.rsplit(".", 1)[-1].lower()
        if key in {
            "actual_certificate_material_loaded",
            "actual_notary_secret_loaded",
            "actual_private_key_loaded",
        } and value is not False:
            blockers.append(f"readiness_validator_must_not_load_secret_material:{path}")

    result = {
        "schema": "alice.wallet.l6.public_release_gate_evaluation.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "metadata_ready": not blockers,
        "blockers": blockers,
        "release_execution_allowed": False,
        "release_execution_authority": "not_granted_by_this_descriptor",
        "public_release_blocker": (
            "No public release until packages are signed, notarized where applicable, "
            "hashes are frozen, and HF metadata is approved."
        ),
        "closed_boundaries": closed_boundaries(),
    }
    validate_l6_packet(result)
    return result


def build_owner_signing_checklist(
    *,
    source_commit: str,
    app_version: str,
    channel: str,
) -> dict[str, Any]:
    items = [
        {
            "id": "macos_developer_id_application_ref",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": "Developer ID Application identity reference, not certificate material.",
        },
        {
            "id": "macos_developer_id_installer_ref",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": "Developer ID Installer identity reference if pkg is shipped.",
        },
        {
            "id": "macos_notary_profile_ref",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": "notarytool keychain-profile reference only.",
        },
        {
            "id": "windows_authenticode_certificate_ref",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": "Authenticode certificate thumbprint or HSM reference only.",
        },
        {
            "id": "windows_timestamp_service_ref",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": "RFC3161 timestamp service reference.",
        },
        {
            "id": "signed_manifest_key_ref",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": (
                "release manifest public key reference and signature reference, "
                "not private key material."
            ),
        },
        {
            "id": "package_signing_evidence_bundle",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": "per-package verification checks with SHA-256 of redacted validation logs.",
        },
        {
            "id": "hf_metadata_owner_approval",
            "required_before_public_release": True,
            "complete": False,
            "evidence_required": "HF repo and path approval for each package; OSS remains disabled.",
        },
    ]
    packet = {
        "schema": "alice.wallet.l6.owner_signing_checklist.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "channel": channel,
        "descriptor_ready_for_owner_review": True,
        "owner_signing_environment_ready": False,
        "public_release_ready": False,
        "items": items,
        "fail_closed_reason": (
            "real artifacts, credential refs, package evidence, signed manifest, "
            "and HF approval are not present in this descriptor packet"
        ),
        "closed_boundaries": closed_boundaries(),
    }
    validate_l6_packet(packet)
    return packet


def _write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _write_markdown(
    path: Path,
    *,
    source_commit: str,
    app_version: str,
    channel: str,
    artifact_meta: dict[str, str],
) -> None:
    lines = [
        "# Alice L6 Public Client Release Signing Readiness",
        "",
        f"State: `{STATE}`",
        f"Source commit: `{source_commit}`",
        f"App version: `{app_version}`",
        f"Channel: `{channel}`",
        "",
        "This packet is descriptor-only. It inspects release metadata shape and",
        "records the commands that must be run by an operator in the real signing",
        "environment, but it does not sign, notarize, staple, upload, publish, or",
        "load credentials.",
        "",
        "No public release is allowed until packages are signed, notarized where",
        "applicable, hashes are frozen, and HF metadata is approved.",
        "",
        "The owner checklist and per-package signing evidence format are included",
        "so a real signing environment can attach redacted verification logs later.",
        "This packet remains fail-closed until those references and hashes exist.",
        "",
        "## Artifact Hashes",
        "",
    ]
    for name, digest in sorted(artifact_meta.items()):
        lines.append(f"- `{name}` SHA-256 `{digest}`")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_l6_artifacts(
    *,
    out_dir: Path,
    source_commit: str,
    app_version: str,
    channel: str,
) -> dict[str, str]:
    out_dir.mkdir(parents=True, exist_ok=True)
    manifest = build_signed_manifest_template(
        source_commit=source_commit,
        app_version=app_version,
        channel=channel,
    )
    missing_credentials = {
        "schema": CREDENTIAL_STATUS_SCHEMA,
        "state": STATE,
        "macos": {
            "developer_id_application_ref": None,
            "developer_id_installer_ref": None,
            "notary_profile_ref": None,
            "actual_certificate_material_loaded": False,
            "actual_notary_secret_loaded": False,
        },
        "windows": {
            "authenticode_certificate_ref": None,
            "timestamp_service_ref": None,
            "actual_certificate_material_loaded": False,
        },
        "manifest": {
            "release_manifest_key_ref": None,
            "actual_private_key_loaded": False,
        },
        "closed_boundaries": closed_boundaries(),
    }
    gate = evaluate_public_release_gate(
        manifest=manifest,
        credential_status=missing_credentials,
    )
    payloads = {
        "l6_distribution_goal.json": build_distribution_goal(
            source_commit=source_commit,
            app_version=app_version,
            channel=channel,
        ),
        "l6_macos_signing_readiness.json": build_macos_signing_readiness(
            source_commit=source_commit,
            app_version=app_version,
            channel=channel,
        ),
        "l6_windows_signing_readiness.json": build_windows_signing_readiness(
            source_commit=source_commit,
            app_version=app_version,
            channel=channel,
        ),
        "l6_signed_manifest_template.json": manifest,
        "l6_missing_credentials_fail_closed.json": missing_credentials,
        "l6_public_release_gate_fail_closed.json": gate,
        "l6_owner_signing_checklist.json": build_owner_signing_checklist(
            source_commit=source_commit,
            app_version=app_version,
            channel=channel,
        ),
    }
    for name, payload in payloads.items():
        _write_json(out_dir / name, payload)

    artifact_meta: dict[str, str] = {}
    for path in sorted(out_dir.glob("*.json")):
        artifact_meta[path.name] = sha256_file(path)

    summary = {
        "schema": "alice.wallet.l6.release_signing_readiness_summary.v1",
        "state": STATE,
        "generated_at_utc": utc_now(),
        "source_commit": source_commit,
        "app_version": app_version,
        "channel": channel,
        "artifact_sha256": artifact_meta,
        "descriptor_ready_for_owner_review": True,
        "owner_signing_environment_ready": False,
        "metadata_ready": False,
        "blockers": gate["blockers"],
        "release_execution_allowed": False,
        "closed_boundaries": closed_boundaries(),
    }
    validate_l6_packet(summary)
    _write_json(out_dir / "l6_release_signing_readiness_summary.json", summary)
    artifact_meta["l6_release_signing_readiness_summary.json"] = sha256_file(
        out_dir / "l6_release_signing_readiness_summary.json"
    )

    md_path = out_dir / "L6_PUBLIC_CLIENT_RELEASE_SIGNING_READINESS.md"
    _write_markdown(
        md_path,
        source_commit=source_commit,
        app_version=app_version,
        channel=channel,
        artifact_meta=artifact_meta,
    )
    artifact_meta[md_path.name] = sha256_file(md_path)
    return artifact_meta


def _read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser(description="Write or validate L6 public release signing readiness metadata.")
    parser.add_argument("--out-dir", type=Path)
    parser.add_argument("--source-commit")
    parser.add_argument("--app-version")
    parser.add_argument("--channel", default="stable")
    parser.add_argument("--validate-manifest", type=Path)
    parser.add_argument("--credential-status", type=Path)
    parser.add_argument("--require-release-ready", action="store_true")
    args = parser.parse_args()

    if args.validate_manifest:
        manifest = _read_json(args.validate_manifest)
        if args.credential_status:
            result = evaluate_public_release_gate(
                manifest=manifest,
                credential_status=_read_json(args.credential_status),
            )
            print(json.dumps(result, indent=2, sort_keys=True))
            if args.require_release_ready and not result["metadata_ready"]:
                raise SystemExit(2)
            return
        validate_signed_manifest_metadata(
            manifest,
            require_release_ready=args.require_release_ready,
        )
        print(json.dumps({"result": "pass"}, indent=2, sort_keys=True))
        return

    if not args.out_dir or not args.source_commit or not args.app_version:
        parser.error("--out-dir, --source-commit, and --app-version are required unless validating a manifest")

    artifacts = write_l6_artifacts(
        out_dir=args.out_dir,
        source_commit=args.source_commit,
        app_version=args.app_version,
        channel=args.channel,
    )
    print(json.dumps(artifacts, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
