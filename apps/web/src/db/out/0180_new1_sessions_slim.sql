ALTER TABLE "sessions" DROP CONSTRAINT "sessions_host_user_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "sessions" DROP CONSTRAINT "sessions_spawned_by_session_id_sessions_id_fk";
--> statement-breakpoint
DROP INDEX "idx_session_issues_assigned_session";--> statement-breakpoint
DROP INDEX "idx_sessions_spawned_by";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "host_user_id";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "work_launch";--> statement-breakpoint
ALTER TABLE "sessions" DROP COLUMN "handoff";--> statement-breakpoint
-- NEW-1: issue links move onto the session that holds them. A child's
-- assigned rows leave the root; a row the child already holds (never today,
-- the unique key forbade it) would conflict, so those go first. The column
-- stays as a never-written read shim for the shipped desktop (NEW-8).
DELETE FROM "session_issues" si
USING "session_issues" held
WHERE si."assigned_session_id" IS NOT NULL
  AND held."session_id" = si."assigned_session_id"
  AND held."issue_id" = si."issue_id";--> statement-breakpoint
UPDATE "session_issues" si
SET "session_id" = si."assigned_session_id",
    "assigned_session_id" = NULL,
    "shared" = s."shared",
    "owner_id" = s."user_id"
FROM "sessions" s
WHERE si."assigned_session_id" IS NOT NULL
  AND s."id" = si."assigned_session_id";--> statement-breakpoint
-- The dispatcher flag and the move/aside/caption shims are never written
-- again; the legacy spawned-by pointer too.
UPDATE "sessions" SET "dispatch" = false WHERE "dispatch";--> statement-breakpoint
UPDATE "sessions" SET "spawned_by_session_id" = NULL WHERE "spawned_by_session_id" IS NOT NULL;
