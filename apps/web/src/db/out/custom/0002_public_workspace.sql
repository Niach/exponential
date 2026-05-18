-- Public workspaces are now a generic per-workspace flag, not a singleton.
-- Drop the partial unique index introduced earlier; any workspace can be public.
DROP INDEX IF EXISTS workspaces_single_public;
