-- #EXP-1849 (ONE root per app and member): an app's editor root IS the
-- member's conversation with the app's agent. Three steps, idempotent:
-- 1. A member who already holds a separate conversation root with an app's
--    agent (a hand-off or a trigger reached it) AND an editor root of that
--    app keeps the older conversation as a plain chat (agent_id cleared), so
--    the editor root can take the agent (uniq_sessions_agent_conversation).
UPDATE "sessions" AS c
SET "agent_id" = NULL, "updated_at" = now()
FROM "apps" AS a
WHERE a."agent_id" IS NOT NULL
  AND c."agent_id" = a."agent_id"
  AND c."parent_id" IS NULL
  AND c."app_id" IS NULL
  AND EXISTS (
    SELECT 1 FROM "sessions" AS e
    WHERE e."app_id" = a."id" AND e."user_id" = c."user_id" AND e."parent_id" IS NULL
  );
--> statement-breakpoint
-- 2. Every editor root takes its app's agent.
UPDATE "sessions" AS e
SET "agent_id" = a."agent_id", "updated_at" = now()
FROM "apps" AS a
WHERE e."app_id" = a."id"
  AND e."parent_id" IS NULL
  AND e."agent_id" IS NULL
  AND a."agent_id" IS NOT NULL;
--> statement-breakpoint
-- 3. The app agent's device is the app's AGENT SLOT: only a device shared
--    with the team; anything else (the creator's own device, the default
--    of #EXP-1730) empties it = each member's own device.
UPDATE "agents" AS g
SET "device_id" = '', "defaults" = g."defaults" || '{"deviceId": null}'::jsonb, "updated_at" = now()
FROM "apps" AS a
WHERE a."agent_id" = g."id"
  AND g."device_id" <> ''
  AND NOT EXISTS (
    SELECT 1 FROM "devices" AS d
    WHERE d."device_id" = g."device_id" AND g."team_id" = ANY (d."shared_team_ids")
  );
