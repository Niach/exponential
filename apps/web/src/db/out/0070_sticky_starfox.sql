ALTER TABLE "users" ADD COLUMN "team_ids" uuid[] DEFAULT '{}'::uuid[] NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_comments_body_fts" ON "comments" USING gin (to_tsvector('english', "body"));--> statement-breakpoint
CREATE INDEX "idx_issues_fts" ON "issues" USING gin (to_tsvector('english', coalesce("title", '') || ' ' || coalesce("description", '')));