ALTER TABLE "mcp_grants" DROP CONSTRAINT "mcp_grants_user_id_client_id_unique";--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD COLUMN "kind" varchar(16) DEFAULT 'builtin' NOT NULL;--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD COLUMN "all_apps" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD COLUMN "app_ids" jsonb DEFAULT '[]'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD COLUMN "access" varchar(8) DEFAULT 'full' NOT NULL;--> statement-breakpoint
ALTER TABLE "mcp_grants" ADD CONSTRAINT "mcp_grants_user_client_kind_unique" UNIQUE("user_id","client_id","kind");