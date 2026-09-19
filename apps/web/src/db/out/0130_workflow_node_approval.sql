ALTER TABLE "workflow_nodes" ADD COLUMN "approved_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD COLUMN "note" varchar(500);