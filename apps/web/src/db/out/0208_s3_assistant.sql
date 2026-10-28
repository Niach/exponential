CREATE TYPE "public"."agent_role" AS ENUM('assistant', 'agent');--> statement-breakpoint
ALTER TABLE "agents" ADD COLUMN "role" "agent_role" DEFAULT 'agent' NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_agents_assistant" ON "agents" USING btree ("team_id","owner_id") WHERE role = 'assistant';--> statement-breakpoint
-- S-3: a member's voice agent they OWN becomes their Assistant (agent-settings.json `assistant`): the brand mark (icon/color null), and its triggers collapse to ONE schedule preset (any enabled schedule = the hourly default, else none; events never).
UPDATE "agents" SET
  "role" = 'assistant',
  "icon" = NULL,
  "color" = NULL,
  "triggers" = CASE
    WHEN EXISTS (
      SELECT 1 FROM jsonb_array_elements("agents"."triggers") AS t
      WHERE t->>'kind' = 'schedule' AND coalesce((t->>'enabled')::boolean, true)
    ) THEN jsonb_build_array(jsonb_build_object('id', gen_random_uuid()::text, 'enabled', true, 'kind', 'schedule', 'cron', '0 * * * *'))
    ELSE '[]'::jsonb
  END,
  "updated_at" = now()
FROM "user_voice_settings" AS v
WHERE v."agent_id" = "agents"."id" AND v."user_id" = "agents"."owner_id";
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
