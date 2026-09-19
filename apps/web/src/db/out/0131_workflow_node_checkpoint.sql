ALTER TABLE "workflow_nodes" ADD COLUMN "checkpoint_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD COLUMN "after_node_ids" jsonb DEFAULT '[]'::jsonb NOT NULL;