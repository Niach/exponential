CREATE TYPE "public"."pr_merge_state" AS ENUM('draft', 'checks', 'queued', 'testing', 'merged', 'conflict', 'failing', 'held');--> statement-breakpoint
CREATE TABLE "commit_checks" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"repository_id" uuid NOT NULL,
	"sha" varchar(64) NOT NULL,
	"name" varchar(255) NOT NULL,
	"status" "pr_check_status" NOT NULL,
	"conclusion" "pr_check_conclusion",
	"details_url" text,
	"started_at" timestamp with time zone,
	"completed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "uniq_commit_checks_name" UNIQUE("repository_id","sha","name")
);
--> statement-breakpoint
CREATE TABLE "merge_trains" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"repository_id" uuid NOT NULL,
	"base" varchar(255) NOT NULL,
	"base_sha" varchar(64),
	"units" jsonb NOT NULL,
	"state" varchar(16) DEFAULT 'dispatching' NOT NULL,
	"attempt" integer DEFAULT 0 NOT NULL,
	"bisect_of" uuid,
	"untested" boolean DEFAULT false NOT NULL,
	"tested_sha" varchar(64),
	"run_ref" text,
	"conflicts" jsonb,
	"error" text,
	"finished_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_state" "pr_merge_state" DEFAULT 'checks' NOT NULL;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_wait" varchar(16);--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_attempts" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_hold" varchar(16);--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_queued_since" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_ready_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_now" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_position" integer;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_train_id" uuid;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merge_approved_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "teams" ADD COLUMN "merge_mode" varchar(16) DEFAULT 'auto' NOT NULL;--> statement-breakpoint
ALTER TABLE "commit_checks" ADD CONSTRAINT "commit_checks_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_trains" ADD CONSTRAINT "merge_trains_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_trains" ADD CONSTRAINT "merge_trains_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_trains" ADD CONSTRAINT "merge_trains_bisect_of_merge_trains_id_fk" FOREIGN KEY ("bisect_of") REFERENCES "public"."merge_trains"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_merge_trains_repo_state" ON "merge_trains" USING btree ("repository_id","state");--> statement-breakpoint
CREATE INDEX "idx_merge_trains_tested_sha" ON "merge_trains" USING btree ("repository_id","tested_sha");--> statement-breakpoint
ALTER TABLE "pull_requests" ADD CONSTRAINT "pull_requests_merge_train_id_merge_trains_id_fk" FOREIGN KEY ("merge_train_id") REFERENCES "public"."merge_trains"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_pull_requests_merge_state" ON "pull_requests" USING btree ("repository_id","merge_state");--> statement-breakpoint
-- EXP-1709: existing rows take their merge state from the PR state (every team is `auto` by the column default; inert until MERGE_TRAIN is on).
UPDATE "pull_requests" SET "merge_state" = 'merged' WHERE "state" = 'merged';--> statement-breakpoint
UPDATE "pull_requests" SET "merge_state" = 'draft', "merge_wait" = 'draft' WHERE "state" = 'draft';
