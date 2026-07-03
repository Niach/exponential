CREATE TABLE "run_configs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"project_id" uuid NOT NULL,
	"workspace_id" uuid NOT NULL,
	"name" varchar(255) NOT NULL,
	"argv" jsonb NOT NULL,
	"cwd" text,
	"env" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "run_configs_project_id_name_unique" UNIQUE("project_id","name")
);
--> statement-breakpoint
ALTER TABLE "run_configs" ADD CONSTRAINT "run_configs_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "run_configs" ADD CONSTRAINT "run_configs_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_run_configs_workspace" ON "run_configs" USING btree ("workspace_id");