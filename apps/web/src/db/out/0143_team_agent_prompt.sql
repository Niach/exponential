ALTER TABLE "teams" ADD COLUMN "agent_prompt" text DEFAULT '' NOT NULL;--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "agent_prompt_updated_at" timestamp with time zone;