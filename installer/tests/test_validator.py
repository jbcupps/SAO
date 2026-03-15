import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

SRC_ROOT = Path(__file__).resolve().parents[1] / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

from tools import validator


class ValidatorTests(unittest.TestCase):
    def test_check_permissions_uses_expected_read_only_commands(self):
        calls: list[tuple[list[str], bool, str | None]] = []

        def fake_run(
            args: list[str],
            parse_json: bool = True,
            host_os: str | None = None,
        ) -> str:
            calls.append((list(args), parse_json, host_os))
            if args[:2] == ["account", "show"]:
                return '{"id":"sub-123","name":"Test Subscription"}'
            if args[:3] == ["role", "assignment", "list"]:
                return '[{"role":"Owner","scope":"/subscriptions/sub-123"}]'
            if args[:2] == ["rest", "--method"]:
                return "user-id"
            if args[:2] == ["provider", "show"]:
                return "Registered"
            raise AssertionError(f"Unexpected command: {args}")

        with patch("tools.validator._run", side_effect=fake_run):
            result = validator.check_permissions(
                admin_oid="oid-123", host_os="windows"
            )

        parsed = json.loads(result)
        self.assertEqual(
            calls,
            [
                (
                    [
                        "account",
                        "show",
                        "--query",
                        "{id:id, name:name}",
                        "--output",
                        "json",
                    ],
                    True,
                    "windows",
                ),
                (
                    [
                        "role",
                        "assignment",
                        "list",
                        "--assignee",
                        "oid-123",
                        "--scope",
                        "/subscriptions/sub-123",
                        "--query",
                        "[].{role:roleDefinitionName, scope:scope}",
                        "--output",
                        "json",
                    ],
                    True,
                    "windows",
                ),
                (
                    [
                        "rest",
                        "--method",
                        "GET",
                        "--url",
                        "https://graph.microsoft.com/v1.0/me",
                        "--query",
                        "id",
                        "-o",
                        "tsv",
                    ],
                    False,
                    "windows",
                ),
                (
                    [
                        "provider",
                        "show",
                        "--namespace",
                        "Microsoft.App",
                        "--query",
                        "registrationState",
                        "-o",
                        "tsv",
                    ],
                    False,
                    "windows",
                ),
                (
                    [
                        "provider",
                        "show",
                        "--namespace",
                        "Microsoft.DBforPostgreSQL",
                        "--query",
                        "registrationState",
                        "-o",
                        "tsv",
                    ],
                    False,
                    "windows",
                ),
                (
                    [
                        "provider",
                        "show",
                        "--namespace",
                        "Microsoft.KeyVault",
                        "--query",
                        "registrationState",
                        "-o",
                        "tsv",
                    ],
                    False,
                    "windows",
                ),
                (
                    [
                        "provider",
                        "show",
                        "--namespace",
                        "Microsoft.OperationalInsights",
                        "--query",
                        "registrationState",
                        "-o",
                        "tsv",
                    ],
                    False,
                    "windows",
                ),
            ],
        )
        self.assertEqual(parsed["graph_api"], "OK")
        self.assertEqual(parsed["active_subscription_id"], "sub-123")

    def test_permission_previews_include_role_assignment_and_providers(self):
        previews = validator.describe_permission_check_commands(
            admin_oid="oid-123",
            subscription_id="sub-123",
            host_os="windows",
        )

        self.assertIn(
            "az role assignment list --assignee oid-123 --scope /subscriptions/sub-123",
            previews[1],
        )
        self.assertEqual(len(previews), 7)


if __name__ == "__main__":
    unittest.main()
