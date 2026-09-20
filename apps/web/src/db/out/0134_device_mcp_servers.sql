CREATE TABLE "device_mcp_servers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"device_row_id" uuid NOT NULL,
	"device_id" text NOT NULL,
	"user_id" text NOT NULL,
	"name" varchar(64) NOT NULL,
	"transport" varchar(16) DEFAULT 'http' NOT NULL,
	"url" text,
	"command" text,
	"args" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"source" varchar(16) DEFAULT 'manual' NOT NULL,
	"agent" varchar(16),
	"enabled" boolean DEFAULT true NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "device_mcp_servers_device_row_id_name_unique" UNIQUE("device_row_id","name")
);
--> statement-breakpoint
ALTER TABLE "device_mcp_servers" ADD CONSTRAINT "device_mcp_servers_device_row_id_devices_id_fk" FOREIGN KEY ("device_row_id") REFERENCES "public"."devices"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "device_mcp_servers" ADD CONSTRAINT "device_mcp_servers_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_device_mcp_servers_device" ON "device_mcp_servers" USING btree ("device_row_id");