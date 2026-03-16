"""Shared Azure bootstrap troubleshooting helpers."""

import json
import re
from pathlib import Path
from typing import Any

CATALOG_FILE = "azure_bootstrap_issue_catalog.json"
DEFAULT_IMAGE_REFERENCE = "ghcr.io/jbcupps/sao:latest"


def _catalog_paths() -> list[Path]:
    here = Path(__file__).resolve()
    return [
        Path("/app/shared") / CATALOG_FILE,
        here.parents[2] / "shared" / CATALOG_FILE,
    ]


def load_issue_catalog() -> dict[str, Any]:
    """Load the shared Azure bootstrap troubleshooting catalog."""
    for path in _catalog_paths():
        if path.exists():
            return json.loads(path.read_text(encoding="utf-8"))
    raise FileNotFoundError(
        "Azure bootstrap issue catalog not found. Expected installer/shared/"
        + CATALOG_FILE
    )


def _flatten_text(value: Any) -> str:
    """Convert nested JSON-like input into a lowercase searchable string."""
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    if isinstance(value, (list, tuple, set)):
        return " ".join(_flatten_text(item) for item in value)
    if isinstance(value, dict):
        parts: list[str] = []
        for key, item in value.items():
            parts.append(str(key))
            parts.append(_flatten_text(item))
        return " ".join(part for part in parts if part)
    return str(value)


def _normalize_context(request: dict[str, Any]) -> dict[str, str]:
    """Normalize troubleshooting request values for matching and rendering."""
    context = {
        "resource_group": str(request.get("resource_group") or "").strip(),
        "deployment_name": str(request.get("deployment_name") or "").strip(),
        "location": str(request.get("location") or "").strip(),
        "failed_resource_type": str(
            request.get("failed_resource_type") or ""
        ).strip(),
        "failed_resource_name": str(
            request.get("failed_resource_name") or ""
        ).strip(),
        "image_reference": str(
            request.get("image_reference") or DEFAULT_IMAGE_REFERENCE
        ).strip(),
        "container_app_name": str(
            request.get("container_app_name") or ""
        ).strip(),
        "postgres_server_name": str(
            request.get("postgres_server_name") or ""
        ).strip(),
        "revision": str(request.get("revision") or "").strip(),
        "runtime_startup_stage": str(
            request.get("runtime_startup_stage") or ""
        ).strip(),
        "host_os": str(request.get("host_os") or "windows").strip(),
        "issue_type_hint": str(request.get("issue_type_hint") or "").strip(),
        "raw_error": _flatten_text(request.get("raw_error")),
    }
    if not context["failed_resource_name"]:
        deleted_vault = request.get("deleted_vault") or {}
        if isinstance(deleted_vault, dict):
            context["failed_resource_name"] = str(
                deleted_vault.get("name") or ""
            ).strip()
    if not context["location"]:
        deleted_vault = request.get("deleted_vault") or {}
        if isinstance(deleted_vault, dict):
            context["location"] = str(
                deleted_vault.get("location") or ""
            ).strip()
    if not context["deployment_name"]:
        context["deployment_name"] = "sao-bootstrap"
    if not context["location"]:
        context["location"] = "eastus2"
    if (
        not context["container_app_name"]
        and "microsoft.app/containerapps"
        in context["failed_resource_type"].lower()
    ):
        context["container_app_name"] = (
            context["failed_resource_name"] or "sao-app"
        )
    return context


def _matches_rule(
    rule: dict[str, Any],
    searchable_text: str,
    resource_type: str,
    resource_name: str,
) -> bool:
    """Evaluate a catalog match rule against normalized failure evidence."""
    resource_type_checks = [
        token.lower() for token in rule.get("resource_type_contains", [])
    ]
    if resource_type_checks and not any(
        token in resource_type or token in searchable_text
        for token in resource_type_checks
    ):
        return False

    resource_name_checks = [
        token.lower() for token in rule.get("resource_name_contains", [])
    ]
    if resource_name_checks and not any(
        token in resource_name or token in searchable_text
        for token in resource_name_checks
    ):
        return False

    all_of = [token.lower() for token in rule.get("all_of", [])]
    if any(token not in searchable_text for token in all_of):
        return False

    any_of = [token.lower() for token in rule.get("any_of", [])]
    if any_of and not any(token in searchable_text for token in any_of):
        return False

    not_any_of = [token.lower() for token in rule.get("not_any_of", [])]
    if any(token in searchable_text for token in not_any_of):
        return False

    return True


def classify_issue(request: dict[str, Any]) -> dict[str, Any]:
    """Match a troubleshooting request against the shared issue catalog."""
    catalog = load_issue_catalog()
    context = _normalize_context(request)
    searchable_text = " ".join(
        part
        for part in [
            context["raw_error"],
            _flatten_text(request.get("evidence")),
            context["failed_resource_type"],
            context["failed_resource_name"],
            _flatten_text(request.get("top_level_error")),
            _flatten_text(request.get("nested_error")),
        ]
        if part
    ).lower()
    resource_type = context["failed_resource_type"].lower()
    resource_name = context["failed_resource_name"].lower()
    issue_type_hint = context["issue_type_hint"].lower()

    for issue in catalog.get("issues", []):
        issue_type = str(issue.get("issue_type") or "").lower()
        if issue_type_hint and issue_type_hint == issue_type:
            return issue
        if issue_type == "unknown":
            continue
        if _matches_rule(
            issue.get("match", {}),
            searchable_text=searchable_text,
            resource_type=resource_type,
            resource_name=resource_name,
        ):
            return issue

    for issue in catalog.get("issues", []):
        if issue.get("issue_type") == "unknown":
            return issue
    raise ValueError("Shared issue catalog is missing an unknown fallback.")


def render_command_template(template: str, request: dict[str, Any]) -> str:
    """Render catalog command templates using the troubleshooting context."""
    context = _normalize_context(request)

    def replace(match: re.Match[str]) -> str:
        key = match.group(1)
        return context.get(key, "")

    rendered = re.sub(r"\[\[([a-z_]+)\]\]", replace, template)
    return rendered.strip()


def build_troubleshooting_response(request: dict[str, Any]) -> dict[str, Any]:
    """Classify a bootstrap issue and return a structured response."""
    issue = classify_issue(request)
    evidence = [
        str(item).strip()
        for item in request.get("evidence", [])
        if str(item).strip()
    ]
    request_context = _normalize_context(request)
    manual_commands = [
        render_command_template(command, request_context)
        for command in issue.get("manual_commands", [])
        if render_command_template(command, request_context)
    ]
    return {
        "issue_type": issue.get("issue_type", "unknown"),
        "diagnosis": issue.get("diagnosis", ""),
        "evidence": evidence[:6],
        "guided_actions": list(issue.get("guided_actions", [])),
        "manual_commands": manual_commands,
        "safe_to_auto_apply": list(issue.get("safe_to_auto_apply", [])),
    }
