-- Purge grant rows already orphaned by pre-cascade user deletions: the old
-- `set null` FK left ownerless rows that still entitle their workspace to the
-- departed user's private repos (the entitlement check matches on
-- workspace+installation+repo alone) and that no re-auth REPLACE or
-- account-deletion cleanup can reach.
DELETE FROM "github_installation_repo_grants" WHERE "granted_by_user_id" IS NULL;--> statement-breakpoint
ALTER TABLE "github_installation_repo_grants" DROP CONSTRAINT "github_installation_repo_grants_granted_by_user_id_users_id_fk";
--> statement-breakpoint
ALTER TABLE "github_installation_repo_grants" ADD CONSTRAINT "github_installation_repo_grants_granted_by_user_id_users_id_fk" FOREIGN KEY ("granted_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;
