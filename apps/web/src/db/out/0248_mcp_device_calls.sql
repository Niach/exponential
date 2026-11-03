CREATE TABLE "mcp_device_calls" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"user_id" text NOT NULL,
	"server_id" uuid NOT NULL,
	"device_id" uuid NOT NULL,
	"tool" varchar(128) NOT NULL,
	"args" jsonb NOT NULL,
	"veto" jsonb NOT NULL,
	"reply_public" varchar(43) NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"claimed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "mcp_servers" ADD COLUMN "device_id" uuid;--> statement-breakpoint
ALTER TABLE "mcp_device_calls" ADD CONSTRAINT "mcp_device_calls_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_device_calls" ADD CONSTRAINT "mcp_device_calls_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_device_calls" ADD CONSTRAINT "mcp_device_calls_server_id_mcp_servers_id_fk" FOREIGN KEY ("server_id") REFERENCES "public"."mcp_servers"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "mcp_device_calls" ADD CONSTRAINT "mcp_device_calls_device_id_devices_id_fk" FOREIGN KEY ("device_id") REFERENCES "public"."devices"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_mcp_device_calls_expires" ON "mcp_device_calls" USING btree ("expires_at");--> statement-breakpoint
ALTER TABLE "mcp_servers" ADD CONSTRAINT "mcp_servers_device_id_devices_id_fk" FOREIGN KEY ("device_id") REFERENCES "public"."devices"("id") ON DELETE set null ON UPDATE no action;