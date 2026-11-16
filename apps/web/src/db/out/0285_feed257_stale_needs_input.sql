-- FEED-257: repair stale needs_input flags. A session reads "needs input" only while something waits on a person: an open question/plan card or a flag (`blocked`). (1) An undelivered `unclaimed` flag on a claimed live session whose input went away without its device (a newer pr_merge line superseded a hold notice) and that answered its first Request: cleared (the watchdog's DRAINED rule, every tick from now on). (2) A live row with needs_input but no flag and no open card: re-derived (false). (3) An ended row never needs input.
UPDATE "sessions" s SET
  "blocked" = NULL,
  "needs_input" = EXISTS (SELECT 1 FROM "messages" m WHERE m."session_id" = s."id" AND m."answered_at" IS NULL AND m."card"->>'kind' IN ('question', 'plan')),
  "updated_at" = now()
WHERE s."status" IN ('running', 'idle') AND s."started_at" IS NOT NULL AND s."subagent_key" IS NULL AND s."blocked"->>'kind' = 'unclaimed'
  AND NOT EXISTS (SELECT 1 FROM "messages" m WHERE m."session_id" = s."id" AND m."consumed_at" IS NULL
    AND (m."author_kind" = 'user' OR (m."author_session_id" IS NOT NULL AND m."author_session_id" <> s."id")))
  AND (
    NOT EXISTS (SELECT 1 FROM "messages" m WHERE m."session_id" = s."id" AND m."card" IS NULL
      AND (m."author_kind" = 'user' OR (m."author_session_id" IS NOT NULL AND m."author_session_id" <> s."id")))
    OR EXISTS (SELECT 1 FROM "messages" m WHERE m."session_id" = s."id" AND m."author_kind" = 'agent' AND (m."author_session_id" IS NULL OR m."author_session_id" = s."id"))
  );--> statement-breakpoint
UPDATE "sessions" s SET "needs_input" = false, "updated_at" = now()
WHERE s."status" <> 'ended' AND s."needs_input" = true AND s."blocked" IS NULL
  AND NOT EXISTS (SELECT 1 FROM "messages" m WHERE m."session_id" = s."id" AND m."answered_at" IS NULL AND m."card"->>'kind' IN ('question', 'plan'));--> statement-breakpoint
UPDATE "sessions" SET "needs_input" = false, "updated_at" = now() WHERE "status" = 'ended' AND "needs_input" = true;
