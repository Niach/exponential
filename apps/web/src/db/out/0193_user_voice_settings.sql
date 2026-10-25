CREATE TABLE "user_voice_settings" (
	"user_id" text PRIMARY KEY NOT NULL,
	"voice_device_id" varchar(128),
	"agent_id" uuid,
	"speak_replies" boolean DEFAULT false NOT NULL,
	"push_to_talk" boolean DEFAULT false NOT NULL,
	"ptt_shortcut" varchar(32) DEFAULT 'ctrl+space' NOT NULL,
	"wake_word" boolean DEFAULT false NOT NULL,
	"wake_phrase" varchar(64) DEFAULT '' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD CONSTRAINT "user_voice_settings_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD CONSTRAINT "user_voice_settings_agent_id_agents_id_fk" FOREIGN KEY ("agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
-- NEW-67: carry NEW-53's per-device voice setting over. For each member the
-- most recently saved device with speakReplies on becomes the voice device;
-- its assistant becomes the agent when that agent still exists. Only
-- desktops can run the voice.
INSERT INTO "user_voice_settings" ("user_id", "voice_device_id", "agent_id", "speak_replies")
SELECT DISTINCT ON (d."user_id")
	d."user_id",
	d."device_id",
	a."id",
	(a."id" IS NOT NULL)
FROM "devices" d
LEFT JOIN "agents" a ON a."id"::text = d."launch_defaults"->'voice'->>'assistantAgentId'
WHERE d."kind" = 'desktop'
	AND d."launch_defaults"->'voice'->>'speakReplies' = 'true'
ORDER BY d."user_id", d."launch_defaults_updated_at" DESC NULLS LAST
ON CONFLICT ("user_id") DO NOTHING;--> statement-breakpoint
-- The key is retired: clients stop sending it, the clamp drops it.
UPDATE "devices" SET "launch_defaults" = "launch_defaults" - 'voice'
WHERE "launch_defaults" ? 'voice';
