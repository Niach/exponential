-- SLOP-3: one run. Workflows (EXP-978) and the stack system (EXP-897) are
-- gone; a run owns its PR on its own row. Data first, then the drops.
--
-- 1. A finished workflow node run reads as an agent-started run.
UPDATE "coding_sessions" SET "started_reason" = 'agent' WHERE "started_reason" = 'workflow';--> statement-breakpoint
-- 2. A workflow node's open question has nobody left to answer it.
UPDATE "coding_sessions" SET "pending_question" = NULL
WHERE "workflow_node_id" IS NOT NULL AND "pending_question" IS NOT NULL;--> statement-breakpoint
-- 3. Every `pr_open` form stamps the caller's row now. Heal the multi-issue
-- runs opened before that (modelled on 0133): an issue-less, action-less row
-- takes the open PR of the issues sharing its branch. Idempotent: only rows
-- without a PR are touched.
UPDATE "coding_sessions" cs
SET "pr_url" = pr."pr_url", "pr_number" = pr."pr_number", "pr_state" = pr."pr_state"
FROM (
  SELECT DISTINCT ON (cs2."id") cs2."id", i."pr_url", i."pr_number", i."pr_state"
  FROM "coding_sessions" cs2
  JOIN "issues" i ON i."branch" = cs2."branch" AND i."team_id" = cs2."team_id"
  WHERE cs2."issue_id" IS NULL AND cs2."action_name" IS NULL
    AND cs2."pr_url" IS NULL AND cs2."branch" IS NOT NULL
    AND i."pr_url" IS NOT NULL AND i."pr_state" = 'open'
  ORDER BY cs2."id", i."created_at", i."identifier"
) pr
WHERE cs."id" = pr."id";--> statement-breakpoint
ALTER TABLE "coding_sessions" DROP CONSTRAINT IF EXISTS "coding_sessions_workflow_id_workflows_id_fk";--> statement-breakpoint
ALTER TABLE "coding_sessions" DROP CONSTRAINT IF EXISTS "coding_sessions_workflow_node_id_workflow_nodes_id_fk";--> statement-breakpoint
DROP INDEX IF EXISTS "idx_coding_sessions_workflow";--> statement-breakpoint
ALTER TABLE "coding_sessions" DROP COLUMN "workflow_id";--> statement-breakpoint
ALTER TABLE "coding_sessions" DROP COLUMN "workflow_node_id";--> statement-breakpoint
ALTER TABLE "coding_sessions" DROP COLUMN "workflow_role";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "pr_stack_number";--> statement-breakpoint
DROP TABLE "workflow_events" CASCADE;--> statement-breakpoint
DROP TABLE "workflow_nodes" CASCADE;--> statement-breakpoint
DROP TABLE "workflows" CASCADE;
