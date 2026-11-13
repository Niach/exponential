CREATE TABLE "repo_packs" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"repository_id" uuid NOT NULL,
	"kind" varchar(16) DEFAULT 'pack' NOT NULL,
	"key" text NOT NULL,
	"bytes" bigint NOT NULL,
	"seq" bigserial NOT NULL,
	"refs" jsonb,
	"superseded_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "repo_packs_key_unique" UNIQUE("key")
);
--> statement-breakpoint
CREATE TABLE "repo_refs" (
	"repository_id" uuid NOT NULL,
	"ref" varchar(255) NOT NULL,
	"sha" varchar(64) NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "repo_refs_repository_id_ref_pk" PRIMARY KEY("repository_id","ref")
);
--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "body" text;--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "head_sha" varchar(64);--> statement-breakpoint
ALTER TABLE "pull_requests" ADD COLUMN "merged_sha" varchar(64);--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "pr_sequence" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "repo_packs" ADD CONSTRAINT "repo_packs_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "repo_packs" ADD CONSTRAINT "repo_packs_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "repo_refs" ADD CONSTRAINT "repo_refs_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_repo_packs_repo" ON "repo_packs" USING btree ("repository_id","kind","seq");--> statement-breakpoint
CREATE INDEX "idx_repo_packs_team" ON "repo_packs" USING btree ("team_id");