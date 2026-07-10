ALTER TYPE "public"."notification_type" ADD VALUE 'issue_created' BEFORE 'pr_opened';--> statement-breakpoint
ALTER TABLE "workspaces" ADD COLUMN "comp_tier" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "desktop_app_card_dismissed_at" timestamp with time zone;