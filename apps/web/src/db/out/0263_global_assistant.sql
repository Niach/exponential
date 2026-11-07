-- #EXP-1720 (agent-settings.json `assistant`): ONE Assistant per member,
-- in their hidden personal team. Each member's NEWEST per-team Assistant
-- (its conversation's latest activity, else the newest row) becomes the
-- global one and moves into a new personal team; every old conversation
-- ENDS and stays readable in its team as the member's private ended thread
-- (detached from the agent, so no cascade deletes it); every reference to
-- the member's other Assistants (assignees, actors, authors, started-by,
-- the voice mirror) is re-pointed to the global one, and those rows go.
-- The global Assistant's next message opens a fresh conversation in the
-- personal team. The personal team's slug = `personal-` + sha256(owner)[:24]
-- (lib/agents/assistant.ts personalTeamSlug).
INSERT INTO "teams" ("name", "slug", "kind", "owner_id")
SELECT DISTINCT 'Personal', 'personal-' || left(encode(sha256(convert_to(a."owner_id", 'UTF8')), 'hex'), 24), 'personal', a."owner_id"
FROM "agents" a
WHERE a."role" = 'assistant'
ON CONFLICT DO NOTHING;
--> statement-breakpoint
CREATE TABLE "exp1720_assistants" AS
SELECT a."id" AS "old_id", a."owner_id", first_value(a."id") OVER (
  PARTITION BY a."owner_id"
  ORDER BY c."at" DESC NULLS LAST, a."created_at" DESC, a."id"
) AS "keep_id"
FROM "agents" a
LEFT JOIN LATERAL (
  SELECT max(coalesce(s."last_activity_at", s."created_at")) AS "at"
  FROM "sessions" s
  WHERE s."agent_id" = a."id" AND s."parent_id" IS NULL
) c ON true
WHERE a."role" = 'assistant';
--> statement-breakpoint
CREATE TABLE "exp1720_roots" AS
SELECT s."id" FROM "sessions" s
WHERE s."parent_id" IS NULL AND s."agent_id" IN (SELECT "old_id" FROM "exp1720_assistants");
--> statement-breakpoint
UPDATE "sessions" SET "status" = 'ended', "ended_at" = coalesce("ended_at", now()), "busy" = false, "background" = false, "needs_input" = false, "blocked" = NULL
WHERE "root_id" IN (SELECT "id" FROM "exp1720_roots") AND "status" <> 'ended';
--> statement-breakpoint
UPDATE "sessions" SET "agent_id" = NULL WHERE "id" IN (SELECT "id" FROM "exp1720_roots");
--> statement-breakpoint
UPDATE "issues" i SET "assignee_agent_id" = m."keep_id" FROM "exp1720_assistants" m WHERE i."assignee_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "issues" i SET "creator_agent_id" = m."keep_id" FROM "exp1720_assistants" m WHERE i."creator_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "issue_drafts" d SET "assignee_agent_id" = m."keep_id" FROM "exp1720_assistants" m WHERE d."assignee_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "comments" c SET "author_agent_id" = m."keep_id" FROM "exp1720_assistants" m WHERE c."author_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "issue_events" e SET "actor_agent_id" = m."keep_id" FROM "exp1720_assistants" m WHERE e."actor_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "sessions" s SET "started_by_agent_id" = m."keep_id" FROM "exp1720_assistants" m WHERE s."started_by_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "user_voice_settings" v SET "agent_id" = m."keep_id" FROM "exp1720_assistants" m WHERE v."agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
DELETE FROM "agents" WHERE "id" IN (SELECT "old_id" FROM "exp1720_assistants" WHERE "old_id" <> "keep_id");
--> statement-breakpoint
UPDATE "agents" a SET "team_id" = t."id", "instructions_updated_at" = now()
FROM "teams" t
WHERE a."id" IN (SELECT DISTINCT "keep_id" FROM "exp1720_assistants") AND t."owner_id" = a."owner_id" AND t."kind" = 'personal';
--> statement-breakpoint
DROP TABLE "exp1720_roots";
--> statement-breakpoint
DROP TABLE "exp1720_assistants";
