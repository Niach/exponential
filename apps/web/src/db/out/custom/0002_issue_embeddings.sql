-- #NEW-134: one embedding per issue, for semantic issue search (#NEW-131).
--
-- OPTIONAL by design. This file is applied at every boot like 0001
-- (bootstrap-cloud applyCustomSql), but everything it creates needs the
-- pgvector extension, and a database whose Postgres image does not ship it
-- (stock postgres:*-alpine: every install older than this file, and any
-- self-host that keeps its image) must keep booting and migrating. So:
--
--   * nothing here is in a Drizzle migration or in @exp/db-schema: a
--     migration naming the `vector` type would fail on such a database;
--   * the whole body runs only when pg_available_extensions lists `vector`
--     at 0.8.0 or newer (HNSW since 0.5, the iterative index scan the scoped
--     search uses since 0.8);
--   * it never raises: a role that may not CREATE EXTENSION, or any other
--     failure, leaves the database without the table (the block is one
--     subtransaction, so never half of it) and the server without semantic
--     search: lexical search is untouched (lib/issue-embeddings/store.ts
--     `issueEmbeddingsAvailable`).
--
-- The first boot after the Postgres image gained pgvector creates everything;
-- every later boot is a no-op (IF NOT EXISTS throughout).
--
-- Server-only: no shape proxy, never synced, not in any publication.
DO $$
DECLARE
  available text;
BEGIN
  SELECT coalesce(installed_version, default_version) INTO available
  FROM pg_available_extensions WHERE name = 'vector';
  IF available IS NULL THEN
    RETURN;
  END IF;
  IF string_to_array(split_part(available, '-', 1), '.')::int[] < ARRAY[0, 8, 0] THEN
    RAISE NOTICE 'issue embeddings: pgvector % is older than 0.8.0, semantic issue search stays off', available;
    RETURN;
  END IF;
  BEGIN
    CREATE EXTENSION IF NOT EXISTS vector;

    CREATE TABLE IF NOT EXISTS issue_embeddings (
      issue_id uuid PRIMARY KEY REFERENCES issues (id) ON DELETE CASCADE,
      -- The issue's team at embed time: lets a team-scoped search narrow on
      -- this table alone before it joins issues for the live-board rules.
      team_id uuid NOT NULL,
      -- embedder/model.json `id`: the model, its quantization and the input
      -- recipe. Rows of another model are stale and never searched.
      model text NOT NULL,
      -- L2-normalised, so cosine distance = 1 - dot product.
      embedding vector(384) NOT NULL,
      -- sha256 of model id + the exact input text: an unchanged hash skips
      -- the inference.
      content_hash text NOT NULL,
      -- The issues.updated_at this row was made from (NOT when the row was
      -- written): the sweep re-checks an issue whose updated_at is newer.
      updated_at timestamptz NOT NULL DEFAULT now()
    );

    CREATE INDEX IF NOT EXISTS idx_issue_embeddings_hnsw
      ON issue_embeddings USING hnsw (embedding vector_cosine_ops);
    CREATE INDEX IF NOT EXISTS idx_issue_embeddings_team
      ON issue_embeddings (team_id);
  EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'issue embeddings: not set up (%), semantic issue search stays off', SQLERRM;
  END;
END $$;
