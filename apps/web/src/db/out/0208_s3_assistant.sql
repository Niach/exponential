CREATE TYPE "public"."agent_role" AS ENUM('assistant', 'agent');--> statement-breakpoint
ALTER TABLE "agents" ADD COLUMN "role" "agent_role" DEFAULT 'agent' NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_agents_assistant" ON "agents" USING btree ("team_id","owner_id") WHERE role = 'assistant';--> statement-breakpoint
-- S-3: a member's voice agent they OWN becomes their Assistant (agent-settings.json `assistant`) only while it is THEIRS alone: no other member has a conversation with it and it has no event trigger (else it stays a team agent; the voice steps below clear the mirror and ensureAssistant makes a fresh Assistant on first use). The brand mark (icon/color null), and its triggers collapse to ONE schedule preset: the first enabled schedule whose cron is a preset (`schedules`) keeps that cron and its tz, another enabled schedule = the hourly default, none = off.
UPDATE "agents" SET
  "role" = 'assistant',
  "icon" = NULL,
  "color" = NULL,
  "triggers" = CASE
    WHEN EXISTS (
      SELECT 1 FROM jsonb_array_elements("agents"."triggers") AS t
      WHERE t->>'kind' = 'schedule' AND coalesce((t->>'enabled')::boolean, true)
    ) THEN jsonb_build_array(coalesce(
      (
        SELECT jsonb_strip_nulls(jsonb_build_object('id', gen_random_uuid()::text, 'enabled', true, 'kind', 'schedule', 'cron', t->>'cron', 'tz', t->>'tz'))
        FROM jsonb_array_elements("agents"."triggers") WITH ORDINALITY AS e(t, n)
        WHERE t->>'kind' = 'schedule' AND coalesce((t->>'enabled')::boolean, true)
          AND t->>'cron' IN ('*/15 * * * *', '*/30 * * * *', '0 * * * *', '0 9 * * *')
        ORDER BY n
        LIMIT 1
      ),
      jsonb_build_object('id', gen_random_uuid()::text, 'enabled', true, 'kind', 'schedule', 'cron', '0 * * * *')
    ))
    ELSE '[]'::jsonb
  END,
  "updated_at" = now()
FROM "user_voice_settings" AS v
WHERE v."agent_id" = "agents"."id" AND v."user_id" = "agents"."owner_id"
  AND NOT EXISTS (
    SELECT 1 FROM "sessions" AS s WHERE s."agent_id" = "agents"."id" AND s."user_id" <> "agents"."owner_id"
  )
  AND NOT EXISTS (
    SELECT 1 FROM jsonb_array_elements("agents"."triggers") AS t WHERE t->>'kind' = 'event'
  );
--> statement-breakpoint
-- S-3: the voice device IS the Assistant's device (user_voice_settings.voice_device_id / agent_id = its server-kept mirror): the Assistant's device when it is the member's own desktop, else none.
UPDATE "user_voice_settings" AS v SET
  "voice_device_id" = CASE WHEN EXISTS (
    SELECT 1 FROM "devices" AS d WHERE d."user_id" = v."user_id" AND d."device_id" = a."device_id" AND d."kind" = 'desktop'
  ) THEN a."device_id" ELSE NULL END,
  "updated_at" = now()
FROM "agents" AS a
WHERE a."id" = v."agent_id" AND a."role" = 'assistant' AND a."owner_id" = v."user_id";
--> statement-breakpoint
-- A voice agent that did not become the member's Assistant, or an Assistant off a desktop: no voice (voice.json `clearOnNone`).
UPDATE "user_voice_settings" SET
  "speak_replies" = false,
  "push_to_talk" = false,
  "wake_word" = false,
  "meeting_started_at" = NULL,
  "updated_at" = now()
WHERE "voice_device_id" IS NULL
  OR "agent_id" IS NULL
  OR "agent_id" NOT IN (SELECT "id" FROM "agents" WHERE "role" = 'assistant' AND "owner_id" = "user_voice_settings"."user_id");
--> statement-breakpoint
UPDATE "user_voice_settings" SET "agent_id" = NULL, "voice_device_id" = NULL
WHERE "agent_id" IS NOT NULL
  AND "agent_id" NOT IN (SELECT "id" FROM "agents" WHERE "role" = 'assistant' AND "owner_id" = "user_voice_settings"."user_id");
