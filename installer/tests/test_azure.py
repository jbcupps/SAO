import io
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import Mock, patch

SRC_ROOT = Path(__file__).resolve().parents[1] / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

from tools import azure


class FakeStreamingProcess:
    def __init__(self, lines: list[str], returncode: int = 0):
        self.stdout = iter(lines)
        self._returncode = returncode

    def wait(self, timeout: int) -> int:
        return self._returncode


class AzureToolTests(unittest.TestCase):
    def test_run_uses_direct_argv_without_shell_wrapper(self):
        result = Mock(returncode=0, stdout='{"id":"sub-123"}', stderr="")

        with patch.object(
            azure, "_resolve_azure_cli_path", return_value="/usr/bin/az"
        ), patch("tools.azure.subprocess.run", return_value=result) as run_mock:
            output = azure._run(
                ["account", "show", "--query", "{id:id}"],
                host_os="windows",
            )

        self.assertIn('"id": "sub-123"', output)
        called_args = run_mock.call_args.args[0]
        called_kwargs = run_mock.call_args.kwargs
        self.assertEqual(
            called_args,
            ["/usr/bin/az", "account", "show", "--query", "{id:id}"],
        )
        self.assertNotIn("shell", called_kwargs)
        self.assertNotIn("executable", called_kwargs)

    def test_az_login_streams_with_direct_argv(self):
        process = FakeStreamingProcess(["Line 1\n", "Line 2\n"])

        with patch.object(
            azure, "_resolve_azure_cli_path", return_value="/usr/bin/az"
        ), patch(
            "tools.azure.subprocess.Popen", return_value=process
        ) as popen_mock, redirect_stdout(io.StringIO()):
            output = azure.az_login(host_os="windows")

        self.assertEqual(output, "Line 1\nLine 2")
        called_args = popen_mock.call_args.args[0]
        called_kwargs = popen_mock.call_args.kwargs
        self.assertEqual(
            called_args,
            ["/usr/bin/az", "login", "--use-device-code"],
        )
        self.assertNotIn("shell", called_kwargs)
        self.assertNotIn("executable", called_kwargs)

    def test_read_only_classifier_distinguishes_safe_and_write_commands(self):
        self.assertTrue(
            azure.is_safe_read_only_az_args(
                ["rest", "--method", "GET", "--url", "https://example"]
            )
        )
        self.assertTrue(
            azure.is_safe_read_only_az_args(
                ["role", "assignment", "list", "--assignee", "oid"]
            )
        )
        self.assertTrue(
            azure.is_safe_read_only_az_args(
                ["deployment", "operation", "group", "list", "--name", "sao-bootstrap"]
            )
        )
        self.assertTrue(
            azure.is_safe_read_only_az_args(["keyvault", "list-deleted"])
        )
        self.assertFalse(
            azure.is_safe_read_only_az_args(
                ["group", "create", "--name", "sao-rg"]
            )
        )

    def test_start_infrastructure_provisioning_uses_no_wait_and_fixed_name(self):
        result = Mock(returncode=0, stdout="", stderr="")

        with patch.object(
            azure, "_resolve_azure_cli_path", return_value="/usr/bin/az"
        ), patch("tools.azure.subprocess.run", return_value=result) as run_mock:
            azure.start_infrastructure_provisioning(
                resource_group="sao-rg",
                location="eastus2",
                admin_oid="oid-123",
                host_os="windows",
            )

        self.assertEqual(
            run_mock.call_args.args[0],
            [
                "/usr/bin/az",
                "deployment",
                "group",
                "create",
                "--name",
                azure.DEFAULT_DEPLOYMENT_NAME,
                "--resource-group",
                "sao-rg",
                "--template-file",
                "/app/bicep/main.bicep",
                "--parameters",
                "location=eastus2",
                "adminOid=oid-123",
                "saoImageTag=latest",
                "--no-wait",
                "--output",
                "json",
            ],
        )

    def test_validate_infrastructure_provisioning_supports_optional_suffix(self):
        result = Mock(returncode=0, stdout='{"status":"Valid"}', stderr="")

        with patch.object(
            azure, "_resolve_azure_cli_path", return_value="/usr/bin/az"
        ), patch("tools.azure.subprocess.run", return_value=result) as run_mock:
            azure.validate_infrastructure_provisioning(
                resource_group="sao-rg",
                location="eastus2",
                admin_oid="oid-123",
                host_os="windows",
                name_suffix="a7c",
            )

        self.assertEqual(
            run_mock.call_args.args[0],
            [
                "/usr/bin/az",
                "deployment",
                "group",
                "validate",
                "--name",
                azure.DEFAULT_DEPLOYMENT_NAME,
                "--resource-group",
                "sao-rg",
                "--template-file",
                "/app/bicep/main.bicep",
                "--parameters",
                "location=eastus2",
                "adminOid=oid-123",
                "saoImageTag=latest",
                "nameSuffix=a7c",
                "--output",
                "json",
            ],
        )

    def test_get_group_deployment_status_uses_expected_status_query(self):
        result = Mock(
            returncode=0,
            stdout='{"state":"Running","timestamp":"2026-03-15T12:00:00Z"}',
            stderr="",
        )

        with patch.object(
            azure, "_resolve_azure_cli_path", return_value="/usr/bin/az"
        ), patch("tools.azure.subprocess.run", return_value=result) as run_mock:
            output = azure.get_group_deployment_status(
                resource_group="sao-rg",
                deployment_name="sao-bootstrap",
                host_os="windows",
            )

        self.assertIn('"state": "Running"', output)
        self.assertEqual(
            run_mock.call_args.args[0],
            [
                "/usr/bin/az",
                "deployment",
                "group",
                "show",
                "--resource-group",
                "sao-rg",
                "--name",
                "sao-bootstrap",
                "--query",
                "{state:properties.provisioningState, timestamp:properties.timestamp}",
                "--output",
                "json",
            ],
        )

    def test_delete_resource_group_uses_expected_command(self):
        result = Mock(returncode=0, stdout="", stderr="")

        with patch.object(
            azure, "_resolve_azure_cli_path", return_value="/usr/bin/az"
        ), patch("tools.azure.subprocess.run", return_value=result) as run_mock:
            output = azure.delete_resource_group(
                "sao-rg", host_os="windows"
            )

        self.assertIn("Cleanup requested for resource group sao-rg", output)
        self.assertEqual(
            run_mock.call_args.args[0],
            [
                "/usr/bin/az",
                "group",
                "delete",
                "--name",
                "sao-rg",
                "--yes",
            ],
        )

    def test_list_deleted_key_vaults_uses_expected_command(self):
        result = Mock(returncode=0, stdout="[]", stderr="")

        with patch.object(
            azure, "_resolve_azure_cli_path", return_value="/usr/bin/az"
        ), patch("tools.azure.subprocess.run", return_value=result) as run_mock:
            azure.list_deleted_key_vaults(host_os="windows")

        self.assertEqual(
            run_mock.call_args.args[0],
            [
                "/usr/bin/az",
                "keyvault",
                "list-deleted",
                "--resource-type",
                "vault",
                "--output",
                "json",
            ],
        )


if __name__ == "__main__":
    unittest.main()
