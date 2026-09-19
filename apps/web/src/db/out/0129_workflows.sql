CREATE TABLE "workflow_nodes" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workflow_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"issue_id" uuid NOT NULL,
	"member_issue_ids" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"kind" varchar(16) DEFAULT 'leaf' NOT NULL,
	"state" varchar(16) DEFAULT 'blocked' NOT NULL,
	"risk" varchar(8) DEFAULT 'medium' NOT NULL,
	"wave" integer DEFAULT 0 NOT NULL,
	"lane" integer DEFAULT 0 NOT NULL,
	"on_cycle" boolean DEFAULT false NOT NULL,
	"session_id" uuid,
	"attempt" integer DEFAULT 0 NOT NULL,
	"base_branch" varchar(255),
	"budget" jsonb,
	"touches" text[] DEFAULT '{}'::text[] NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "workflows" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"repository_id" uuid,
	"creator_id" text,
	"name" varchar(255) NOT NULL,
	"status" varchar(16) DEFAULT 'draft' NOT NULL,
	"device_id" varchar(128),
	"launch" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"gate" varchar(16) DEFAULT 'human' NOT NULL,
	"start_on" varchar(16) DEFAULT 'contract' NOT NULL,
	"integration_branch" varchar(255) NOT NULL,
	"final_pr_url" text,
	"final_pr_number" integer,
	"final_pr_state" "pr_state",
	"decisions" text DEFAULT '' NOT NULL,
	"metrics" jsonb DEFAULT '{"nodes":0,"edges":0,"depth":0,"width":0,"cycles":[]}'::jsonb NOT NULL,
	"started_at" timestamp with time zone,
	"ended_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD CONSTRAINT "workflow_nodes_workflow_id_workflows_id_fk" FOREIGN KEY ("workflow_id") REFERENCES "public"."workflows"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD CONSTRAINT "workflow_nodes_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD CONSTRAINT "workflow_nodes_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD CONSTRAINT "workflow_nodes_session_id_coding_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."coding_sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflows" ADD CONSTRAINT "workflows_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflows" ADD CONSTRAINT "workflows_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflows" ADD CONSTRAINT "workflows_creator_id_users_id_fk" FOREIGN KEY ("creator_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_workflow_nodes_workflow" ON "workflow_nodes" USING btree ("workflow_id");--> statement-breakpoint
CREATE INDEX "idx_workflow_nodes_team" ON "workflow_nodes" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_workflow_nodes_issue" ON "workflow_nodes" USING btree ("issue_id");--> statement-breakpoint
CREATE INDEX "idx_workflow_nodes_session" ON "workflow_nodes" USING btree ("session_id") WHERE session_id IS NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_workflow_nodes_issue" ON "workflow_nodes" USING btree ("workflow_id","issue_id");--> statement-breakpoint
CREATE INDEX "idx_workflows_team" ON "workflows" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_workflows_repository" ON "workflows" USING btree ("repository_id");