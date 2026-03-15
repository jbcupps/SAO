"""Azure CLI wrappers for the SAO installer agent."""

import json
import os
import re
import shlex
import subprocess

HOST_OS = os.environ.get("HOST_OS", "windows" if os.name == "nt" else "linux")
AZURE_CLI_PATH = "/usr/local/bin/az"


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


def _format_display_command(args: list[str], host_os: str) -> str:
    """Format a visible az command for the user's host shell."""
    display_args = ["az", *args]
    if host_os == "windows":
        return " ".join(_quote_for_powershell(arg) for arg in display_args)
    return shlex.join(display_args)


def _confirm_command(display_command: str, host_os: str) -> bool:
    """Show the command and require explicit user confirmation."""
    shell_name = "PowerShell" if host_os == "windows" else "bash"
    border = "=" * 72
    print(f"\n{border}")
    print(f"Azure CLI command ({shell_name} syntax):")
    print(display_command)
    print(border)

    while True:
        try:
            answer = input("Run this command? (y/n) ").strip().lower()
        except EOFError:
            return False
        if answer in {"y", "n"}:
            return answer == "y"
        print("Please enter 'y' or 'n'.")


def _run(
    args: list[str],
    parse_json: bool = True,
    host_os: str | None = None,
) -> str:
    """Run an az CLI command, return output as string."""
    normalized_host_os = _normalize_host_os(host_os)
    display_command = _format_display_command(args, normalized_host_os)
    if not _confirm_command(display_command, normalized_host_os):
        return "COMMAND CANCELLED: User declined to run command."

    run_args = [AZURE_CLI_PATH, *args]
    bash_command = shlex.join(run_args)
    print(f"DEBUG: executing via /bin/bash -lc {shlex.quote(bash_command)}")

    try:
        result = subprocess.run(
            ["/bin/bash", "-lc", bash_command],
            shell=False,
            executable="/bin/bash",
            capture_output=True,
            text=True,
            timeout=300,
        )
    except subprocess.TimeoutExpired:
        return "COMMAND FAILED: Azure CLI command timed out after 300 seconds."
    except FileNotFoundError as exc:
        return f"COMMAND FAILED: {exc}"

    if result.returncode != 0:
        return f"COMMAND FAILED (exit {result.returncode}):\n{result.stderr.strip()}"
    output = result.stdout.strip()
    if parse_json:
        try:
            parsed = json.loads(output)
            return json.dumps(parsed, indent=2)
        except json.JSONDecodeError:
            pass
    return output


def az_login(host_os: str | None = None) -> str:
    """Initiate device code login."""
    return _run(["login", "--use-device-code"], parse_json=False, host_os=host_os)


def get_signed_in_user(host_os: str | None = None) -> str:
    """Get current user identity."""
    return _run(
        [
            "ad",
            "signed-in-user",
            "show",
            "--query",
            "{oid:id, upn:userPrincipalName, name:displayName}",
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


def provision_infrastructure(
    resource_group: str,
    location: str,
    admin_oid: str,
    host_os: str | None = None,
) -> str:
    """Deploy the SAO Bicep template."""
    return _run(
        [
            "deployment",
            "group",
            "create",
            "--resource-group",
            resource_group,
            "--template-file",
            "/app/bicep/main.bicep",
            "--parameters",
            f"location={location}",
            f"adminOid={admin_oid}",
            "saoImageTag=latest",
            "--output",
            "json",
        ],
        host_os=host_os,
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


# Characters that indicate shell injection attempts
_SHELL_METACHARACTERS = re.compile(r"[|;&`$()]")


def run_az_command(command: str, host_os: str | None = None) -> str:
    """Run an arbitrary az command with basic sanitization."""
    if _SHELL_METACHARACTERS.search(command):
        return (
            "REJECTED: Command contains shell metacharacters. "
            "Use only simple az CLI arguments."
        )
    try:
        args = shlex.split(command, posix=True)
    except ValueError as exc:
        return f"REJECTED: Unable to parse command: {exc}"

    if args and args[0] == "az":
        args = args[1:]
    if not args:
        return "REJECTED: Command is empty."

    return _run(args, parse_json=False, host_os=host_os)
