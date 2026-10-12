ALTER TYPE "public"."issue_event_type" ADD VALUE 'title_changed';--> statement-breakpoint
ALTER TABLE "issue_events" ADD COLUMN "via_mcp" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_events" ADD COLUMN "session_id" uuid;