ALTER TABLE "coding_sessions" ADD COLUMN "agent" varchar(16);--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "agent_accounts" jsonb;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "agent_usage" jsonb;--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "agent_usage_at" timestamp with time zone;