CREATE TABLE "mcp_server_skill_approvals" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"server_id" uuid NOT NULL,
	"uri" text NOT NULL,
	"digest_set" text NOT NULL,
	"approved_by_user_id" text,
	"approved_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "mcp_server_skill_approvals_server_id_uri_unique" UNIQUE("server_id","uri")
);
--> statement-breakpoint
ALTER TABLE "mcp_server_skill_approvals" ADD CONSTRAINT "mcp_server_skill_approvals_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_server_skill_approvals" ADD CONSTRAINT "mcp_server_skill_approvals_server_id_mcp_servers_id_fk" FOREIGN KEY ("server_id") REFERENCES "public"."mcp_servers"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_server_skill_approvals" ADD CONSTRAINT "mcp_server_skill_approvals_approved_by_user_id_users_id_fk" FOREIGN KEY ("approved_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;