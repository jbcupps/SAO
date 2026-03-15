"""Pre-flight permission checks for SAO deployment."""

import json

from tools.azure import _run, format_az_command

PROVIDERS_NEEDED = [
    "Microsoft.App",
    "Microsoft.DBforPostgreSQL",
    "Microsoft.KeyVault",
    "Microsoft.OperationalInsights",
]


def _account_show_command() -> list[str]:
    """Return the active subscription lookup command."""
    return [
        "account",
        "show",
        "--query",
        "{id:id, name:name}",
        "--output",
        "json",
    ]


def _role_assignment_command(
    admin_oid: str, subscription_id: str
) -> list[str]:
    """Return the role assignment lookup command."""
    return [
        "role",
        "assignment",
        "list",
        "--assignee",
        admin_oid,
        "--scope",
        f"/subscriptions/{subscription_id}",
        "--query",
        "[].{role:roleDefinitionName, scope:scope}",
        "--output",
        "json",
    ]


def _graph_canary_command() -> list[str]:
    """Return the Graph API permission canary command."""
    return [
        "rest",
        "--method",
        "GET",
        "--url",
        "https://graph.microsoft.com/v1.0/me",
        "--query",
        "id",
        "-o",
        "tsv",
    ]


def _provider_command(provider: str) -> list[str]:
    """Return the provider registration check command."""
    return [
        "provider",
        "show",
        "--namespace",
        provider,
        "--query",
        "registrationState",
        "-o",
        "tsv",
    ]


def describe_permission_check_commands(
    admin_oid: str | None = None,
    subscription_id: str | None = None,
    host_os: str | None = None,
) -> list[str]:
    """Return human-readable Azure CLI previews for permission checks."""
    resolved_admin_oid = admin_oid or "<signed-in-user-oid>"
    resolved_subscription_id = subscription_id or "<active-subscription-id>"
    commands = [
        _account_show_command(),
        _role_assignment_command(resolved_admin_oid, resolved_subscription_id),
        _graph_canary_command(),
    ]
    commands.extend(_provider_command(provider) for provider in PROVIDERS_NEEDED)
    return [format_az_command(command, host_os=host_os) for command in commands]


def check_permissions(
    admin_oid: str | None = None,
    subscription_id: str | None = None,
    host_os: str | None = None,
) -> str:
    """Comprehensive pre-flight permission check."""
    results: dict[str, str | list[dict] | dict[str, str]] = {}

    subscription_result = _run(_account_show_command(), host_os=host_os)
    results["subscription"] = subscription_result

    active_subscription_id = subscription_id
    if "COMMAND FAILED" not in subscription_result:
        try:
            parsed_subscription = json.loads(subscription_result)
            active_subscription_id = active_subscription_id or parsed_subscription.get(
                "id"
            )
        except json.JSONDecodeError:
            parsed_subscription = None
    else:
        parsed_subscription = None

    if admin_oid and active_subscription_id:
        role_result = _run(
            _role_assignment_command(admin_oid, active_subscription_id),
            host_os=host_os,
        )
        results["role_assignments"] = role_result
    elif admin_oid and not active_subscription_id:
        results["role_assignments"] = (
            "SKIPPED: Unable to resolve an active subscription for role lookup."
        )
    else:
        results["role_assignments"] = (
            "SKIPPED: Admin Object ID not available yet for role lookup."
        )

    graph = _run(_graph_canary_command(), parse_json=False, host_os=host_os)
    results["graph_api"] = (
        "OK" if "COMMAND FAILED" not in graph else f"FAILED: {graph}"
    )

    for provider in PROVIDERS_NEEDED:
        check = _run(
            _provider_command(provider),
            parse_json=False,
            host_os=host_os,
        )
        results[f"provider_{provider}"] = check.strip()

    if parsed_subscription is not None:
        results["active_subscription_id"] = parsed_subscription.get("id", "")

    return json.dumps(results, indent=2)
