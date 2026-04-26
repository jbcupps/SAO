-- Self-serve OrionII installer staging.
--
-- An admin registers one or more installer sources (URL + expected sha256 + version).
-- When a user creates an entity, SAO downloads the *default* source (if not already cached
-- under SAO_DATA_DIR/installers/<sha>/<filename>), verifies the sha256, and pins those
-- coordinates on the agents row. Bundle download serves the pinned cached copy — admins
-- no longer need shell access to stage the MSI manually.

CREATE TABLE installer_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind TEXT NOT NULL CHECK (kind IN ('orion-msi')),
    url TEXT NOT NULL,
    filename TEXT NOT NULL,
    version TEXT NOT NULL,
    expected_sha256 TEXT NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT false,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL
);

-- At most one default per kind.
CREATE UNIQUE INDEX idx_installer_sources_one_default
    ON installer_sources (kind)
    WHERE is_default = true;

-- Per-agent installer pin so re-downloading the same agent's bundle gives the same MSI
-- and old agents keep working when the default source rolls forward.
ALTER TABLE agents ADD COLUMN installer_sha256 TEXT;
ALTER TABLE agents ADD COLUMN installer_filename TEXT;
ALTER TABLE agents ADD COLUMN installer_version TEXT;
