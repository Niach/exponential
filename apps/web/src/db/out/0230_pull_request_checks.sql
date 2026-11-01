CREATE TYPE "public"."pr_check_conclusion" AS ENUM('success', 'failure', 'neutral', 'cancelled', 'skipped', 'timed_out', 'action_required', 'startup_failure', 'stale');--> statement-breakpoint
CREATE TYPE "public"."pr_check_status" AS ENUM('queued', 'in_progress', 'completed');--> statement-breakpoint
CREATE TYPE "public"."pr_checks_state" AS ENUM('none', 'pending', 'success', 'failure');--> statement-breakpoint
CREATE TABLE "pull_request_checks" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"pull_request_id" uuid NOT NULL,
	"head_sha" varchar(64) NOT NULL,
	"name" varchar(255) NOT NULL,
	"status" "pr_check_status" NOT NULL,
	"conclusion" "pr_check_conclusion",
	"required" boolean DEFAULT false NOT NULL,
	"started_at" timestamp with time zone,
	"completed_at" timestamp with time zone,
	"details_url" text,
	"queued_flagged_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "uniq_pull_request_checks_name" UNIQUE("pull_request_id","name")
);
--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "checks_state" "pr_checks_state" DEFAULT 'none' NOT NULL;--> statement-breakpoint
ALTER TABLE "pull_request_checks" ADD CONSTRAINT "pull_request_checks_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "pull_request_checks" ADD CONSTRAINT "pull_request_checks_pull_request_id_pull_requests_id_fk" FOREIGN KEY ("pull_request_id") REFERENCES "public"."pull_requests"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_pull_request_checks_team" ON "pull_request_checks" USING btree ("team_id");