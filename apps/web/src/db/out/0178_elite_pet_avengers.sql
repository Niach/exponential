ALTER TABLE "sessions" ADD COLUMN "move_to_device_id" varchar(128);--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "handoff" jsonb;