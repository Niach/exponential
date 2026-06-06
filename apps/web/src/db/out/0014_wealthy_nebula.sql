CREATE TABLE "agent_runs" (
	"issue_id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"plan_text" jsonb,
	"question" jsonb,
	"question_asked_at" timestamp with time zone,
	"plan_revision" integer DEFAULT 0 NOT NULL,
	"approved_at" timestamp with time zone,
	"approved_by" text,
	"last_comment_seen_at" timestamp with time zone,
	"session_id" text,
	"run_mode" varchar(16),
	"interactive_claimed_at" timestamp with time zone,
	"interactive_claimed_expires_at" timestamp with time zone,
	"last_error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "issue_agent_state" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
DROP TABLE "issue_agent_state" CASCADE;--> statement-breakpoint
ALTER TABLE "issues" DROP CONSTRAINT "issues_agent_plan_approved_by_users_id_fk";
--> statement-breakpoint
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "agent_runs" ADD CONSTRAINT "agent_runs_approved_by_users_id_fk" FOREIGN KEY ("approved_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_agent_runs_workspace" ON "agent_runs" USING btree ("workspace_id");--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "agent_plan_revision";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "agent_plan_approved_at";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "agent_plan_approved_by";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "agent_last_comment_seen_at";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "agent_session_id";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "agent_run_mode";--> statement-breakpoint
ALTER TABLE "issues" DROP COLUMN "agent_interactive_claimed_at";