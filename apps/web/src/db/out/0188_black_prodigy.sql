ALTER TABLE "agents" ADD COLUMN "instructions_updated_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "comments" ADD COLUMN "author_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD COLUMN "assignee_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_events" ADD COLUMN "actor_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "assignee_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "creator_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "agent_instructions_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "comments" ADD CONSTRAINT "comments_author_agent_id_agents_id_fk" FOREIGN KEY ("author_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_assignee_agent_id_agents_id_fk" FOREIGN KEY ("assignee_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_events" ADD CONSTRAINT "issue_events_actor_agent_id_agents_id_fk" FOREIGN KEY ("actor_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_assignee_agent_id_agents_id_fk" FOREIGN KEY ("assignee_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_creator_agent_id_agents_id_fk" FOREIGN KEY ("creator_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issues_assignee_agent" ON "issues" USING btree ("assignee_agent_id") WHERE assignee_agent_id IS NOT NULL;--> statement-breakpoint
ALTER TABLE "issue_drafts" ADD CONSTRAINT "issue_drafts_single_assignee_check" CHECK (assignee_id IS NULL OR assignee_agent_id IS NULL);--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_single_assignee_check" CHECK (assignee_id IS NULL OR assignee_agent_id IS NULL);