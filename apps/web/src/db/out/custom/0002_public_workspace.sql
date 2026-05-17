-- Enforce a single public workspace per instance.
-- The partial unique index only constrains rows where is_public = true,
-- so any number of rows with is_public = false are allowed.
CREATE UNIQUE INDEX IF NOT EXISTS workspaces_single_public
  ON workspaces ((is_public))
  WHERE is_public = true;
