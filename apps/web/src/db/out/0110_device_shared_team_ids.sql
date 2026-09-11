-- FEED-33: a device can be shared with SEVERAL teams. `shared_team_id`
-- (one uuid, FK set-null) becomes `shared_team_ids` (a sorted uuid[], `{}` =
-- private) on devices and on its device_worktrees scoping mirror; the old
-- column is dropped in 0111 once the data has moved. The GIN indexes serve
-- the shapes' `&&` overlap arms and the router's `@>` probes. Trigger #16
-- (custom/0001_triggers.sql) replaces the FK's set-null on team delete.
-- The two triggers whose WHEN clauses name the old column go first: they
-- would block 0111's DROP COLUMN, and the trigger file re-creates both on
-- the new column right after the migrations run (bootstrap applyCustomSql).
DROP TRIGGER IF EXISTS update_updated_at ON "device_worktrees";--> statement-breakpoint
DROP TRIGGER IF EXISTS propagate_device_shared_team ON "devices";--> statement-breakpoint
DROP INDEX "idx_device_worktrees_shared_team";--> statement-breakpoint
DROP INDEX "idx_devices_shared_team";--> statement-breakpoint
ALTER TABLE "device_worktrees" ADD COLUMN "shared_team_ids" uuid[] DEFAULT '{}'::uuid[] NOT NULL;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "shared_team_ids" uuid[] DEFAULT '{}'::uuid[] NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_device_worktrees_shared_teams" ON "device_worktrees" USING gin ("shared_team_ids");--> statement-breakpoint
CREATE INDEX "idx_devices_shared_teams" ON "devices" USING gin ("shared_team_ids");--> statement-breakpoint
UPDATE "devices" SET "shared_team_ids" = ARRAY["shared_team_id"] WHERE "shared_team_id" IS NOT NULL;--> statement-breakpoint
UPDATE "device_worktrees" SET "shared_team_ids" = ARRAY["shared_team_id"] WHERE "shared_team_id" IS NOT NULL;
