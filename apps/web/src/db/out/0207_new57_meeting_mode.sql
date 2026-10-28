ALTER TABLE "messages" ADD COLUMN "expires_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD COLUMN "desktop_audio" varchar(16) DEFAULT 'off' NOT NULL;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD COLUMN "desktop_audio_apps" jsonb DEFAULT '[]'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD COLUMN "meeting_started_at" timestamp with time zone;--> statement-breakpoint
CREATE INDEX "idx_messages_expires" ON "messages" USING btree ("expires_at") WHERE expires_at IS NOT NULL;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD CONSTRAINT "user_voice_settings_desktop_audio_check" CHECK ("user_voice_settings"."desktop_audio" IN ('off', 'all', 'include', 'exclude'));