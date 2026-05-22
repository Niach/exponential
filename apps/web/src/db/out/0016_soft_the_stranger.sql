ALTER TABLE "projects" ADD COLUMN "github_repo" text;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD COLUMN "github_user_login" text;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD COLUMN "github_repos" jsonb;