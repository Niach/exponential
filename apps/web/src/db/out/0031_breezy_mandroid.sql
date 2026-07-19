-- EXP-180: standalone helpdesk tickets, workspace-level helpdesk flag, public
-- boards removed, widget support-project target removed.
-- Hand-ordered: additive DDL → data backfills (which need the OLD columns) →
-- NOT NULL/FK constraints → destructive drops.

-- 1. Additive columns (nullable first where a backfill must fill them)
ALTER TABLE "workspaces" ADD COLUMN "helpdesk_enabled" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "support_threads" ADD COLUMN "workspace_id" uuid;--> statement-breakpoint
ALTER TABLE "support_threads" ADD COLUMN "title" varchar(500);--> statement-breakpoint
ALTER TABLE "support_threads" ADD COLUMN "status" varchar(16) DEFAULT 'open' NOT NULL;--> statement-breakpoint
ALTER TABLE "support_threads" ADD COLUMN "linked_issue_id" uuid;--> statement-breakpoint
ALTER TABLE "widget_submissions" ADD COLUMN "support_thread_id" uuid;--> statement-breakpoint
ALTER TABLE "widget_configs" ALTER COLUMN "project_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "widget_submissions" ALTER COLUMN "issue_id" DROP NOT NULL;--> statement-breakpoint

-- 2. Backfill standalone threads from their old issue/project anchors. The
-- old backing issue becomes the linked issue (it stays a normal issue on its
-- board); thread status derives from the issue's resolution state.
UPDATE "support_threads" st
SET "workspace_id" = p."workspace_id",
    "title" = i."title",
    "status" = CASE WHEN i."status" IN ('done', 'cancelled', 'duplicate')
                    THEN 'resolved' ELSE 'open' END,
    "linked_issue_id" = st."issue_id"
FROM "issues" i, "projects" p
WHERE i."id" = st."issue_id" AND p."id" = st."project_id";--> statement-breakpoint

-- Orphan safety net (issue or project row missing): anchor on any workspace
-- via the projects table is impossible, so drop such threads outright rather
-- than leave unscopable rows (FK cascade made this unreachable in practice).
DELETE FROM "support_threads" WHERE "workspace_id" IS NULL;--> statement-breakpoint

-- 3. Old ticket submissions become thread-anchored as well (they keep their
-- issue_id too — the escalated/legacy issue remains valid history).
UPDATE "widget_submissions" ws
SET "support_thread_id" = st."id"
FROM "support_threads" st
WHERE st."linked_issue_id" = ws."issue_id" AND ws."issue_id" IS NOT NULL;--> statement-breakpoint

-- 4. Workspace helpdesk flag: on wherever any project had it on (reads the
-- per-project column that is dropped below).
UPDATE "workspaces" w
SET "helpdesk_enabled" = true
WHERE EXISTS (
  SELECT 1 FROM "projects" p
  WHERE p."workspace_id" = w."id" AND p."helpdesk_enabled"
);--> statement-breakpoint

-- 5. Constraints on the backfilled columns
ALTER TABLE "support_threads" ALTER COLUMN "workspace_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "support_threads" ALTER COLUMN "title" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "support_threads" ADD CONSTRAINT "support_threads_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "support_threads" ADD CONSTRAINT "support_threads_linked_issue_id_issues_id_fk" FOREIGN KEY ("linked_issue_id") REFERENCES "public"."issues"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "widget_submissions" ADD CONSTRAINT "widget_submissions_support_thread_id_support_threads_id_fk" FOREIGN KEY ("support_thread_id") REFERENCES "public"."support_threads"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_support_threads_workspace" ON "support_threads" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "idx_widget_submissions_thread" ON "widget_submissions" USING btree ("support_thread_id");--> statement-breakpoint

-- 6. Rewire widget_configs.project_id from cascade to set null
ALTER TABLE "widget_configs" DROP CONSTRAINT "widget_configs_project_id_projects_id_fk";--> statement-breakpoint
ALTER TABLE "widget_configs" ADD CONSTRAINT "widget_configs_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint

-- 7. Destructive drops (backfills above already consumed the old columns)
ALTER TABLE "support_threads" DROP CONSTRAINT "support_threads_issue_id_unique";--> statement-breakpoint
ALTER TABLE "support_messages" DROP CONSTRAINT "support_messages_issue_id_issues_id_fk";--> statement-breakpoint
ALTER TABLE "support_threads" DROP CONSTRAINT "support_threads_issue_id_issues_id_fk";--> statement-breakpoint
ALTER TABLE "support_threads" DROP CONSTRAINT "support_threads_project_id_projects_id_fk";--> statement-breakpoint
ALTER TABLE "widget_configs" DROP CONSTRAINT "widget_configs_support_project_id_projects_id_fk";--> statement-breakpoint
DROP INDEX "idx_projects_public";--> statement-breakpoint
DROP INDEX "idx_support_messages_issue";--> statement-breakpoint
DROP INDEX "idx_support_threads_project";--> statement-breakpoint
ALTER TABLE "projects" DROP COLUMN "is_public";--> statement-breakpoint
ALTER TABLE "projects" DROP COLUMN "public_show_comments";--> statement-breakpoint
ALTER TABLE "projects" DROP COLUMN "public_show_activity";--> statement-breakpoint
ALTER TABLE "projects" DROP COLUMN "helpdesk_enabled";--> statement-breakpoint
ALTER TABLE "support_messages" DROP COLUMN "issue_id";--> statement-breakpoint
ALTER TABLE "support_threads" DROP COLUMN "issue_id";--> statement-breakpoint
ALTER TABLE "support_threads" DROP COLUMN "project_id";--> statement-breakpoint
ALTER TABLE "widget_configs" DROP COLUMN "support_project_id";
