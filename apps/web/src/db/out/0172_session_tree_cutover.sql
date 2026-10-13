ALTER TABLE "coding_sessions" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
ALTER TABLE "session_attachments" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
ALTER TABLE "thread_items" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
ALTER TABLE "threads" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
DROP TABLE "coding_sessions" CASCADE;--> statement-breakpoint
DROP TABLE "session_attachments" CASCADE;--> statement-breakpoint
DROP TABLE "thread_items" CASCADE;--> statement-breakpoint
DROP TABLE "threads" CASCADE;--> statement-breakpoint
ALTER TABLE "actions" DROP CONSTRAINT "actions_repository_id_repositories_id_fk";
--> statement-breakpoint
ALTER TABLE "issue_drafts" DROP CONSTRAINT "issue_drafts_thread_id_threads_id_fk";
--> statement-breakpoint
ALTER TABLE "notifications" DROP CONSTRAINT "notifications_session_id_coding_sessions_id_fk";
--> statement-breakpoint
ALTER TABLE "pins" DROP CONSTRAINT "pins_session_id_coding_sessions_id_fk";
--> statement-breakpoint
DROP INDEX "idx_issues_pr_url";--> statement-breakpoint
DROP INDEX "idx_issues_pr_base_branch";--> statement-breakpoint
-- EXP-1291 hard cutover: rows pointing at the dropped coding_sessions ids
-- would violate the new sessions FKs.
DELETE FROM "pins" WHERE "session_id" IS NOT NULL;--> statement-breakpoint
UPDATE "notifications" SET "session_id" = NULL WHERE "session_id" IS NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD COLUMN "session_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "notifications" ADD CONSTRAINT "notifications_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pins" ADD CONSTRAINT "pins_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "actions" DROP COLUMN "repository_id";--> statement-breakpoint
ALTER TABLE "actions" DROP COLUMN "description";--> statement-breakpoint
ALTER TABLE "actions" DROP COLUMN "triggers";--> statement-breakpoint
ALTER TABLE "actions" DROP COLUMN "mcp_server_ids";--> statement-breakpoint
ALTER TABLE "issue_drafts" DROP COLUMN "thread_id";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "pr_url";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "pr_number";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "pr_state";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "branch";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "pr_merged_at";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "pr_base_branch";--> statement-breakpoint
DROP TYPE "public"."coding_session_status";--> statement-breakpoint
DROP TYPE "public"."thread_item_kind";--> statement-breakpoint
DROP TYPE "public"."thread_item_source";