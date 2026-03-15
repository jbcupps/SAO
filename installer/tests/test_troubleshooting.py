import sys
import unittest
from pathlib import Path

SRC_ROOT = Path(__file__).resolve().parents[1] / "src"
if str(SRC_ROOT) not in sys.path:
    sys.path.insert(0, str(SRC_ROOT))

from tools import troubleshooting


class TroubleshootingTests(unittest.TestCase):
    def test_classifies_keyvault_soft_delete(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "sao-bootstrap",
                "location": "eastus2",
                "failed_resource_type": "Microsoft.KeyVault/vaults",
                "failed_resource_name": "sao-abc-kv",
                "raw_error": (
                    "ConflictError: A vault with the same name already exists "
                    "in deleted state."
                ),
                "evidence": ["deleted but recoverable"],
            }
        )

        self.assertEqual(response["issue_type"], "keyvault_soft_delete")
        self.assertIn("purge_deleted_key_vault", response["guided_actions"])
        self.assertIn(
            "az keyvault purge --name sao-abc-kv --location eastus2",
            response["manual_commands"],
        )

    def test_classifies_keyvault_name_conflict(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "sao-bootstrap",
                "location": "eastus2",
                "failed_resource_type": "Microsoft.KeyVault/vaults",
                "failed_resource_name": "sao-abc-kv",
                "raw_error": "Conflict: Vault name sao-abc-kv is already in use.",
                "evidence": ["already in use"],
            }
        )

        self.assertEqual(response["issue_type"], "keyvault_name_conflict")
        self.assertIn("retry_with_name_suffix", response["guided_actions"])

    def test_classifies_ghcr_private_package(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "container-app",
                "location": "eastus2",
                "failed_resource_type": "Microsoft.App/containerApps",
                "failed_resource_name": "sao-app",
                "image_reference": "ghcr.io/jbcupps/sao:latest",
                "raw_error": (
                    "ContainerAppOperationError: DENIED: requested access to "
                    "the resource is denied"
                ),
                "evidence": ["ghcr.io/jbcupps/sao:latest"],
            }
        )

        self.assertEqual(response["issue_type"], "container_image_ghcr_private")
        self.assertIn("retry_with_image_override", response["guided_actions"])
        self.assertIn("GHCR package", response["diagnosis"])
        self.assertIn(
            "Confirm the GitHub Container Registry package for ghcr.io/jbcupps/sao:latest is set to Public in GitHub package settings",
            response["manual_commands"],
        )
        self.assertTrue(
            any(
                command == "docker manifest inspect ghcr.io/jbcupps/sao:latest"
                for command in response["manual_commands"]
            )
        )

    def test_classifies_container_image_denied_for_non_ghcr_registry(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "container-app",
                "location": "eastus2",
                "failed_resource_type": "Microsoft.App/containerApps",
                "failed_resource_name": "sao-app",
                "image_reference": "docker.io/example/sao:latest",
                "raw_error": (
                    "ContainerAppOperationError: unauthorized: authentication "
                    "required"
                ),
                "evidence": ["docker.io/example/sao:latest"],
            }
        )

        self.assertEqual(response["issue_type"], "container_image_denied")
        self.assertTrue(
            any("registry set" in command for command in response["manual_commands"])
        )

    def test_classifies_container_image_not_found(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "container-app",
                "location": "eastus2",
                "failed_resource_type": "Microsoft.App/containerApps",
                "failed_resource_name": "sao-app",
                "image_reference": "ghcr.io/jbcupps/sao:missing",
                "raw_error": "manifest unknown: tag does not exist",
            }
        )

        self.assertEqual(response["issue_type"], "container_image_not_found")

    def test_classifies_containerapp_revision_failed(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "container-app",
                "location": "eastus2",
                "failed_resource_type": "Microsoft.App/containerApps",
                "failed_resource_name": "sao-app",
                "raw_error": "ContainerAppOperationError: revision failed readiness probe",
            }
        )

        self.assertEqual(response["issue_type"], "containerapp_revision_failed")

    def test_classifies_provider_not_registered(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "sao-bootstrap",
                "location": "eastus2",
                "raw_error": "The subscription is not registered to use namespace Microsoft.App",
            }
        )

        self.assertEqual(response["issue_type"], "provider_not_registered")

    def test_classifies_quota_or_capacity(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "sao-bootstrap",
                "location": "eastus2",
                "raw_error": "Regional capacity is currently unavailable for this SKU.",
            }
        )

        self.assertEqual(response["issue_type"], "quota_or_capacity")

    def test_falls_back_to_unknown(self):
        response = troubleshooting.build_troubleshooting_response(
            {
                "resource_group": "sao-rg",
                "deployment_name": "sao-bootstrap",
                "location": "eastus2",
                "raw_error": "something strange happened",
            }
        )

        self.assertEqual(response["issue_type"], "unknown")


if __name__ == "__main__":
    unittest.main()
