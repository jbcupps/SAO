"""Azure CLI wrappers for the SAO installer agent."""

import json
import re
import subprocess


def _run(cmd: str, parse_json: bool = True) -> str:
    """Run an az CLI command, return output as string."""
    full_cmd = f"az {cmd}"
    print(f"  $ {full_cmd}")
    result = subprocess.run(
        full_cmd, shell=True, capture_output=True, text=True, timeout=300
    )
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


def az_login() -> str:
    """Initiate device code login."""
    return _run("login --use-device-code", parse_json=False)


def get_signed_in_user() -> str:
    """Get current user identity."""
    return _run(
        "ad signed-in-user show "
        "--query '{oid:id, upn:userPrincipalName, name:displayName}'"
    )


def list_subscriptions() -> str:
    """List available subscriptions."""
    return _run(
        "account list --query '[].{id:id, name:name, state:state}' --output json"
    )


def set_subscription(subscription_id: str) -> str:
    """Set active subscription."""
    return _run(
        f"account set --subscription {subscription_id}", parse_json=False
    )


def create_resource_group(name: str, location: str) -> str:
    """Create a resource group."""
    return _run(f"group create --name {name} --location {location}")


def provision_infrastructure(
    resource_group: str, location: str, admin_oid: str
) -> str:
    """Deploy the SAO Bicep template."""
    return _run(
        f"deployment group create "
        f"--resource-group {resource_group} "
        f"--template-file /app/bicep/main.bicep "
        f"--parameters location={location} adminOid={admin_oid} saoImageTag=latest "
        f"--output json"
    )


def check_deployment_status(resource_group: str) -> str:
    """Check if SAO container is running."""
    fqdn_result = _run(
        f"containerapp show --name sao-app --resource-group {resource_group} "
        f"--query 'properties.configuration.ingress.fqdn' -o tsv",
        parse_json=False,
    )
    if "COMMAND FAILED" in fqdn_result:
        return fqdn_result

    health_result = _run(
        f"rest --method GET --url https://{fqdn_result}/api/health",
        parse_json=True,
    )
    return f"Endpoint: https://{fqdn_result}\nHealth: {health_result}"


# Characters that indicate shell injection attempts
_SHELL_METACHARACTERS = re.compile(r"[|;&`$()]")


def run_az_command(command: str) -> str:
    """Run an arbitrary az command with basic sanitization."""
    if _SHELL_METACHARACTERS.search(command):
        return (
            "REJECTED: Command contains shell metacharacters. "
            "Use only simple az CLI arguments."
        )
    return _run(command, parse_json=False)
