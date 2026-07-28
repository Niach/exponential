ALTER TABLE "teams" ADD COLUMN "pr_opened_status_id" uuid;--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "pr_opened_automation" boolean DEFAULT true NOT NULL;--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "pr_merged_status_id" uuid;--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "pr_merged_automation" boolean DEFAULT true NOT NULL;--> statement-breakpoint
ALTER TABLE "teams" ADD CONSTRAINT "teams_pr_opened_status_id_issue_statuses_id_fk" FOREIGN KEY ("pr_opened_status_id") REFERENCES "public"."issue_statuses"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "teams" ADD CONSTRAINT "teams_pr_merged_status_id_issue_statuses_id_fk" FOREIGN KEY ("pr_merged_status_id") REFERENCES "public"."issue_statuses"("id") ON DELETE set null ON UPDATE no action;