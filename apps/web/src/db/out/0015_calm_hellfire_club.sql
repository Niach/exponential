CREATE TABLE "releases" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workspace_id" uuid NOT NULL,
	"name" varchar(255) NOT NULL,
	"description" text,
	"target_date" date,
	"shipped_at" timestamp with time zone,
	"created_by" text,
	"pr_url" text,
	"pr_number" integer,
	"pr_state" "pr_state",
	"pr_merged_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "coding_sessions" ALTER COLUMN "issue_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "coding_sessions" ALTER COLUMN "project_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "release_id" uuid;--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "release_id" uuid;--> statement-breakpoint
ALTER TABLE "releases" ADD CONSTRAINT "releases_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "releases" ADD CONSTRAINT "releases_created_by_users_id_fk" FOREIGN KEY ("created_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_releases_workspace" ON "releases" USING btree ("workspace_id");--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD CONSTRAINT "coding_sessions_release_id_releases_id_fk" FOREIGN KEY ("release_id") REFERENCES "public"."releases"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_release_id_releases_id_fk" FOREIGN KEY ("release_id") REFERENCES "public"."releases"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_coding_sessions_release" ON "coding_sessions" USING btree ("release_id");--> statement-breakpoint
CREATE INDEX "idx_issues_release" ON "issues" USING btree ("release_id");