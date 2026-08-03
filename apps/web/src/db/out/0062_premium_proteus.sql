ALTER TABLE "devices" ADD COLUMN "version" varchar(32);--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "update_requested_at" timestamp with time zone;