ALTER TABLE "ci_runs" DROP CONSTRAINT "uniq_ci_runs_number";--> statement-breakpoint
ALTER TABLE "ci_jobs" ADD COLUMN "provider" varchar(16) DEFAULT 'exponential' NOT NULL;--> statement-breakpoint
ALTER TABLE "ci_jobs" ADD COLUMN "external_id" varchar(64);--> statement-breakpoint
ALTER TABLE "ci_jobs" ADD COLUMN "external_url" text;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD COLUMN "provider" varchar(16) DEFAULT 'exponential' NOT NULL;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD COLUMN "external_id" varchar(64);--> statement-breakpoint
ALTER TABLE "ci_runs" ADD COLUMN "external_url" text;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD COLUMN "external_attempt" integer;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD COLUMN "external_access" varchar(8);--> statement-breakpoint
CREATE INDEX "idx_ci_jobs_external" ON "ci_jobs" USING btree ("run_id","external_id");--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_ci_runs_number" ON "ci_runs" USING btree ("repository_id","number") WHERE provider = 'exponential';--> statement-breakpoint
ALTER TABLE "ci_runs" ADD CONSTRAINT "uniq_ci_runs_external" UNIQUE("repository_id","external_id");