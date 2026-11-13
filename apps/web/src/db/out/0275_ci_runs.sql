CREATE TABLE "ci_jobs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"run_id" uuid NOT NULL,
	"key" varchar(64) NOT NULL,
	"check" varchar(255) NOT NULL,
	"position" integer NOT NULL,
	"runs_on" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"needs" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"matrix" jsonb DEFAULT '{}'::jsonb NOT NULL,
	"timeout_min" integer DEFAULT 60 NOT NULL,
	"status" "pr_check_status" DEFAULT 'queued' NOT NULL,
	"conclusion" "pr_check_conclusion",
	"reason" varchar(32),
	"runner_device_id" uuid,
	"steps" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"log_key" text,
	"log_bytes" bigint DEFAULT 0 NOT NULL,
	"queued_at" timestamp with time zone DEFAULT now() NOT NULL,
	"started_at" timestamp with time zone,
	"completed_at" timestamp with time zone,
	"spec" jsonb NOT NULL,
	"job_token_hash" text,
	"secrets_fetched_at" timestamp with time zone,
	"dispatched_device_id" uuid,
	"dispatched_at" timestamp with time zone,
	"last_beat_at" timestamp with time zone,
	"queued_flagged_at" timestamp with time zone,
	"log_chunks" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "ci_runs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"repository_id" uuid NOT NULL,
	"number" integer NOT NULL,
	"name" varchar(64) NOT NULL,
	"sha" varchar(64) NOT NULL,
	"ref" varchar(255),
	"trigger" varchar(16) NOT NULL,
	"pull_request_id" uuid,
	"state" varchar(16) DEFAULT 'pending' NOT NULL,
	"error" text,
	"rerun_of" uuid,
	"created_by_user_id" text,
	"started_at" timestamp with time zone,
	"completed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "uniq_ci_runs_number" UNIQUE("repository_id","number")
);
--> statement-breakpoint
CREATE TABLE "ci_secrets" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"repository_id" uuid NOT NULL,
	"name" varchar(128) NOT NULL,
	"value_ciphertext" text NOT NULL,
	"updated_by_user_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "uniq_ci_secrets_name" UNIQUE("repository_id","name")
);
--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "runner" jsonb;--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "ci_config" jsonb DEFAULT '{}'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "ci_jobs" ADD CONSTRAINT "ci_jobs_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_jobs" ADD CONSTRAINT "ci_jobs_run_id_ci_runs_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."ci_runs"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_jobs" ADD CONSTRAINT "ci_jobs_runner_device_id_devices_id_fk" FOREIGN KEY ("runner_device_id") REFERENCES "public"."devices"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD CONSTRAINT "ci_runs_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD CONSTRAINT "ci_runs_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD CONSTRAINT "ci_runs_pull_request_id_pull_requests_id_fk" FOREIGN KEY ("pull_request_id") REFERENCES "public"."pull_requests"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD CONSTRAINT "ci_runs_rerun_of_ci_runs_id_fk" FOREIGN KEY ("rerun_of") REFERENCES "public"."ci_runs"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_runs" ADD CONSTRAINT "ci_runs_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_secrets" ADD CONSTRAINT "ci_secrets_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_secrets" ADD CONSTRAINT "ci_secrets_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ci_secrets" ADD CONSTRAINT "ci_secrets_updated_by_user_id_users_id_fk" FOREIGN KEY ("updated_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_ci_jobs_run" ON "ci_jobs" USING btree ("run_id");--> statement-breakpoint
CREATE INDEX "idx_ci_jobs_team" ON "ci_jobs" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_ci_jobs_open" ON "ci_jobs" USING btree ("status") WHERE status <> 'completed';--> statement-breakpoint
CREATE INDEX "idx_ci_jobs_runner" ON "ci_jobs" USING btree ("runner_device_id");--> statement-breakpoint
CREATE INDEX "idx_ci_runs_team" ON "ci_runs" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_ci_runs_repo_sha" ON "ci_runs" USING btree ("repository_id","sha");