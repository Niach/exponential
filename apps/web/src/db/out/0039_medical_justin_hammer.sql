CREATE TYPE "public"."issue_source" AS ENUM('user', 'widget');--> statement-breakpoint
ALTER TABLE "issues" DROP CONSTRAINT "issues_creator_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "widget_configs" DROP CONSTRAINT "widget_configs_widget_user_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "attachments" ALTER COLUMN "uploader_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "issues" ALTER COLUMN "creator_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "source" "issue_source" DEFAULT 'user' NOT NULL;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_creator_id_users_id_fk" FOREIGN KEY ("creator_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
-- ---------------------------------------------------------------------------
-- Data backfill: retire the synthetic per-widget bot users (users.is_agent).
-- Runs BEFORE the is_agent / widget_user_id columns are dropped, while the
-- synthetic users are still identifiable. Every statement is a no-op on a
-- database that has no synthetic users, so it is safe on prod (which never
-- created any if it predates the widget feature) and on the dogfood cloud.
--
-- Order matters:
--   1. Stamp source='widget' on issues authored by a bot (before we detach
--      them — after nulling creator_id there is no way to tell them apart).
--   2. Detach those issues from the bot creator (the new FK is ON DELETE SET
--      NULL, but set it explicitly so source + creator move together).
--   3. Null the uploader of widget screenshot attachments (attachments
--      .uploader_id is ON DELETE CASCADE — nulling first preserves the blob
--      row when the bot is deleted).
--   4. Remove the bots' team memberships, then the bot users themselves. The
--      widget_configs.widget_user_id restrict FK was dropped above, so nothing
--      pins them anymore.
-- ---------------------------------------------------------------------------
UPDATE "issues" SET "source" = 'widget'
  WHERE "creator_id" IN (SELECT "id" FROM "users" WHERE "is_agent" = true);--> statement-breakpoint
UPDATE "issues" SET "creator_id" = NULL
  WHERE "creator_id" IN (SELECT "id" FROM "users" WHERE "is_agent" = true);--> statement-breakpoint
UPDATE "attachments" SET "uploader_id" = NULL
  WHERE "uploader_id" IN (SELECT "id" FROM "users" WHERE "is_agent" = true);--> statement-breakpoint
DELETE FROM "team_members"
  WHERE "user_id" IN (SELECT "id" FROM "users" WHERE "is_agent" = true);--> statement-breakpoint
DELETE FROM "users" WHERE "is_agent" = true;--> statement-breakpoint
ALTER TABLE "widget_configs" DROP COLUMN "widget_user_id";--> statement-breakpoint
ALTER TABLE "users" DROP COLUMN "is_agent";
