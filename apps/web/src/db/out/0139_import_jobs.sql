CREATE TABLE "import_entity_map" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"job_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"source" varchar(32) NOT NULL,
	"external_kind" varchar(16) NOT NULL,
	"external_id" text NOT NULL,
	"external_ref" text,
	"local_id" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "import_jobs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"created_by_user_id" text,
	"source" varchar(32) NOT NULL,
	"status" varchar(16) DEFAULT 'draft' NOT NULL,
	"credential" text,
	"payload" jsonb,
	"preview" jsonb,
	"plan" jsonb,
	"progress" jsonb,
	"counts" jsonb,
	"error" text,
	"claim_token" uuid,
	"claimed_at" timestamp with time zone,
	"started_at" timestamp with time zone,
	"finished_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "import_entity_map" ADD CONSTRAINT "import_entity_map_job_id_import_jobs_id_fk" FOREIGN KEY ("job_id") REFERENCES "public"."import_jobs"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "import_entity_map" ADD CONSTRAINT "import_entity_map_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "import_jobs" ADD CONSTRAINT "import_jobs_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "import_jobs" ADD CONSTRAINT "import_jobs_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_import_entity_map_external" ON "import_entity_map" USING btree ("team_id","source","external_kind","external_id");--> statement-breakpoint
CREATE INDEX "idx_import_entity_map_job" ON "import_entity_map" USING btree ("job_id");--> statement-breakpoint
CREATE INDEX "idx_import_jobs_team" ON "import_jobs" USING btree ("team_id","created_at");--> statement-breakpoint
CREATE INDEX "idx_import_jobs_live" ON "import_jobs" USING btree ("status") WHERE status in ('previewing', 'ready', 'running');