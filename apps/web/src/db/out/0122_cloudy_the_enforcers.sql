ALTER TABLE "issues" ADD COLUMN "pr_base_branch" text;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "pr_stack_number" integer;--> statement-breakpoint
CREATE INDEX "idx_coding_sessions_parent" ON "coding_sessions" USING btree ("parent_session_id");--> statement-breakpoint
CREATE INDEX "idx_issues_pr_base_branch" ON "issues" USING btree ("pr_base_branch") WHERE pr_base_branch IS NOT NULL;