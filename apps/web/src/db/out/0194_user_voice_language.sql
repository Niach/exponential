ALTER TABLE "user_voice_settings" ADD COLUMN "language" varchar(8) DEFAULT 'en' NOT NULL;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD COLUMN "voice_file" varchar(512) DEFAULT '' NOT NULL;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD COLUMN "voice_transcript" varchar(500) DEFAULT '' NOT NULL;--> statement-breakpoint
ALTER TABLE "user_voice_settings" ADD CONSTRAINT "user_voice_settings_language_check" CHECK ("user_voice_settings"."language" IN ('en', 'de'));