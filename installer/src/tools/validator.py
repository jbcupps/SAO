"""Pre-flight permission checks for SAO deployment."""

import json

from tools.azure import _run


def check_permissions() -> str:
    """Comprehensive pre-flight permission check."""
    results = {}

    # 1. Subscription access
    sub = _run("account show --query '{id:id, name:name}'")
    results["subscription"] = sub

    # 2. Graph API access (canary call)
    graph = _run(
        "rest --method GET "
        "--url https://graph.microsoft.com/v1.0/me "
        "--query id -o tsv",
        parse_json=False,
    )
    results["graph_api"] = (
        "OK" if "COMMAND FAILED" not in graph else f"FAILED: {graph}"
    )

    # 3. Resource providers
    providers_needed = [
        "Microsoft.App",
        "Microsoft.DBforPostgreSQL",
        "Microsoft.KeyVault",
        "Microsoft.OperationalInsights",
    ]
    for provider in providers_needed:
        check = _run(
            f"provider show --namespace {provider} "
            f"--query registrationState -o tsv",
            parse_json=False,
        )
        results[f"provider_{provider}"] = check.strip()

    return json.dumps(results, indent=2)
