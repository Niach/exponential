ALTER TABLE "actions" ADD COLUMN "mcp_server_ids" jsonb DEFAULT '[]'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "mcp_credentials" ADD COLUMN "shared" boolean DEFAULT false NOT NULL;