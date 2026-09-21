ALTER TABLE "workflows" ALTER COLUMN "gate" SET DEFAULT 'agent';--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD COLUMN "merged_into" varchar(255);--> statement-breakpoint
ALTER TABLE "workflow_nodes" ADD COLUMN "retried_at" timestamp with time zone;--> statement-breakpoint
-- EXP-1010: the review gate setting is gone, every node gets an agent review.
-- The column stays for engines that still read it, pinned to the one mode left.
UPDATE "workflows" SET "gate" = 'agent' WHERE "gate" <> 'agent';
