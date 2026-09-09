CREATE TABLE "mcp_oauth_flows" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"state" varchar(64) NOT NULL,
	"user_id" text NOT NULL,
	"team_id" uuid NOT NULL,
	"server_id" uuid NOT NULL,
	"device_row_id" uuid NOT NULL,
	"redirect" varchar(16) DEFAULT 'hosted' NOT NULL,
	"status" varchar(16) DEFAULT 'pending' NOT NULL,
	"authorize_url" text,
	"error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"completed_at" timestamp with time zone,
	CONSTRAINT "mcp_oauth_flows_state_unique" UNIQUE("state")
);
--> statement-breakpoint
CREATE TABLE "mcp_server_readiness" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"server_id" uuid NOT NULL,
	"device_row_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"ready" boolean DEFAULT false NOT NULL,
	"expires_at" timestamp with time zone,
	"error" text,
	"checked_at" timestamp with time zone DEFAULT now() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "mcp_server_readiness_server_id_device_row_id_unique" UNIQUE("server_id","device_row_id")
);
--> statement-breakpoint
CREATE TABLE "mcp_servers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"name" varchar(64) NOT NULL,
	"transport" varchar(16) DEFAULT 'http' NOT NULL,
	"url" text,
	"header_names" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"command" text,
	"args" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"env_names" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"scopes" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"auth" varchar(16) DEFAULT 'none' NOT NULL,
	"enabled_by_default" boolean DEFAULT false NOT NULL,
	"created_by_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "mcp_servers_team_id_name_unique" UNIQUE("team_id","name")
);
--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "agent_account" varchar(64);--> statement-breakpoint
ALTER TABLE "mcp_oauth_flows" ADD CONSTRAINT "mcp_oauth_flows_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_oauth_flows" ADD CONSTRAINT "mcp_oauth_flows_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_oauth_flows" ADD CONSTRAINT "mcp_oauth_flows_server_id_mcp_servers_id_fk" FOREIGN KEY ("server_id") REFERENCES "public"."mcp_servers"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_oauth_flows" ADD CONSTRAINT "mcp_oauth_flows_device_row_id_devices_id_fk" FOREIGN KEY ("device_row_id") REFERENCES "public"."devices"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_server_readiness" ADD CONSTRAINT "mcp_server_readiness_server_id_mcp_servers_id_fk" FOREIGN KEY ("server_id") REFERENCES "public"."mcp_servers"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_server_readiness" ADD CONSTRAINT "mcp_server_readiness_device_row_id_devices_id_fk" FOREIGN KEY ("device_row_id") REFERENCES "public"."devices"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_server_readiness" ADD CONSTRAINT "mcp_server_readiness_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_servers" ADD CONSTRAINT "mcp_servers_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_servers" ADD CONSTRAINT "mcp_servers_created_by_id_users_id_fk" FOREIGN KEY ("created_by_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_mcp_oauth_flows_device" ON "mcp_oauth_flows" USING btree ("device_row_id");--> statement-breakpoint
CREATE INDEX "idx_mcp_server_readiness_device" ON "mcp_server_readiness" USING btree ("device_row_id");--> statement-breakpoint
CREATE INDEX "idx_mcp_servers_team" ON "mcp_servers" USING btree ("team_id");