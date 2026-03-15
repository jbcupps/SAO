"""Graph API calls — stub for future implementation."""


def discover_tenant() -> str:
    """Stub: Graph API tenant discovery is not yet implemented in the installer."""
    return (
        '{"status": "not_implemented", '
        '"message": "Graph API integration is planned for a future release. '
        'Post-install role alignment is handled by the SAO agent."}'
    )
