ALTER TABLE "issues" ADD COLUMN "deleted_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "deleted_by" text;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "deleted_by_agent_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_deleted_by_users_id_fk" FOREIGN KEY ("deleted_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_deleted_by_agent_id_agents_id_fk" FOREIGN KEY ("deleted_by_agent_id") REFERENCES "public"."agents"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issues_deleted" ON "issues" USING btree ("team_id","deleted_at") WHERE deleted_at IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_issues_deleted_by" ON "issues" USING btree ("deleted_by") WHERE deleted_by IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_issues_deleted_by_agent" ON "issues" USING btree ("deleted_by_agent_id") WHERE deleted_by_agent_id IS NOT NULL;