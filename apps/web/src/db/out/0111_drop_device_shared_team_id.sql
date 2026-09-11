ALTER TABLE "device_worktrees" DROP CONSTRAINT "device_worktrees_shared_team_id_teams_id_fk";
--> statement-breakpoint
ALTER TABLE "devices" DROP CONSTRAINT "devices_shared_team_id_teams_id_fk";
--> statement-breakpoint
ALTER TABLE "device_worktrees" DROP COLUMN "shared_team_id";--> statement-breakpoint
ALTER TABLE "devices" DROP COLUMN "shared_team_id";