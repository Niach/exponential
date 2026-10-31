CREATE TABLE "mcp_calls" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid,
	"user_id" text NOT NULL,
	"client_id" text,
	"app_id" uuid,
	"tool" varchar(128) NOT NULL,
	"status" varchar(16) NOT NULL,
	"error" varchar(64),
	"latency_ms" integer NOT NULL,
	"args_hash" varchar(80) NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE INDEX "idx_mcp_calls_team_created" ON "mcp_calls" USING btree ("team_id","created_at");--> statement-breakpoint
CREATE INDEX "idx_mcp_calls_user_created" ON "mcp_calls" USING btree ("user_id","created_at");--> statement-breakpoint
CREATE INDEX "idx_mcp_calls_created" ON "mcp_calls" USING btree ("created_at");