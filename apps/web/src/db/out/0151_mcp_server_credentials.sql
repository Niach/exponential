DROP TABLE "device_mcp_servers" CASCADE;--> statement-breakpoint
DROP TABLE "mcp_oauth_flows" CASCADE;--> statement-breakpoint
DROP TABLE "mcp_server_readiness" CASCADE;--> statement-breakpoint
CREATE TABLE "mcp_credentials" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"server_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"team_id" uuid NOT NULL,
	"ciphertext" text NOT NULL,
	"expires_at" timestamp with time zone,
	"issuer" text,
	"client_id" text,
	"error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "mcp_credentials_server_id_user_id_unique" UNIQUE("server_id","user_id")
);
--> statement-breakpoint
CREATE TABLE "mcp_oauth_clients" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"issuer" text NOT NULL,
	"redirect_uri" text NOT NULL,
	"registration_endpoint" text NOT NULL,
	"client_id" text NOT NULL,
	"client_secret_ciphertext" text,
	"token_endpoint_auth_method" varchar(32) DEFAULT 'none' NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "mcp_oauth_clients_cache_key" UNIQUE("issuer","redirect_uri","registration_endpoint")
);
--> statement-breakpoint
CREATE TABLE "mcp_oauth_flows" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"state" varchar(64) NOT NULL,
	"user_id" text NOT NULL,
	"team_id" uuid NOT NULL,
	"server_id" uuid NOT NULL,
	"code_verifier_ciphertext" text NOT NULL,
	"client_id" text NOT NULL,
	"issuer" text NOT NULL,
	"token_endpoint" text NOT NULL,
	"resource" text NOT NULL,
	"redirect_uri" text NOT NULL,
	"return_to" text,
	"status" varchar(16) DEFAULT 'pending' NOT NULL,
	"error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"completed_at" timestamp with time zone,
	CONSTRAINT "mcp_oauth_flows_state_unique" UNIQUE("state")
);
--> statement-breakpoint
ALTER TABLE "mcp_credentials" ADD CONSTRAINT "mcp_credentials_server_id_mcp_servers_id_fk" FOREIGN KEY ("server_id") REFERENCES "public"."mcp_servers"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_credentials" ADD CONSTRAINT "mcp_credentials_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_credentials" ADD CONSTRAINT "mcp_credentials_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_oauth_flows" ADD CONSTRAINT "mcp_oauth_flows_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_oauth_flows" ADD CONSTRAINT "mcp_oauth_flows_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_oauth_flows" ADD CONSTRAINT "mcp_oauth_flows_server_id_mcp_servers_id_fk" FOREIGN KEY ("server_id") REFERENCES "public"."mcp_servers"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_mcp_credentials_user" ON "mcp_credentials" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_mcp_oauth_flows_user" ON "mcp_oauth_flows" USING btree ("user_id");