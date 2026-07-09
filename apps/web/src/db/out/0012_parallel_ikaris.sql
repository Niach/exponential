CREATE TABLE "github_installation_repo_grants" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workspace_id" uuid NOT NULL,
	"installation_id" bigint NOT NULL,
	"full_name" text NOT NULL,
	"private" boolean DEFAULT false NOT NULL,
	"default_branch" text,
	"granted_by_user_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "github_installation_repo_grants_scope_unique" UNIQUE("workspace_id","installation_id","full_name","granted_by_user_id")
);
--> statement-breakpoint
ALTER TABLE "github_installation_repo_grants" ADD CONSTRAINT "github_installation_repo_grants_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "github_installation_repo_grants" ADD CONSTRAINT "github_installation_repo_grants_granted_by_user_id_users_id_fk" FOREIGN KEY ("granted_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_github_installation_repo_grants_ws_inst" ON "github_installation_repo_grants" USING btree ("workspace_id","installation_id");