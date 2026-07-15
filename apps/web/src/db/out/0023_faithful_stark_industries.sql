-- Historical release timeline events must go BEFORE the enum recreation below
-- (the USING cast would fail on rows holding the removed values).
DELETE FROM "issue_events" WHERE "type" IN ('release_added', 'release_removed');--> statement-breakpoint
ALTER TABLE "releases" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
DROP TABLE "releases" CASCADE;--> statement-breakpoint
ALTER TABLE "coding_sessions" DROP CONSTRAINT "coding_sessions_release_id_releases_id_fk";
--> statement-breakpoint
ALTER TABLE "issues" DROP CONSTRAINT "issues_release_id_releases_id_fk";
--> statement-breakpoint
ALTER TABLE "issue_events" ALTER COLUMN "type" SET DATA TYPE text;--> statement-breakpoint
DROP TYPE "public"."issue_event_type";--> statement-breakpoint
CREATE TYPE "public"."issue_event_type" AS ENUM('status_changed', 'assignee_changed', 'label_added', 'label_removed', 'pr_opened', 'pr_merged', 'project_moved');--> statement-breakpoint
ALTER TABLE "issue_events" ALTER COLUMN "type" SET DATA TYPE "public"."issue_event_type" USING "type"::"public"."issue_event_type";--> statement-breakpoint
DROP INDEX "idx_coding_sessions_release";--> statement-breakpoint
DROP INDEX "idx_issues_release";--> statement-breakpoint
ALTER TABLE "coding_sessions" DROP COLUMN "release_id";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "release_id";