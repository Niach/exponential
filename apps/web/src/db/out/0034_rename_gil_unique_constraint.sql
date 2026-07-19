-- EXP-180 follow-up: the catalog rename loop in 0032 renamed `workspace` →
-- `team` inside constraint names, but `github_installation_links`' unique
-- constraint was CREATED under Postgres' 63-char identifier truncation
-- (…_workspace_id_github_installation_id_u), so the rename produced
-- …_team_id_github_installation_id_u while a schema-derived DB names it
-- …_team_id_github_installation_id_unique (63 chars — now fits). Rename to
-- converge with packages/db-schema so the CI drift check passes; no-op on
-- fresh installs that never had the truncated name.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conname = 'github_installation_links_team_id_github_installation_id_u'
      AND conrelid = 'public.github_installation_links'::regclass
  ) THEN
    ALTER TABLE "github_installation_links"
      RENAME CONSTRAINT "github_installation_links_team_id_github_installation_id_u"
      TO "github_installation_links_team_id_github_installation_id_unique";
  END IF;
END $$;
