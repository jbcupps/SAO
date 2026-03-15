"""Entra ID (Azure AD) tools for the SAO installer agent.

Handles OIDC authentication, app registration validation,
and Microsoft Graph API discovery during bootstrap.
"""

from dataclasses import dataclass
from typing import Any


@dataclass
class EntraTools:
    """Tools for interacting with Microsoft Entra ID during installation.

    These tools allow the installer agent to:
    - Authenticate the admin via OIDC
    - Validate or create Entra app registrations
    - Query Graph API for tenant structure and role mappings
    """

    async def authenticate_admin(self, tenant_id: str) -> dict[str, Any]:
        """Initiate OIDC authentication flow for the admin.

        Starts an Authorization Code flow with PKCE against the
        specified Entra ID tenant. The admin completes auth in
        their browser; this method waits for the callback.

        Args:
            tenant_id: The Entra ID tenant ID or domain.

        Returns:
            Dict with 'oid' (Object ID), 'name', 'email' of
            the authenticated admin.
        """
        # TODO: Implement OIDC Authorization Code flow with PKCE
        # 1. Generate PKCE code_verifier and code_challenge
        # 2. Build authorization URL
        # 3. Open browser or display URL for admin
        # 4. Start local callback server to receive auth code
        # 5. Exchange auth code for tokens
        # 6. Extract OID from id_token claims
        raise NotImplementedError("Entra OIDC authentication not yet implemented")

    async def validate_app_registration(
        self, client_id: str, tenant_id: str
    ) -> dict[str, Any]:
        """Validate an existing Entra app registration.

        Checks that the app registration has the required
        redirect URIs, permissions, and configuration for SAO.

        Args:
            client_id: The application (client) ID.
            tenant_id: The Entra ID tenant ID.

        Returns:
            Dict with validation results and any missing config.
        """
        # TODO: Use Graph API to fetch app registration details
        # Check: redirect URIs, API permissions, token configuration
        raise NotImplementedError("App registration validation not yet implemented")

    async def discover_tenant(self, access_token: str) -> dict[str, Any]:
        """Discover tenant structure via Microsoft Graph API.

        Queries Graph API to discover:
        - Organizational units
        - Security groups
        - App roles
        - User count and structure

        Args:
            access_token: Bearer token with Graph API permissions.

        Returns:
            Dict with discovered tenant structure.
        """
        # TODO: Use msgraph-sdk to query tenant structure
        # GET /organization, /groups, /users (summary), /appRoleAssignments
        raise NotImplementedError("Tenant discovery not yet implemented")

    async def suggest_role_mappings(
        self, tenant_structure: dict[str, Any]
    ) -> list[dict[str, str]]:
        """Suggest SAO role mappings based on tenant structure.

        Analyzes discovered Entra groups and roles, then suggests
        which should map to SAO's User and Administrator roles.

        Args:
            tenant_structure: Output from discover_tenant().

        Returns:
            List of suggested mappings with 'entra_group',
            'sao_role', and 'reason' fields.
        """
        # TODO: Analyze tenant structure and generate role mapping suggestions
        raise NotImplementedError("Role mapping suggestions not yet implemented")
