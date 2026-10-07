ALTER TABLE "issues" ADD COLUMN "draft_id" uuid;--> statement-breakpoint
CREATE INDEX "idx_issues_draft_id" ON "issues" USING btree ("draft_id") WHERE draft_id IS NOT NULL;