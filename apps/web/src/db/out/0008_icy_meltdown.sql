CREATE TABLE "mcp_grants" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text NOT NULL,
	"client_id" text NOT NULL,
	"all_workspaces" boolean DEFAULT false NOT NULL,
	"workspace_ids" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"project_ids" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "mcp_grants_user_id_client_id_unique" UNIQUE("user_id","client_id")
);
--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD CONSTRAINT "mcp_grants_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD CONSTRAINT "mcp_grants_client_id_oauth_applications_client_id_fk" FOREIGN KEY ("client_id") REFERENCES "public"."oauth_applications"("client_id") ON DELETE cascade ON UPDATE no action;