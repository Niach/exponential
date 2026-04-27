ALTER TABLE "issues" ADD COLUMN "google_calendar_event_id" varchar(1024);--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "google_calendar_last_synced_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "google_calendar_last_sync_error" text;