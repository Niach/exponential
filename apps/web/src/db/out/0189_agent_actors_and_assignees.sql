ALTER TABLE "agents" ADD COLUMN IF NOT EXISTS "instructions_updated_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN IF NOT EXISTS "author_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD COLUMN IF NOT EXISTS "assignee_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_events" ADD COLUMN IF NOT EXISTS "actor_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN IF NOT EXISTS "assignee_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN IF NOT EXISTS "creator_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN IF NOT EXISTS "agent_instructions_at" timestamp with time zone;--> statement-breakpoint
DO $$
BEGIN
  -- #EXP-1383: idempotent on purpose. This migration lands on the thread
  -- branch first and is renumbered when the thread goes to master, so a
  -- database that already ran it under its old number must take the
  -- renumbered copy as a no-op.
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'comments_author_agent_id_agents_id_fk') THEN
    ALTER TABLE "comments" ADD CONSTRAINT "comments_author_agent_id_agents_id_fk" FOREIGN KEY ("author_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'issue_drafts_assignee_agent_id_agents_id_fk') THEN
    ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_assignee_agent_id_agents_id_fk" FOREIGN KEY ("assignee_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'issue_events_actor_agent_id_agents_id_fk') THEN
    ALTER TABLE "issue_events" ADD CONSTRAINT "issue_events_actor_agent_id_agents_id_fk" FOREIGN KEY ("actor_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'issues_assignee_agent_id_agents_id_fk') THEN
    ALTER TABLE "issues" ADD CONSTRAINT "issues_assignee_agent_id_agents_id_fk" FOREIGN KEY ("assignee_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'issues_creator_agent_id_agents_id_fk') THEN
    ALTER TABLE "issues" ADD CONSTRAINT "issues_creator_agent_id_agents_id_fk" FOREIGN KEY ("creator_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'issue_drafts_single_assignee_check') THEN
    ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_single_assignee_check" CHECK (assignee_id IS NULL OR assignee_agent_id IS NULL);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'issues_single_assignee_check') THEN
    ALTER TABLE "issues" ADD CONSTRAINT "issues_single_assignee_check" CHECK (assignee_id IS NULL OR assignee_agent_id IS NULL);
  END IF;
END $$;--> statement-breakpoint
CREATE INDEX IF NOT EXISTS "idx_issues_assignee_agent" ON "issues" USING btree ("assignee_agent_id") WHERE assignee_agent_id IS NOT NULL;
