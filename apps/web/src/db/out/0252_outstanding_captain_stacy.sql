CREATE TABLE "app_upstream_bindings" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"app_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"upstream" varchar(64) NOT NULL,
	"server_id" uuid NOT NULL,
	"set_by" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "app_upstream_bindings_app_upstream_unique" UNIQUE("app_id","upstream")
);
--> statement-breakpoint
ALTER TABLE "app_upstream_bindings" ADD CONSTRAINT "app_upstream_bindings_app_id_apps_id_fk" FOREIGN KEY ("app_id") REFERENCES "public"."apps"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_upstream_bindings" ADD CONSTRAINT "app_upstream_bindings_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_upstream_bindings" ADD CONSTRAINT "app_upstream_bindings_server_id_mcp_servers_id_fk" FOREIGN KEY ("server_id") REFERENCES "public"."mcp_servers"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "app_upstream_bindings" ADD CONSTRAINT "app_upstream_bindings_set_by_users_id_fk" FOREIGN KEY ("set_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "app_upstream_bindings_server_idx" ON "app_upstream_bindings" USING btree ("server_id");