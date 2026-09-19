-- EXP-972 item 1: `coding_sessions.batch_issue_ids` (EXP-876) names a batch
-- run on every client. Rows started before the column existed, or by a client
-- too old to send it, carried NULL and every client named them off the
-- issues sharing their `exp/batch-<id8>` branch (what `pr_open` stamped on
-- both sides, EXP-545). Every version floor sends the column now and the
-- client-side branch fallback is gone, so this is the one-off heal: store
-- what the fallback resolved, oldest issue first (its order: `created_at`,
-- identifiers breaking the tie). Data-only, no schema change.
--
-- Idempotent: only NULL columns on issue-less, action-less batch rows are
-- touched, and a batch whose branch no issue carries (no PR yet, or ever)
-- matches nothing and keeps reading "Batch run".
UPDATE "coding_sessions" cs
SET "batch_issue_ids" = mates.ids
FROM (
  SELECT cs2."id", jsonb_agg(i."id" ORDER BY i."created_at", i."identifier") AS ids
  FROM "coding_sessions" cs2
  JOIN "issues" i ON i."branch" = cs2."branch" AND i."team_id" = cs2."team_id"
  WHERE cs2."issue_id" IS NULL AND cs2."action_name" IS NULL
    AND cs2."batch_issue_ids" IS NULL AND cs2."branch" LIKE 'exp/batch-%'
  GROUP BY cs2."id"
) mates
WHERE cs."id" = mates."id";
