CREATE TABLE "merge_train_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"repository_id" uuid NOT NULL,
	"pull_request_id" uuid NOT NULL,
	"merge_train_id" uuid,
	"kind" varchar(16) NOT NULL,
	"base" varchar(255) NOT NULL,
	"attempt" integer DEFAULT 0 NOT NULL,
	"reason" varchar(16),
	"checks" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"ci_job_id" uuid,
	"job_url" text,
	"files" jsonb DEFAULT '[]'::jsonb NOT NULL,
	"sha" varchar(64),
	"by_user_id" text,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "ci_runs" ADD COLUMN "merge_train_id" uuid;--> statement-breakpoint
ALTER TABLE "merge_train_events" ADD CONSTRAINT "merge_train_events_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_train_events" ADD CONSTRAINT "merge_train_events_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_train_events" ADD CONSTRAINT "merge_train_events_pull_request_id_pull_requests_id_fk" FOREIGN KEY ("pull_request_id") REFERENCES "public"."pull_requests"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_train_events" ADD CONSTRAINT "merge_train_events_merge_train_id_merge_trains_id_fk" FOREIGN KEY ("merge_train_id") REFERENCES "public"."merge_trains"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_train_events" ADD CONSTRAINT "merge_train_events_ci_job_id_ci_jobs_id_fk" FOREIGN KEY ("ci_job_id") REFERENCES "public"."ci_jobs"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "merge_train_events" ADD CONSTRAINT "merge_train_events_by_user_id_users_id_fk" FOREIGN KEY ("by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_merge_train_events_team" ON "merge_train_events" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_merge_train_events_repo" ON "merge_train_events" USING btree ("repository_id","created_at");--> statement-breakpoint
CREATE INDEX "idx_merge_train_events_pr" ON "merge_train_events" USING btree ("pull_request_id");--> statement-breakpoint
ALTER TABLE "ci_runs" ADD CONSTRAINT "ci_runs_merge_train_id_merge_trains_id_fk" FOREIGN KEY ("merge_train_id") REFERENCES "public"."merge_trains"("id") ON DELETE set null ON UPDATE no action;