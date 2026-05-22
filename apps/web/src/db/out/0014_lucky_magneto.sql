CREATE TABLE "workspace_agents" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workspace_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"name" varchar(255) NOT NULL,
	"setup_token_hash" text NOT NULL,
	"setup_token_expires_at" timestamp with time zone NOT NULL,
	"setup_token_consumed_at" timestamp with time zone,
	"api_key_id" text,
	"last_seen_at" timestamp with time zone,
	"whatsapp_status" varchar(32) DEFAULT 'not_configured' NOT NULL,
	"whatsapp_pairing_requested_at" timestamp with time zone,
	"whatsapp_qr" text,
	"whatsapp_qr_updated_at" timestamp with time zone,
	"whatsapp_last_error" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "workspace_agents_workspace_id_user_id_unique" UNIQUE("workspace_id","user_id")
);
--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD CONSTRAINT "workspace_agents_workspace_id_workspaces_id_fk" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspaces"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD CONSTRAINT "workspace_agents_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD CONSTRAINT "workspace_agents_api_key_id_apikeys_id_fk" FOREIGN KEY ("api_key_id") REFERENCES "public"."apikeys"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_workspace_agents_workspace" ON "workspace_agents" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "idx_workspace_agents_user" ON "workspace_agents" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "idx_workspace_agents_setup_token" ON "workspace_agents" USING btree ("setup_token_hash");--> statement-breakpoint
CREATE INDEX "idx_workspace_agents_api_key" ON "workspace_agents" USING btree ("api_key_id");