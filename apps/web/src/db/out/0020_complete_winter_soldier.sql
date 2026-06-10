CREATE TABLE "widget_configs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workspace_id" uuid NOT NULL,
	"project_id" uuid NOT NULL,
	"name" varchar(255) NOT NULL,
	"public_key" varchar(64) NOT NULL,
	"allowed_domains" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"enabled" boolean DEFAULT true NOT NULL,
	"form_config" jsonb,
	"widget_user_id" text NOT NULL,
	"created_by_user_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "widget_configs_public_key_unique" UNIQUE("public_key")
);
--> statement-breakpoint
CREATE TABLE "widget_submissions" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"widget_config_id" uuid,
	"issue_id" uuid NOT NULL,
	"reporter_email" varchar(320),
	"reporter_name" varchar(255),
	"reporter_external_id" varchar(255),
	"page_url" text,
	"user_agent" text,
	"viewport_width" integer,
	"viewport_height" integer,
	"screen_width" integer,
	"screen_height" integer,
	"device_pixel_ratio" double precision,
	"custom_data" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "widget_submissions_issue_id_unique" UNIQUE("issue_id")
);
--> statement-breakpoint
ALTER TABLE "widget_configs" ADD CONSTRAINT "widget_configs_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "widget_configs" ADD CONSTRAINT "widget_configs_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "widget_configs" ADD CONSTRAINT "widget_configs_widget_user_id_users_id_fk" FOREIGN KEY ("widget_user_id") REFERENCES "public"."users"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "widget_configs" ADD CONSTRAINT "widget_configs_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "widget_submissions" ADD CONSTRAINT "widget_submissions_widget_config_id_widget_configs_id_fk" FOREIGN KEY ("widget_config_id") REFERENCES "public"."widget_configs"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "widget_submissions" ADD CONSTRAINT "widget_submissions_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_widget_configs_workspace" ON "widget_configs" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "idx_widget_submissions_config" ON "widget_submissions" USING btree ("widget_config_id");