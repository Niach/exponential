CREATE TABLE "agent_registrations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workspace_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"owner_user_id" text NOT NULL,
	"name" varchar(255) NOT NULL,
	"api_key_id" text,
	"oauth_client_id" text,
	"last_seen_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "agent_registrations_workspace_id_user_id_unique" UNIQUE("workspace_id","user_id")
);
--> statement-breakpoint
DROP TABLE "workspace_agents" CASCADE;--> statement-breakpoint
ALTER TABLE "agent_registrations" ADD CONSTRAINT "agent_registrations_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "agent_registrations" ADD CONSTRAINT "agent_registrations_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "agent_registrations" ADD CONSTRAINT "agent_registrations_owner_user_id_users_id_fk" FOREIGN KEY ("owner_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "agent_registrations" ADD CONSTRAINT "agent_registrations_api_key_id_apikeys_id_fk" FOREIGN KEY ("api_key_id") REFERENCES "public"."apikeys"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_agent_registrations_workspace" ON "agent_registrations" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "idx_agent_registrations_user" ON "agent_registrations" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_agent_registrations_owner" ON "agent_registrations" USING btree ("owner_user_id");--> statement-breakpoint
CREATE INDEX "idx_agent_registrations_api_key" ON "agent_registrations" USING btree ("api_key_id");