DROP TRIGGER IF EXISTS mirror_device_shared_team_id ON devices;--> statement-breakpoint
DROP FUNCTION IF EXISTS mirror_device_shared_team_id();--> statement-breakpoint
ALTER TABLE "devices" DROP COLUMN "shared_team_id";