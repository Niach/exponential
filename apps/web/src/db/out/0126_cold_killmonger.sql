CREATE INDEX "idx_github_installation_links_created_by" ON "github_installation_links" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE INDEX "idx_github_installation_repo_grants_granted_by" ON "github_installation_repo_grants" USING btree ("granted_by_user_id");--> statement-breakpoint
CREATE INDEX "idx_repositories_shared_by" ON "repositories" USING btree ("shared_by_user_id");