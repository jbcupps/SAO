# Bootstrap Installer

The fastest path to a live SAO environment is the standalone installer container.

## One Command

```bash
docker run --rm -it \
  -e ANTHROPIC_API_KEY=sk-ant-your-key-here \
  ghcr.io/jbcupps/sao-installer:latest
```

## What The Installer Does

- signs you into Azure with device-code auth
- checks identity and permissions before writing anything
- deploys the SAO Azure footprint
- verifies the live runtime
- prints the endpoint and bootstrap admin identity

## Advanced Bootstrap Inputs

If you already know your browser origin or Entra application details, you can pass them as installer environment variables:

- `SAO_INSTALLER_FRONTEND_URL`
- `SAO_INSTALLER_ALLOWED_ORIGINS`
- `SAO_INSTALLER_JWT_SECRET`
- `SAO_INSTALLER_OIDC_ISSUER_URL`
- `SAO_INSTALLER_OIDC_CLIENT_ID`
- `SAO_INSTALLER_OIDC_CLIENT_SECRET`
- `SAO_INSTALLER_OIDC_PROVIDER_NAME`
- `SAO_INSTALLER_OIDC_SCOPES`

These values are forwarded into the Azure deployment as explicit runtime parameters.
