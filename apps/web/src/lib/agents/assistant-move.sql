-- #EXP-1986 (agent-settings.json `assistant.move`, #EXP-1984): ONE Assistant
-- per member, in their hidden personal team. For every member with an
-- Assistant row in a regular team:
--   * their personal team is created (slug = `personal-` + sha256(owner)[:24],
--     lib/agents/assistant.ts personalTeamSlug);
--   * THE KEPT CONVERSATION = the most recently active conversation root of
--     any of their Assistants (last_activity_at, else created_at; ties to the
--     lower id; `assistantMoveOf`, `keepCases`); its Assistant is kept (none:
--     their newest Assistant row);
--   * the kept root MOVES into the personal team with the same ids (tree key,
--     wraps, messages and every chat store stay readable) and becomes private
--     with its whole tree; the wraps of installs that no longer read it go;
--   * every other conversation root of theirs ENDS, detached from its agent,
--     readable in its team as before;
--   * every reference to their other Assistants points to the kept one; their
--     skill and MCP picks carry into it (#EXP-1988); those rows go; the kept
--     one moves. Its prompt is NOT re-stamped (`instructions_updated_at`):
--     a conversation takes the current prompt with its brief on every launch,
--     and a stamp only puts the Instructions changed card on every moved chat.
-- NOT a migration: the release shepherd switches it on (`ASSISTANT_MOVE=on`,
-- lib/agents/assistant-move.ts) once the clients that work across it are
-- released; every boot then runs it in one transaction under a lock.
-- Idempotent: a member whose Assistants all live in their personal team is
-- untouched, so a pass with nobody left to move changes nothing.
CREATE TABLE "exp1986_movers" AS
SELECT DISTINCT a."owner_id"
FROM "agents" a JOIN "teams" t ON t."id" = a."team_id"
WHERE a."role" = 'assistant' AND t."kind" <> 'personal';
--> statement-breakpoint
INSERT INTO "teams" ("name", "slug", "kind", "owner_id")
SELECT 'Personal', 'personal-' || left(encode(sha256(convert_to(m."owner_id", 'UTF8')), 'hex'), 24), 'personal', m."owner_id"
FROM "exp1986_movers" m
ON CONFLICT DO NOTHING;
--> statement-breakpoint
CREATE TABLE "exp1986_roots" AS
SELECT s."id", s."agent_id", a."owner_id",
  row_number() OVER (
    PARTITION BY a."owner_id"
    ORDER BY coalesce(s."last_activity_at", s."created_at") DESC, s."id"
  ) = 1 AS "keep"
FROM "sessions" s
JOIN "agents" a ON a."id" = s."agent_id"
WHERE s."parent_id" IS NULL AND a."role" = 'assistant'
  AND a."owner_id" IN (SELECT "owner_id" FROM "exp1986_movers");
--> statement-breakpoint
CREATE TABLE "exp1986_assistants" AS
SELECT a."id" AS "old_id", a."owner_id", coalesce(
  (SELECT r."agent_id" FROM "exp1986_roots" r WHERE r."owner_id" = a."owner_id" AND r."keep"),
  first_value(a."id") OVER (PARTITION BY a."owner_id" ORDER BY a."created_at" DESC, a."id")
) AS "keep_id",
(SELECT p."id" FROM "teams" p WHERE p."owner_id" = a."owner_id" AND p."kind" = 'personal') AS "personal_id"
FROM "agents" a
WHERE a."role" = 'assistant' AND a."owner_id" IN (SELECT "owner_id" FROM "exp1986_movers");
--> statement-breakpoint
-- The other conversations end, detached (no cascade takes them with a row).
UPDATE "sessions" SET "status" = 'ended', "ended_at" = coalesce("ended_at", now()), "busy" = false, "background" = false, "needs_input" = false, "blocked" = NULL, "updated_at" = now()
WHERE "root_id" IN (SELECT "id" FROM "exp1986_roots" WHERE NOT "keep") AND "status" <> 'ended';
--> statement-breakpoint
UPDATE "sessions" SET "agent_id" = NULL
WHERE "root_id" IN (SELECT "id" FROM "exp1986_roots" WHERE NOT "keep");
--> statement-breakpoint
UPDATE "sessions" SET "agent_id" = NULL
WHERE "agent_id" IN (SELECT "old_id" FROM "exp1986_assistants" WHERE "old_id" <> "keep_id");
--> statement-breakpoint
-- The kept conversation moves (same ids) and its tree turns private.
UPDATE "sessions" s SET "team_id" = m."personal_id", "updated_at" = now()
FROM "exp1986_roots" r JOIN "exp1986_assistants" m ON m."old_id" = r."agent_id"
WHERE r."keep" AND s."id" = r."id";
--> statement-breakpoint
UPDATE "sessions" SET "shared" = false
WHERE "root_id" IN (SELECT "id" FROM "exp1986_roots" WHERE "keep") AND "shared";
--> statement-breakpoint
UPDATE "conversations" c SET "team_id" = s."team_id", "shared" = false
FROM "sessions" s
WHERE s."id" IN (SELECT "id" FROM "exp1986_roots" WHERE "keep") AND c."session_id" = s."id";
--> statement-breakpoint
UPDATE "conversations" SET "shared" = false
WHERE "shared" AND "session_id" IN (SELECT "id" FROM "sessions" WHERE "root_id" IN (SELECT "id" FROM "exp1986_roots" WHERE "keep"));
--> statement-breakpoint
UPDATE "messages" m SET "team_id" = s."team_id"
FROM "sessions" s
WHERE s."id" IN (SELECT "id" FROM "exp1986_roots" WHERE "keep") AND m."session_id" = s."id" AND m."team_id" <> s."team_id";
--> statement-breakpoint
UPDATE "messages" SET "shared" = false
WHERE "shared" AND "root_id" IN (SELECT "id" FROM "exp1986_roots" WHERE "keep");
--> statement-breakpoint
UPDATE "session_issues" SET "shared" = false
WHERE "shared" AND "session_id" IN (SELECT "id" FROM "sessions" WHERE "root_id" IN (SELECT "id" FROM "exp1986_roots" WHERE "keep"));
--> statement-breakpoint
UPDATE "attachments" a SET "team_id" = s."team_id"
FROM "sessions" s
WHERE s."id" IN (SELECT "id" FROM "exp1986_roots" WHERE "keep") AND a."session_id" = s."id" AND a."team_id" <> s."team_id";
--> statement-breakpoint
-- The readers of a private tree (sealed-content.json): its starter's installs
-- and the installs of a host device of one of its sessions (a member where
-- that session runs). Every other wrap goes; the next key rotates.
WITH "dropped" AS (
  DELETE FROM "session_keys" k
  USING "exp1986_roots" r, "installs" i
  WHERE r."keep" AND k."root_id" = r."id" AND i."id" = k."install_id"
    AND NOT (i."user_id" = r."owner_id" AND i."revoked_at" IS NULL)
    AND NOT EXISTS (
      SELECT 1 FROM "sessions" s
      JOIN "devices" d ON d."device_id" = s."device_id" AND d."id" = i."device_id" AND d."user_id" = i."user_id"
      WHERE s."root_id" = r."id" AND i."revoked_at" IS NULL
        AND (d."user_id" = s."user_id" OR s."team_id" = ANY(d."shared_team_ids"))
        AND EXISTS (SELECT 1 FROM "team_members" tm WHERE tm."team_id" = s."team_id" AND tm."user_id" = d."user_id")
    )
  RETURNING k."root_id"
)
UPDATE "sessions" SET "seal_rotate_epoch" = "seal_epoch"
WHERE "id" IN (SELECT "root_id" FROM "dropped") AND "seal_rotate_epoch" IS NULL AND "seal_epoch" IS NOT NULL;
--> statement-breakpoint
UPDATE "issues" i SET "assignee_agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE i."assignee_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "issues" i SET "creator_agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE i."creator_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "issues" i SET "deleted_by_agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE i."deleted_by_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "issue_drafts" d SET "assignee_agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE d."assignee_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "comments" c SET "author_agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE c."author_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "issue_events" e SET "actor_agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE e."actor_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "sessions" s SET "started_by_agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE s."started_by_agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "teams" t SET "lead_assistant_id" = m."keep_id" FROM "exp1986_assistants" m WHERE t."lead_assistant_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "apps" p SET "agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE p."agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
UPDATE "user_voice_settings" v SET "agent_id" = m."keep_id" FROM "exp1986_assistants" m WHERE v."agent_id" = m."old_id" AND m."old_id" <> m."keep_id";
--> statement-breakpoint
-- #EXP-1988 (`assistant.teamPicks`): every Assistant's skill and MCP picks
-- carry into the kept one before its rows go: the kept list first, then the
-- others' in their creation order, deduped. Each id still names its team's
-- row, so every team keeps its picks; the kept conversation's own pick
-- takes the same additions after its own.
CREATE TABLE "exp1988_picks" AS
SELECT m."keep_id", k."kind", e."value" AS "pick", row_number() OVER (
  PARTITION BY m."keep_id", k."kind"
  ORDER BY (o."id" = m."keep_id") DESC, o."created_at", o."id", e."ordinality"
) AS "ord"
FROM "exp1986_assistants" m
JOIN "agents" o ON o."id" = m."old_id"
CROSS JOIN LATERAL (VALUES ('skill', o."skill_ids"), ('mcp', o."mcp_server_ids")) AS k("kind", "ids")
CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(k."ids") = 'array' THEN k."ids" ELSE '[]'::jsonb END) WITH ORDINALITY AS e("value", "ordinality")
WHERE m."keep_id" IN (SELECT "keep_id" FROM "exp1986_assistants" WHERE "old_id" <> "keep_id");
--> statement-breakpoint
UPDATE "agents" a SET
  "skill_ids" = coalesce((
    SELECT jsonb_agg(p."pick" ORDER BY p."first")
    FROM (SELECT "pick", min("ord") AS "first" FROM "exp1988_picks" WHERE "keep_id" = a."id" AND "kind" = 'skill' GROUP BY "pick") p
  ), '[]'::jsonb),
  "mcp_server_ids" = coalesce((
    SELECT jsonb_agg(p."pick" ORDER BY p."first")
    FROM (SELECT "pick", min("ord") AS "first" FROM "exp1988_picks" WHERE "keep_id" = a."id" AND "kind" = 'mcp' GROUP BY "pick") p
  ), '[]'::jsonb),
  "updated_at" = now()
WHERE a."id" IN (SELECT "keep_id" FROM "exp1988_picks");
--> statement-breakpoint
UPDATE "sessions" s SET
  "skill_ids" = (
    SELECT jsonb_agg(u."pick" ORDER BY u."src", u."ord")
    FROM (
      SELECT DISTINCT ON (q."pick") q."pick", q."src", q."ord"
      FROM (
        SELECT e."value" AS "pick", 0 AS "src", e."ordinality" AS "ord"
        FROM jsonb_array_elements(CASE WHEN jsonb_typeof(s."skill_ids") = 'array' THEN s."skill_ids" ELSE '[]'::jsonb END) WITH ORDINALITY AS e("value", "ordinality")
        UNION ALL
        SELECT p."pick", 1, p."ord" FROM "exp1988_picks" p WHERE p."keep_id" = r."agent_id" AND p."kind" = 'skill'
      ) q
      ORDER BY q."pick", q."src", q."ord"
    ) u
  ),
  "mcp_server_ids" = (
    SELECT jsonb_agg(u."pick" ORDER BY u."src", u."ord")
    FROM (
      SELECT DISTINCT ON (q."pick") q."pick", q."src", q."ord"
      FROM (
        SELECT e."value" AS "pick", 0 AS "src", e."ordinality" AS "ord"
        FROM jsonb_array_elements(CASE WHEN jsonb_typeof(s."mcp_server_ids") = 'array' THEN s."mcp_server_ids" ELSE '[]'::jsonb END) WITH ORDINALITY AS e("value", "ordinality")
        UNION ALL
        SELECT p."pick", 1, p."ord" FROM "exp1988_picks" p WHERE p."keep_id" = r."agent_id" AND p."kind" = 'mcp'
      ) q
      ORDER BY q."pick", q."src", q."ord"
    ) u
  ),
  "updated_at" = now()
FROM "exp1986_roots" r
WHERE r."keep" AND s."id" = r."id" AND r."agent_id" IN (SELECT "keep_id" FROM "exp1988_picks");
--> statement-breakpoint
DELETE FROM "agents" WHERE "id" IN (SELECT "old_id" FROM "exp1986_assistants" WHERE "old_id" <> "keep_id");
--> statement-breakpoint
-- The kept Assistant moves; its skill / MCP picks stay (ids of their teams'
-- rows, read per team: #EXP-1988); its prompt stamp stays.
UPDATE "agents" a SET "team_id" = m."personal_id", "updated_at" = now()
FROM "exp1986_assistants" m
WHERE a."id" = m."keep_id" AND m."old_id" = m."keep_id";
--> statement-breakpoint
DROP TABLE "exp1988_picks";
--> statement-breakpoint
DROP TABLE "exp1986_assistants";
--> statement-breakpoint
DROP TABLE "exp1986_roots";
--> statement-breakpoint
DROP TABLE "exp1986_movers";
