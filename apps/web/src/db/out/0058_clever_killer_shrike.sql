ALTER TABLE "user_notification_prefs" ADD COLUMN "digest_hour" integer DEFAULT 8 NOT NULL;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "timezone" text;