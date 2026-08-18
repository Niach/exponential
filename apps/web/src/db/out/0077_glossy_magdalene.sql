ALTER TYPE "public"."issue_event_type" ADD VALUE IF NOT EXISTS 'created';--> statement-breakpoint
ALTER TYPE "public"."issue_event_type" ADD VALUE IF NOT EXISTS 'priority_changed';--> statement-breakpoint
ALTER TABLE "actions" ADD COLUMN "trigger" jsonb;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "started_reason" varchar(16);