"""Azure CLI wrappers for the SAO installer agent."""

import json
import os
import re
import shlex
import subprocess
from typing import Any

HOST_OS = os.environ.get("HOST_OS", "windows" if os.name == "nt" else "linux")
AZURE_CLI_PATHS = ("/usr/bin/az", "/usr/local/bin/az")
DEFAULT_DEPLOYMENT_NAME = "sao-bootstrap"
DEFAULT_CONTAINER_APP_NAME = "sao-app"
DEFAULT_IMAGE_TAG = "latest"
_CONTROL_CHARACTERS = re.compile(r"[\r\n\x00]")
_READ_ONLY_PREFIXES_2 = {
    ("account", "show"),
    ("account", "list"),
    ("containerapp", "show"),
    ("group", "show"),
    ("keyvault", "list-deleted"),
    ("keyvault", "show"),
    ("provider", "show"),
    ("resource", "list"),
}
_READ_ONLY_PREFIXES_3 = {
    ("ad", "signed-in-user", "show"),
    ("containerapp", "logs", "show"),
    ("containerapp", "replica", "list"),
    ("containerapp", "revision", "list"),
    ("deployment", "group", "show"),
    ("role", "assignment", "list"),
}
_READ_ONLY_PREFIXES_4 = {
    ("deployment", "operation", "group", "list"),
    ("deployment", "operation", "group", "show"),
}


def _normalize_host_os(host_os: str | None = None) -> str:
    """Normalize host OS names to the display modes we support."""
    value = (host_os or HOST_OS).strip().lower()
    if value.startswith("win"):
        return "windows"
    return "linux"


def _quote_for_powershell(arg: str) -> str:
    """Render a command argument in a PowerShell-friendly form."""
    if not arg:
        return "''"
    if re.fullmatch(r"[-\w./:=]+", arg):
        return arg
    return "'" + arg.replace("'", "''") + "'"


def _validate_args(args: list[str]) -> None:
    """Ensure Azure CLI args use a strict argv contract."""
    if not isinstance(args, list):
        raise TypeError("Azure CLI args must be provided as a list of strings.")
    if not args:
        raise ValueError("Azure CLI args cannot be empty.")
    for arg in args:
        if not isinstance(arg, str):
            raise TypeError("Azure CLI args must be provided as a list of strings.")
        if _CONTROL_CHARACTERS.search(arg):
            raise ValueError("Azure CLI args cannot contain control characters.")


def _format_display_command(args: list[str], host_os: str) -> str:
    """Format a visible az command for the user's host shell."""
    _validate_args(args)
    display_args = ["az", *args]
    if host_os == "windows":
        return " ".join(_quote_for_powershell(arg) for arg in display_args)
    return shlex.join(display_args)


def format_az_command(
    args: list[str], host_os: str | None = None
) -> str:
    """Public helper to render a visible Azure CLI command."""
    return _format_display_command(args, _normalize_host_os(host_os))


def _is_device_code_login(args: list[str]) -> bool:
    """Return True when the command is az login --use-device-code."""
    return args[:2] == ["login", "--use-device-code"]


def _print_device_code_instructions(host_os: str):
    """Print clear host-browser instructions for device code login."""
    if host_os != "windows":
        return

    print(
        "\n========================================================================\n"
        "AZURE DEVICE CODE LOGIN (Windows)\n\n"
        "Open this URL in your Windows browser:\n"
        "https://microsoft.com/devicelogin\n"
        "Enter the code shown below when prompted.\n"
        "Sign in with your Entra ID account.\n"
        "========================================================================"
    )


def _resolve_azure_cli_path() -> str | None:
    """Resolve the Azure CLI path inside the Linux container."""
    for candidate in AZURE_CLI_PATHS:
        if os.path.exists(candidate):
            return candidate
    return None


def _run_with_streaming_output(command: list[str]) -> tuple[int, str]:
    """Run a command and stream combined output to the terminal."""
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    output_lines: list[str] = []

    try:
        if process.stdout is not None:
            for line in process.stdout:
                print(line, end="")
                output_lines.append(line)
        returncode = process.wait(timeout=300)
    except subprocess.TimeoutExpired:
        process.kill()
        try:
            remaining_output, _ = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            remaining_output = ""
        if remaining_output:
            print(remaining_output, end="")
            output_lines.append(remaining_output)
        return -1, "".join(output_lines)

    return returncode, "".join(output_lines)


def _format_failure_output(stdout: str, stderr: str) -> str:
    """Combine Azure CLI stdout/stderr into one readable message."""
    parts = [
        part.strip() for part in (stderr, stdout) if part and part.strip()
    ]
    return "\n".join(parts)


def _run(
    args: list[str],
    parse_json: bool = True,
    host_os: str | None = None,
) -> str:
    """Run an az CLI command and return its output as text."""
    _validate_args(args)
    normalized_host_os = _normalize_host_os(host_os)
    if _is_device_code_login(args):
        _print_device_code_instructions(normalized_host_os)

    azure_cli_path = _resolve_azure_cli_path()
    if azure_cli_path is None:
        message = (
            "COMMAND FAILED: Azure CLI was not found in the container. "
            "Expected /usr/bin/az from the official "
            "mcr.microsoft.com/azure-cli:latest base image. "
            "Rebuild the installer image and try again."
        )
        print(message)
        return message

    run_args = [azure_cli_path, *args]

    if _is_device_code_login(args):
        try:
            returncode, streamed_output = _run_with_streaming_output(run_args)
        except FileNotFoundError as exc:
            return f"COMMAND FAILED: {exc}"
        if returncode == -1:
            return "COMMAND FAILED: Azure CLI command timed out after 300 seconds."
        if returncode != 0:
            return (
                f"COMMAND FAILED (exit {returncode}):\n"
                f"{streamed_output.strip()}"
            )
        return streamed_output.strip()

    try:
        result = subprocess.run(
            run_args,
            capture_output=True,
            text=True,
            timeout=300,
        )
    except subprocess.TimeoutExpired:
        return "COMMAND FAILED: Azure CLI command timed out after 300 seconds."
    except FileNotFoundError as exc:
        return f"COMMAND FAILED: {exc}"

    if result.returncode != 0:
        error_output = _format_failure_output(result.stdout, result.stderr)
        return f"COMMAND FAILED (exit {result.returncode}):\n{error_output}"

    output = result.stdout.strip()
    if parse_json and output:
        try:
            parsed = json.loads(output)
            return json.dumps(parsed, indent=2)
        except json.JSONDecodeError:
            pass
    return output


def _parse_json_output(result: str) -> dict[str, Any] | list[Any] | None:
    """Parse JSON output when the command succeeded."""
    if (
        not result
        or "COMMAND FAILED" in result
        or "COMMAND CANCELLED" in result
        or result.strip() == "null"
    ):
        return None
    try:
        return json.loads(result)
    except json.JSONDecodeError:
        return None


def _rest_uses_get(args: list[str]) -> bool:
    """Return True when az rest is explicitly or implicitly a GET."""
    normalized = [arg.lower() for arg in args]
    method = "get"
    for index, arg in enumerate(normalized):
        if arg in {"--method", "-m"} and index + 1 < len(normalized):
            method = normalized[index + 1]
    return method == "get"


def is_safe_read_only_az_args(args: list[str]) -> bool:
    """Classify fallback Azure CLI args as read-only when clearly safe."""
    try:
        _validate_args(args)
    except (TypeError, ValueError):
        return False

    normalized = [arg.lower() for arg in args]
    if tuple(normalized[:2]) in _READ_ONLY_PREFIXES_2:
        return True
    if tuple(normalized[:3]) in _READ_ONLY_PREFIXES_3:
        return True
    if tuple(normalized[:4]) in _READ_ONLY_PREFIXES_4:
        return True
    if normalized[0] == "rest":
        return _rest_uses_get(normalized)
    return False


def _deployment_group_args(
    command: str,
    resource_group: str,
    location: str,
    admin_oid: str,
    deployment_name: str,
    name_suffix: str | None = None,
    sao_image: str | None = None,
) -> list[str]:
    """Build consistent deployment or validation argv tokens."""
    args = [
        "deployment",
        "group",
        command,
        "--name",
        deployment_name,
        "--resource-group",
        resource_group,
        "--template-file",
        "/app/bicep/main.bicep",
        "--parameters",
        f"location={location}",
        f"adminOid={admin_oid}",
    ]
    normalized_image = (sao_image or "").strip()
    if normalized_image:
        args.append(f"saoImage={normalized_image}")
    else:
        args.append(f"saoImageTag={DEFAULT_IMAGE_TAG}")
    normalized_suffix = (name_suffix or "").strip().lower()
    if normalized_suffix:
        args.append(f"nameSuffix={normalized_suffix}")
    return args


def _normalize_failed_operation(operation: dict[str, Any]) -> dict[str, Any] | None:
    """Extract the fields we care about from a deployment operation record."""
    properties = operation.get("properties", {})
    if not isinstance(properties, dict):
        return None

    state = str(
        properties.get("provisioningState")
        or operation.get("provisioningState")
        or ""
    ).strip()
    target_resource = properties.get("targetResource", {})
    if not isinstance(target_resource, dict):
        target_resource = {}

    resource_type = str(
        target_resource.get("resourceType") or target_resource.get("type") or ""
    ).strip()
    resource_name = str(
        target_resource.get("resourceName") or target_resource.get("name") or ""
    ).strip()

    status_message = properties.get("statusMessage")
    status_messages: list[str] = []
    if isinstance(status_message, dict):
        text = json.dumps(status_message, sort_keys=True)
        status_messages.append(text)
    elif isinstance(status_message, list):
        status_messages.extend(str(item).strip() for item in status_message if str(item).strip())
    elif status_message is not None:
        normalized = str(status_message).strip()
        if normalized:
            status_messages.append(normalized)

    if state not in {"Failed", "Canceled"} and not status_messages:
        return None

    return {
        "provisioning_state": state,
        "resource_type": resource_type,
        "resource_name": resource_name,
        "status_messages": status_messages[:3],
    }


def _failed_operations_from_payload(
    operations_payload: list[Any] | None,
) -> list[dict[str, Any]]:
    """Normalize the failed operations from an operation-list payload."""
    if not isinstance(operations_payload, list):
        return []

    failed_operations: list[dict[str, Any]] = []
    for operation in operations_payload:
        if not isinstance(operation, dict):
            continue
        normalized = _normalize_failed_operation(operation)
        if normalized is not None:
            failed_operations.append(normalized)
    return failed_operations


def az_login(host_os: str | None = None) -> str:
    """Initiate device code login."""
    return _run(
        ["login", "--use-device-code"], parse_json=False, host_os=host_os
    )


def get_signed_in_user(host_os: str | None = None) -> str:
    """Get current user identity."""
    return _run(
        [
            "ad",
            "signed-in-user",
            "show",
            "--query",
            "{oid:id, upn:userPrincipalName, name:displayName}",
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def list_subscriptions(host_os: str | None = None) -> str:
    """List available subscriptions."""
    return _run(
        [
            "account",
            "list",
            "--query",
            "[].{id:id, name:name, state:state}",
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def set_subscription(
    subscription_id: str, host_os: str | None = None
) -> str:
    """Set active subscription."""
    return _run(
        ["account", "set", "--subscription", subscription_id],
        parse_json=False,
        host_os=host_os,
    )


def create_resource_group(
    name: str, location: str, host_os: str | None = None
) -> str:
    """Create a resource group."""
    return _run(
        ["group", "create", "--name", name, "--location", location],
        host_os=host_os,
    )


def delete_resource_group(
    name: str, host_os: str | None = None
) -> str:
    """Delete a resource group and everything contained within it."""
    result = _run(
        ["group", "delete", "--name", name, "--yes"],
        parse_json=False,
        host_os=host_os,
    )
    if "COMMAND FAILED" in result or "COMMAND CANCELLED" in result:
        return result
    return (
        f"Cleanup requested for resource group {name}. Azure will remove the "
        "SAO test deployment and every child resource inside that group."
    )


def provision_infrastructure(
    resource_group: str,
    location: str,
    admin_oid: str,
    host_os: str | None = None,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    name_suffix: str | None = None,
    sao_image: str | None = None,
) -> str:
    """Deploy the SAO Bicep template."""
    return start_infrastructure_provisioning(
        resource_group=resource_group,
        location=location,
        admin_oid=admin_oid,
        host_os=host_os,
        deployment_name=deployment_name,
        name_suffix=name_suffix,
        sao_image=sao_image,
    )


def start_infrastructure_provisioning(
    resource_group: str,
    location: str,
    admin_oid: str,
    host_os: str | None = None,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    name_suffix: str | None = None,
    sao_image: str | None = None,
) -> str:
    """Start the SAO Bicep deployment without waiting for completion."""
    return _run(
        [
            *_deployment_group_args(
                "create",
                resource_group=resource_group,
                location=location,
                admin_oid=admin_oid,
                deployment_name=deployment_name,
                name_suffix=name_suffix,
                sao_image=sao_image,
            ),
            "--no-wait",
            "--output",
            "json",
        ],
        parse_json=False,
        host_os=host_os,
    )


def validate_infrastructure_provisioning(
    resource_group: str,
    location: str,
    admin_oid: str,
    host_os: str | None = None,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    name_suffix: str | None = None,
    sao_image: str | None = None,
) -> str:
    """Validate the SAO Bicep deployment before the write starts."""
    return _run(
        [
            *_deployment_group_args(
                "validate",
                resource_group=resource_group,
                location=location,
                admin_oid=admin_oid,
                deployment_name=deployment_name,
                name_suffix=name_suffix,
                sao_image=sao_image,
            ),
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def get_group_deployment(
    resource_group: str,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    host_os: str | None = None,
) -> str:
    """Return the full ARM deployment payload."""
    return _run(
        [
            "deployment",
            "group",
            "show",
            "--resource-group",
            resource_group,
            "--name",
            deployment_name,
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def get_group_deployment_status(
    resource_group: str,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    host_os: str | None = None,
) -> str:
    """Get the current provisioning state for an Azure group deployment."""
    return _run(
        [
            "deployment",
            "group",
            "show",
            "--resource-group",
            resource_group,
            "--name",
            deployment_name,
            "--query",
            "{state:properties.provisioningState, timestamp:properties.timestamp}",
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def get_group_deployment_endpoint(
    resource_group: str,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    host_os: str | None = None,
) -> str:
    """Read the SAO endpoint output from the completed Bicep deployment."""
    return _run(
        [
            "deployment",
            "group",
            "show",
            "--resource-group",
            resource_group,
            "--name",
            deployment_name,
            "--query",
            "properties.outputs.saoEndpoint.value",
            "--output",
            "tsv",
        ],
        parse_json=False,
        host_os=host_os,
    )


def list_resource_group_resource_types(
    resource_group: str, host_os: str | None = None
) -> str:
    """List Azure resource types currently visible in the resource group."""
    return _run(
        [
            "resource",
            "list",
            "--resource-group",
            resource_group,
            "--query",
            "[].type",
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def get_group_deployment_error(
    resource_group: str,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    host_os: str | None = None,
) -> str:
    """Read the structured ARM deployment error payload when available."""
    return _run(
        [
            "deployment",
            "group",
            "show",
            "--resource-group",
            resource_group,
            "--name",
            deployment_name,
            "--query",
            "properties.error",
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def list_group_deployment_operations(
    resource_group: str,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    host_os: str | None = None,
) -> str:
    """List ARM deployment operations to pinpoint the failing resource."""
    return _run(
        [
            "deployment",
            "operation",
            "group",
            "list",
            "--resource-group",
            resource_group,
            "--name",
            deployment_name,
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def get_container_app(
    resource_group: str,
    app_name: str = DEFAULT_CONTAINER_APP_NAME,
    host_os: str | None = None,
) -> str:
    """Return the full Container App payload."""
    return _run(
        [
            "containerapp",
            "show",
            "--resource-group",
            resource_group,
            "--name",
            app_name,
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def list_container_app_revisions(
    resource_group: str,
    app_name: str = DEFAULT_CONTAINER_APP_NAME,
    host_os: str | None = None,
) -> str:
    """List Container App revisions for troubleshooting."""
    return _run(
        [
            "containerapp",
            "revision",
            "list",
            "--resource-group",
            resource_group,
            "--name",
            app_name,
            "--all",
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def get_container_app_system_logs(
    resource_group: str,
    app_name: str = DEFAULT_CONTAINER_APP_NAME,
    tail: int = 50,
    revision: str | None = None,
    host_os: str | None = None,
) -> str:
    """Read recent system logs for the Container App."""
    args = [
        "containerapp",
        "logs",
        "show",
        "--resource-group",
        resource_group,
        "--name",
        app_name,
        "--type",
        "system",
        "--tail",
        str(tail),
        "--output",
        "json",
    ]
    normalized_revision = (revision or "").strip()
    if normalized_revision:
        args.extend(["--revision", normalized_revision])
    return _run(args, host_os=host_os)


def get_container_app_logs(
    resource_group: str,
    app_name: str = DEFAULT_CONTAINER_APP_NAME,
    tail: int = 50,
    revision: str | None = None,
    host_os: str | None = None,
) -> str:
    """Read recent application logs for the Container App."""
    args = [
        "containerapp",
        "logs",
        "show",
        "--resource-group",
        resource_group,
        "--name",
        app_name,
        "--tail",
        str(tail),
        "--output",
        "json",
    ]
    normalized_revision = (revision or "").strip()
    if normalized_revision:
        args.extend(["--revision", normalized_revision])
    return _run(args, host_os=host_os)


def list_container_app_replicas(
    resource_group: str,
    app_name: str = DEFAULT_CONTAINER_APP_NAME,
    revision: str | None = None,
    host_os: str | None = None,
) -> str:
    """List replicas for the Container App."""
    args = [
        "containerapp",
        "replica",
        "list",
        "--resource-group",
        resource_group,
        "--name",
        app_name,
        "--output",
        "json",
    ]
    normalized_revision = (revision or "").strip()
    if normalized_revision:
        args.extend(["--revision", normalized_revision])
    return _run(args, host_os=host_os)


def _select_latest_revision_name(
    app_payload: dict[str, Any] | list[Any] | None,
    revisions_payload: dict[str, Any] | list[Any] | None,
) -> str:
    """Choose the best latest-revision hint from app state or revision list."""
    if isinstance(app_payload, dict):
        properties = app_payload.get("properties", {})
        if isinstance(properties, dict):
            for key in ("latestRevisionName", "latestReadyRevisionName"):
                candidate = str(properties.get(key) or "").strip()
                if candidate:
                    return candidate

    if isinstance(revisions_payload, list):
        for revision in revisions_payload:
            if not isinstance(revision, dict):
                continue
            candidate = str(revision.get("name") or "").strip()
            if candidate:
                return candidate

    return ""


def _parse_log_excerpt(logs_payload: Any, limit: int = 5) -> list[str]:
    """Normalize assorted log payloads into a short readable excerpt."""
    if isinstance(logs_payload, str):
        return [line.strip() for line in logs_payload.splitlines() if line.strip()][:limit]

    messages: list[str] = []
    if isinstance(logs_payload, list):
        for entry in logs_payload:
            if not isinstance(entry, dict):
                normalized = str(entry).strip()
                if normalized:
                    messages.append(normalized)
                continue
            message = str(entry.get("Log") or entry.get("Msg") or "").strip()
            if message:
                messages.append(message)
    elif isinstance(logs_payload, dict):
        messages.extend(
            str(value).strip()
            for key, value in logs_payload.items()
            if key.lower().endswith("message") or key in {"messages", "Log", "Msg"}
        )

    return [message for message in messages if message][:limit]


def _find_revision_summary(
    revisions_payload: list[Any] | None,
    revision_name: str,
) -> dict[str, Any]:
    """Return the latest revision summary when available."""
    if not isinstance(revisions_payload, list):
        return {}

    normalized_revision_name = revision_name.strip()
    fallback: dict[str, Any] = {}
    for revision in revisions_payload:
        if not isinstance(revision, dict):
            continue
        if not fallback:
            fallback = revision
        if str(revision.get("name") or "").strip() == normalized_revision_name:
            return revision
    return fallback


def _find_replica_summary(replicas_payload: list[Any] | None) -> dict[str, Any]:
    """Return a compact summary of the most recent replica state."""
    if not isinstance(replicas_payload, list) or not replicas_payload:
        return {}

    replica = replicas_payload[0]
    if not isinstance(replica, dict):
        return {}

    properties = replica.get("properties", {})
    if not isinstance(properties, dict):
        properties = {}

    containers = properties.get("containers", [])
    if not isinstance(containers, list):
        containers = []

    first_container = containers[0] if containers else {}
    if not isinstance(first_container, dict):
        first_container = {}

    return {
        "name": str(replica.get("name") or "").strip(),
        "runningState": str(properties.get("runningState") or "").strip(),
        "runningStateDetails": str(
            properties.get("runningStateDetails") or ""
        ).strip(),
        "containerReady": bool(first_container.get("ready")),
        "containerRestartCount": int(first_container.get("restartCount") or 0),
        "containerRunningState": str(
            first_container.get("runningState") or ""
        ).strip(),
        "containerRunningStateDetails": str(
            first_container.get("runningStateDetails") or ""
        ).strip(),
    }


def collect_group_deployment_diagnostics(
    resource_group: str,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    host_os: str | None = None,
    _visited: set[str] | None = None,
) -> dict[str, Any]:
    """Recursively collect failure diagnostics for a deployment and its children."""
    visited = set(_visited or set())
    if deployment_name in visited:
        return {
            "deployment_name": deployment_name,
            "provisioning_state": None,
            "timestamp": None,
            "error": None,
            "failed_operations": [],
            "nested": [],
            "collection_errors": [
                f"Skipped recursive loop while collecting {deployment_name}."
            ],
        }
    visited.add(deployment_name)

    show_result = get_group_deployment(
        resource_group=resource_group,
        deployment_name=deployment_name,
        host_os=host_os,
    )
    show_payload = _parse_json_output(show_result)
    error_result = get_group_deployment_error(
        resource_group=resource_group,
        deployment_name=deployment_name,
        host_os=host_os,
    )
    error_payload = _parse_json_output(error_result)
    operations_result = list_group_deployment_operations(
        resource_group=resource_group,
        deployment_name=deployment_name,
        host_os=host_os,
    )
    operations_payload = _parse_json_output(operations_result)
    failed_operations = _failed_operations_from_payload(operations_payload)

    properties = show_payload.get("properties", {}) if isinstance(show_payload, dict) else {}
    child_deployments: list[str] = []
    for operation in failed_operations:
        if (
            str(operation.get("resource_type") or "").lower()
            == "microsoft.resources/deployments"
        ):
            child_name = str(operation.get("resource_name") or "").strip()
            if child_name and child_name not in child_deployments:
                child_deployments.append(child_name)

    nested = [
        collect_group_deployment_diagnostics(
            resource_group=resource_group,
            deployment_name=child_name,
            host_os=host_os,
            _visited=visited,
        )
        for child_name in child_deployments
        if child_name not in visited
    ]

    collection_errors: list[str] = []
    for result in (show_result, error_result, operations_result):
        if "COMMAND FAILED" in result:
            collection_errors.append(result)

    return {
        "deployment_name": deployment_name,
        "provisioning_state": properties.get("provisioningState")
        if isinstance(properties, dict)
        else None,
        "timestamp": properties.get("timestamp")
        if isinstance(properties, dict)
        else None,
        "error": error_payload,
        "failed_operations": failed_operations,
        "nested": nested,
        "collection_errors": collection_errors[:3],
    }


def collect_container_app_diagnostics(
    resource_group: str,
    app_name: str = DEFAULT_CONTAINER_APP_NAME,
    host_os: str | None = None,
) -> dict[str, Any]:
    """Gather Container App state, revisions, replicas, and logs when available."""
    app_result = get_container_app(
        resource_group=resource_group,
        app_name=app_name,
        host_os=host_os,
    )
    app_payload = _parse_json_output(app_result)

    revisions_result = list_container_app_revisions(
        resource_group=resource_group,
        app_name=app_name,
        host_os=host_os,
    )
    revisions_payload = _parse_json_output(revisions_result)
    revision_name = _select_latest_revision_name(app_payload, revisions_payload)

    replicas_payload: Any = None
    app_logs: Any = None
    system_logs: Any = None
    collection_errors: list[str] = []

    if revision_name:
        replicas_result = list_container_app_replicas(
            resource_group=resource_group,
            app_name=app_name,
            revision=revision_name,
            host_os=host_os,
        )
        replicas_payload = _parse_json_output(replicas_result)
        if replicas_payload is None and "COMMAND FAILED" in replicas_result:
            collection_errors.append(replicas_result)

        app_logs_result = get_container_app_logs(
            resource_group=resource_group,
            app_name=app_name,
            tail=50,
            revision=revision_name,
            host_os=host_os,
        )
        app_logs = _parse_json_output(app_logs_result)
        if app_logs is None:
            if "COMMAND FAILED" in app_logs_result:
                collection_errors.append(app_logs_result)
            elif app_logs_result.strip():
                app_logs = app_logs_result.strip()

        logs_result = get_container_app_system_logs(
            resource_group=resource_group,
            app_name=app_name,
            tail=50,
            revision=revision_name,
            host_os=host_os,
        )
        system_logs = _parse_json_output(logs_result)
        if system_logs is None:
            if "COMMAND FAILED" in logs_result:
                collection_errors.append(logs_result)
            elif logs_result.strip():
                system_logs = logs_result.strip()

    if "COMMAND FAILED" in app_result:
        collection_errors.append(app_result)
    if "COMMAND FAILED" in revisions_result:
        collection_errors.append(revisions_result)

    return {
        "app": app_payload,
        "revisions": revisions_payload if isinstance(revisions_payload, list) else [],
        "latest_revision": revision_name or None,
        "replicas": replicas_payload if isinstance(replicas_payload, list) else [],
        "app_logs": app_logs,
        "system_logs": system_logs,
        "collection_errors": collection_errors[:4],
    }


def list_deleted_key_vaults(host_os: str | None = None) -> str:
    """List deleted Key Vaults visible to the active subscription."""
    return _run(
        [
            "keyvault",
            "list-deleted",
            "--resource-type",
            "vault",
            "--output",
            "json",
        ],
        host_os=host_os,
    )


def purge_deleted_key_vault(
    name: str,
    location: str,
    host_os: str | None = None,
) -> str:
    """Permanently purge a soft-deleted Key Vault so the name can be reused."""
    result = _run(
        [
            "keyvault",
            "purge",
            "--name",
            name,
            "--location",
            location,
        ],
        parse_json=False,
        host_os=host_os,
    )
    if "COMMAND FAILED" in result or "COMMAND CANCELLED" in result:
        return result
    return (
        f"Purge requested for deleted Key Vault {name} in {location}. "
        "Azure is clearing the soft-deleted name so the deployment can reuse it."
    )


def check_deployment_status(
    resource_group: str, host_os: str | None = None
) -> str:
    """Check whether the deployed Container App is actually ready to serve."""
    diagnostics = collect_container_app_diagnostics(
        resource_group=resource_group,
        app_name=DEFAULT_CONTAINER_APP_NAME,
        host_os=host_os,
    )
    app_payload = diagnostics.get("app")
    app_properties = app_payload.get("properties", {}) if isinstance(app_payload, dict) else {}
    if not isinstance(app_properties, dict):
        app_properties = {}

    ingress = app_properties.get("configuration", {})
    if isinstance(ingress, dict):
        ingress = ingress.get("ingress", {})
    else:
        ingress = {}
    if not isinstance(ingress, dict):
        ingress = {}

    fqdn = str(ingress.get("fqdn") or "").strip()
    if not fqdn:
        return "COMMAND FAILED: Could not resolve the SAO Container App ingress FQDN."

    endpoint = f"https://{fqdn}"
    health_result = _run(
        ["rest", "--method", "GET", "--url", f"{endpoint}/api/health"],
        parse_json=True,
        host_os=host_os,
    )
    health_payload = _parse_json_output(health_result)
    health_status = ""
    if isinstance(health_payload, dict):
        health_status = str(health_payload.get("status") or "").strip().lower()

    latest_revision = str(diagnostics.get("latest_revision") or "").strip()
    revision_summary = _find_revision_summary(
        diagnostics.get("revisions"), latest_revision
    )
    revision_properties = (
        revision_summary.get("properties", {})
        if isinstance(revision_summary, dict)
        else {}
    )
    if not isinstance(revision_properties, dict):
        revision_properties = {}

    revision_health = str(revision_properties.get("healthState") or "").strip()
    revision_state = str(revision_properties.get("runningState") or "").strip()
    revision_state_details = str(
        revision_properties.get("runningStateDetails")
        or revision_properties.get("provisioningError")
        or ""
    ).strip()

    replica_summary = _find_replica_summary(diagnostics.get("replicas"))
    replica_details = " | ".join(
        part
        for part in [
            str(replica_summary.get("runningStateDetails") or "").strip(),
            str(replica_summary.get("containerRunningStateDetails") or "").strip(),
        ]
        if part
    )
    lowered_runtime_text = " ".join(
        part.lower()
        for part in [
            revision_health,
            revision_state,
            revision_state_details,
            replica_details,
            " ".join(_parse_log_excerpt(diagnostics.get("app_logs"), limit=3)),
            " ".join(_parse_log_excerpt(diagnostics.get("system_logs"), limit=3)),
        ]
        if part
    )

    ready = (
        health_status == "ok"
        and revision_health.lower() not in {"unhealthy"}
        and revision_state.lower() not in {"failed"}
        and (
            not replica_summary
            or bool(replica_summary.get("containerReady"))
        )
    )
    runtime_state = "warming"
    if ready:
        runtime_state = "ready"
    elif any(
        token in lowered_runtime_text
        for token in (
            "crashloopbackoff",
            "container crashing",
            "persistent failure",
            "exit code",
            "panicked",
            "terminated",
            "unhealthy",
            "failed",
        )
    ):
        runtime_state = "failed"

    lines = [
        f"Endpoint: {endpoint}",
        f"Ready: {'true' if ready else 'false'}",
        f"Runtime state: {runtime_state}",
    ]
    if latest_revision:
        lines.append(f"Revision: {latest_revision}")
    if revision_health:
        lines.append(f"Revision health: {revision_health}")
    if revision_state:
        lines.append(f"Revision state: {revision_state}")
    if revision_state_details:
        lines.append(f"Revision details: {revision_state_details}")
    if replica_summary:
        lines.append(
            "Replica state: "
            + " / ".join(
                part
                for part in [
                    str(replica_summary.get("runningState") or "").strip(),
                    str(replica_summary.get("containerRunningState") or "").strip(),
                ]
                if part
            )
        )
        if replica_details:
            lines.append(f"Replica details: {replica_details}")
        lines.append(
            f"Replica restarts: {replica_summary.get('containerRestartCount', 0)}"
        )

    if health_payload is not None:
        lines.append(f"Health: {json.dumps(health_payload, indent=2)}")
    else:
        lines.append(f"Health: {health_result.strip()}")

    app_log_excerpt = _parse_log_excerpt(diagnostics.get("app_logs"), limit=4)
    if app_log_excerpt:
        lines.append("Application logs:")
        lines.extend(f"- {line}" for line in app_log_excerpt)

    system_log_excerpt = _parse_log_excerpt(diagnostics.get("system_logs"), limit=4)
    if system_log_excerpt:
        lines.append("System logs:")
        lines.extend(f"- {line}" for line in system_log_excerpt)

    collection_errors = diagnostics.get("collection_errors", [])
    if isinstance(collection_errors, list) and collection_errors:
        lines.append("Diagnostics warnings:")
        lines.extend(f"- {warning}" for warning in collection_errors[:2])

    return "\n".join(lines)


def run_az_command(args: list[str], host_os: str | None = None) -> str:
    """Run an arbitrary az command using strict argv tokens."""
    if not isinstance(args, list) or any(not isinstance(arg, str) for arg in args):
        return "REJECTED: args must be a non-empty array of strings."

    normalized_args = list(args)
    if normalized_args and normalized_args[0] == "az":
        normalized_args = normalized_args[1:]
    if not normalized_args:
        return "REJECTED: args must contain at least one Azure CLI token."

    try:
        return _run(normalized_args, parse_json=False, host_os=host_os)
    except (TypeError, ValueError) as exc:
        return f"REJECTED: {exc}"
