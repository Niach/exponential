ALTER TABLE "mcp_calls" ADD COLUMN "session_id" uuid;--> statement-breakpoint
ALTER TABLE "mcp_calls" ADD COLUMN "target" varchar(256);