# SAO Installer Specification

This document captures the repo-local contract for the standalone SAO bootstrap installer.

## Inputs

Required:

- `ANTHROPIC_API_KEY`
- Azure account access sufficient to create the SAO resource group and resources inside it

Optional installer environment variables:

- `SAO_INSTALLER_FRONTEND_URL`
- `SAO_INSTALLER_ALLOWED_ORIGINS`
- `SAO_INSTALLER_JWT_SECRET`
- `SAO_INSTALLER_OIDC_ISSUER_URL`
- `SAO_INSTALLER_OIDC_CLIENT_ID`
- `SAO_INSTALLER_OIDC_CLIENT_SECRET`
- `SAO_INSTALLER_OIDC_PROVIDER_NAME`
- `SAO_INSTALLER_OIDC_SCOPES`

## Azure Resources

The current Bicep contract provisions:

- a virtual network with dedicated Container Apps and PostgreSQL subnets
- a private DNS zone for PostgreSQL private access
- PostgreSQL Flexible Server plus the `sao` database
- a Key Vault using RBAC instead of inline access policies
- a Log Analytics workspace
- a Storage Account and Azure Files share for `/data/sao`
- a Container Apps environment and the SAO application container

## Runtime Configuration Seeded At Deploy Time

- `SAO_DATA_DIR=/data/sao`
- `SAO_FRONTEND_URL`
- `SAO_ALLOWED_ORIGINS`
- `SAO_COOKIE_SECURE=true`
- `SAO_BOOTSTRAP_ADMIN_OID`
- `SAO_RP_ID`
- `SAO_RP_ORIGIN`
- optional `SAO_JWT_SECRET`
- optional env-backed OIDC provider values

## Success Criteria

A deployment is only complete when:

1. ARM reports success.
2. The latest Container App revision is healthy.
3. `/api/health` responds successfully.
4. The installer prints the SAO URL and bootstrap admin identity.
