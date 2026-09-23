ALTER TYPE "public"."issue_event_type" ADD VALUE 'estimate_changed';--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "estimate" integer;