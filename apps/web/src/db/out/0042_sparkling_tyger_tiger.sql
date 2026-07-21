CREATE INDEX "idx_attachments_comment" ON "attachments" USING btree ("comment_id") WHERE comment_id IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_fcm_tokens_user" ON "fcm_tokens" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_issue_subscribers_issue" ON "issue_subscribers" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_issues_creator" ON "issues" USING btree ("creator_id");--> statement-breakpoint
CREATE INDEX "idx_issues_pr_url" ON "issues" USING btree ("pr_url") WHERE pr_url IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_issues_duplicate_of" ON "issues" USING btree ("duplicate_of_id") WHERE duplicate_of_id IS NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_notifications_issue" ON "notifications" USING btree ("issue_id");