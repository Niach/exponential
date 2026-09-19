ALTER TABLE "workflow_nodes" ADD COLUMN "review_round" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD COLUMN "review" jsonb;