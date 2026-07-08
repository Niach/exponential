-- v7: workspace-level publicness → project-level feedback boards. The data
-- migration below must run while is_public still exists, in the same
-- migration as the column drop.
-- 1) Ex-public workspaces keep their public surface as feedback-board projects.
UPDATE "projects" SET "type" = 'feedback', "public_show_comments" = true
  WHERE "workspace_id" IN (SELECT "id" FROM "workspaces" WHERE "is_public" = true);--> statement-breakpoint
-- 2) PRIVACY-CRITICAL: purge self-joined (non-owner) members of ex-public
-- workspaces. Once is_public is gone, getReadableUserIdsInWorkspaces treats
-- every membership as an explicit invite and syncs co-member emails — strangers
-- who self-joined the public board must not become part of that directory.
DELETE FROM "workspace_members"
  WHERE "workspace_id" IN (SELECT "id" FROM "workspaces" WHERE "is_public" = true)
    AND "role" <> 'owner';--> statement-breakpoint
ALTER TABLE "workspaces" DROP COLUMN "is_public";--> statement-breakpoint
ALTER TABLE "workspaces" DROP COLUMN "public_write_policy";--> statement-breakpoint
DROP TYPE "public"."public_write_policy";
