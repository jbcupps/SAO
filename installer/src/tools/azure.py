"""Azure CLI wrappers for the SAO installer agent."""

import json
import os
import re
import shlex
import subprocess

HOST_OS = os.environ.get("HOST_OS", "windows" if os.name == "nt" else "linux")
AZURE_CLI_PATHS = ("/usr/bin/az", "/usr/local/bin/az")
DEFAULT_DEPLOYMENT_NAME = "sao-bootstrap"
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
    """Run an az CLI command, return output as string."""
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
        "saoImageTag=latest",
    ]
    normalized_suffix = (name_suffix or "").strip().lower()
    if normalized_suffix:
        args.append(f"nameSuffix={normalized_suffix}")
    return args


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
) -> str:
    """Deploy the SAO Bicep template."""
    return start_infrastructure_provisioning(
        resource_group=resource_group,
        location=location,
        admin_oid=admin_oid,
        host_os=host_os,
        deployment_name=deployment_name,
        name_suffix=name_suffix,
    )


def start_infrastructure_provisioning(
    resource_group: str,
    location: str,
    admin_oid: str,
    host_os: str | None = None,
    deployment_name: str = DEFAULT_DEPLOYMENT_NAME,
    name_suffix: str | None = None,
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
            ),
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
    """Check if SAO container is running."""
    fqdn_result = _run(
        [
            "containerapp",
            "show",
            "--name",
            "sao-app",
            "--resource-group",
            resource_group,
            "--query",
            "properties.configuration.ingress.fqdn",
            "-o",
            "tsv",
        ],
        parse_json=False,
        host_os=host_os,
    )
    if "COMMAND FAILED" in fqdn_result or "COMMAND CANCELLED" in fqdn_result:
        return fqdn_result

    health_result = _run(
        ["rest", "--method", "GET", "--url", f"https://{fqdn_result}/api/health"],
        parse_json=True,
        host_os=host_os,
    )
    return f"Endpoint: https://{fqdn_result}\nHealth: {health_result}"


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
