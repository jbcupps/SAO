"""PostgreSQL setup tools for the SAO installer agent.

Handles database connectivity verification, migration execution,
and initial data seeding during bootstrap.
"""

from dataclasses import dataclass
from typing import Any


@dataclass
class PostgresTools:
    """Tools for PostgreSQL setup and validation.

    These tools allow the installer agent to:
    - Verify database connectivity
    - Run migrations
    - Seed initial data (admin user, installer state)
    - Check for existing state (idempotent resumption)

    Args:
        database_url: PostgreSQL connection string.
    """

    database_url: str

    async def check_connectivity(self) -> dict[str, Any]:
        """Verify PostgreSQL connectivity and version.

        Returns:
            Dict with 'connected' (bool), 'version' (str),
            and 'database' (str) fields.
        """
        # TODO: Connect to PostgreSQL and run SELECT version()
        # Use psycopg async connection
        raise NotImplementedError("Database connectivity check not yet implemented")

    async def check_migrations(self) -> dict[str, Any]:
        """Check migration status.

        Returns:
            Dict with 'pending' (int count), 'applied' (int count),
            and 'latest' (str migration name) fields.
        """
        # TODO: Query _sqlx_migrations table to check status
        raise NotImplementedError("Migration status check not yet implemented")

    async def run_migrations(self) -> dict[str, Any]:
        """Run pending database migrations.

        Executes any unapplied migrations from the migrations/ directory.

        Returns:
            Dict with 'applied' (list of migration names) and 'status'.
        """
        # TODO: Execute pending sqlx migrations
        # Could call SAO server API or run directly
        raise NotImplementedError("Migration execution not yet implemented")

    async def seed_admin(self, oid: str, name: str, email: str) -> dict[str, Any]:
        """Seed the initial admin user from Entra OID.

        Creates the first admin record in the users table using
        the authenticated Entra Object ID. This is the root
        identity for the SAO installation.

        Args:
            oid: Entra Object ID of the admin.
            name: Display name from Entra profile.
            email: Email from Entra profile.

        Returns:
            Dict with 'user_id' and 'role'.
        """
        # TODO: INSERT into users table with role='administrator'
        # Check for existing user first (idempotent)
        raise NotImplementedError("Admin seeding not yet implemented")

    async def check_existing_state(self) -> dict[str, Any]:
        """Check for existing installer state (idempotent resumption).

        Queries the database for any previous installer progress
        so the agent can resume from where it left off.

        Returns:
            Dict with installer state fields, or empty dict if
            no previous state exists.
        """
        # TODO: Query installer_state table
        raise NotImplementedError("State check not yet implemented")

    async def has_users(self) -> bool:
        """Check if any users exist in the database.

        Used to determine if SAO should enter installer mode
        or operational mode.

        Returns:
            True if users exist, False if database is empty.
        """
        # TODO: SELECT COUNT(*) FROM users
        raise NotImplementedError("User check not yet implemented")
