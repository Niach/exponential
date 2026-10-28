ALTER TABLE "pull_requests" ADD COLUMN "not_run" text;--> statement-breakpoint
ALTER TABLE "session_reads" ADD COLUMN "story_seen_at" timestamp with time zone;