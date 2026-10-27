ALTER TABLE "sessions" ADD COLUMN "background" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "last_activity_at" timestamp with time zone;