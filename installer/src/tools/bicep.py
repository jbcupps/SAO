"""Bicep deployment tools for the SAO installer agent.

Handles Azure resource deployment via Bicep templates
for production hosting of SAO on Azure Container Apps.
"""

from dataclasses import dataclass
from typing import Any


@dataclass
class BicepTools:
    """Tools for Azure Bicep deployments.

        These tools allow the installer agent to:
        - Validate Bicep templates
        - Deploy Azure Container Apps + PostgreSQL Flexible Server
        - Check deployment status
        - Configure container environment variables

        Note: Bicep deployment is optional — SAO can run entirely
        in local Docker. These tools are for production Azure hosting.
    """

    async def validate_template(self, template_path: str) -> dict[str, Any]:
        """Validate a Bicep template without deploying.

        Args:
            template_path: Path to the .bicep file.

        Returns:
            Dict with 'valid' (bool) and 'errors' (list) fields.
        """
        # TODO: Run `az bicep build` to validate template
        raise NotImplementedError("Bicep validation not yet implemented")

    async def deploy(
        self,
        resource_group: str,
        template_path: str,
        parameters: dict[str, Any],
    ) -> dict[str, Any]:
        """Deploy Azure resources via Bicep template.

        Deploys the SAO infrastructure stack:
        - Azure Container Apps environment
        - SAO container app
        - PostgreSQL Flexible Server
        - Managed Identity

        The Azure runtime expects the production SAO application image
        contract, not the standalone installer image.

        Args:
            resource_group: Azure resource group name.
            template_path: Path to the .bicep file.
            parameters: Deployment parameters.

        Returns:
            Dict with deployment outputs (endpoints, connection strings).
        """
        # TODO: Run `az deployment group create` with template and parameters
        raise NotImplementedError("Bicep deployment not yet implemented")

    async def check_deployment_status(
        self, resource_group: str, deployment_name: str
    ) -> dict[str, Any]:
        """Check the status of a Bicep deployment.

        Args:
            resource_group: Azure resource group name.
            deployment_name: Name of the deployment.

        Returns:
            Dict with 'status', 'provisioning_state', and 'outputs'.
        """
        # TODO: Run `az deployment group show` to check status
        raise NotImplementedError("Deployment status check not yet implemented")
