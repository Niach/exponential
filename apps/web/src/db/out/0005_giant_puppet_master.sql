CREATE TABLE "issue_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"issue_id" uuid NOT NULL,
	"workspace_id" uuid NOT NULL,
	"actor_user_id" text,
	"type" varchar(32) NOT NULL,
	"payload" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "issue_subscribers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"issue_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"workspace_id" uuid NOT NULL,
	"source" varchar(16) NOT NULL,
	"unsubscribed" boolean DEFAULT false NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "issue_subscribers_issue_id_user_id_unique" UNIQUE("issue_id","user_id")
);
--> statement-breakpoint
ALTER TABLE "workspace_agents" ALTER COLUMN "setup_token_hash" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "workspace_agents" ALTER COLUMN "setup_token_expires_at" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "pr_url" text;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "pr_number" integer;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "pr_state" varchar(16);--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "branch" text;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "pr_merged_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_session_id" text;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_run_mode" varchar(16);--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "agent_interactive_claimed_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD COLUMN "owner_user_id" text;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD COLUMN "oauth_client_id" text;--> statement-breakpoint
ALTER TABLE "users" ADD COLUMN "is_agent" boolean DEFAULT false NOT NULL;--> statement-breakpoint
UPDATE "users" SET "is_agent" = true WHERE "id" IN (SELECT "user_id" FROM "workspace_agents");--> statement-breakpoint
ALTER TABLE "issue_events" ADD CONSTRAINT "issue_events_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_events" ADD CONSTRAINT "issue_events_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_events" ADD CONSTRAINT "issue_events_actor_user_id_users_id_fk" FOREIGN KEY ("actor_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ADD CONSTRAINT "issue_subscribers_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ADD CONSTRAINT "issue_subscribers_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_subscribers" ADD CONSTRAINT "issue_subscribers_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issue_events_issue" ON "issue_events" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_issue_events_workspace" ON "issue_events" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "idx_issue_subscribers_user" ON "issue_subscribers" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_issue_subscribers_workspace" ON "issue_subscribers" USING btree ("workspace_id");--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD CONSTRAINT "workspace_agents_owner_user_id_users_id_fk" FOREIGN KEY ("owner_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_workspace_agents_owner" ON "workspace_agents" USING btree ("owner_user_id");