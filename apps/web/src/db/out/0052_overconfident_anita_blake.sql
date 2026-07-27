CREATE TYPE "public"."issue_status_category" AS ENUM('backlog', 'unstarted', 'started', 'completed', 'cancelled', 'duplicate');--> statement-breakpoint
CREATE TABLE "issue_statuses" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"category" "issue_status_category" NOT NULL,
	"name" varchar(255) NOT NULL,
	"color" varchar(7) NOT NULL,
	"sort_order" double precision DEFAULT 0 NOT NULL,
	"builtin_key" "issue_status",
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "issues" ADD COLUMN "status_id" uuid;--> statement-breakpoint
ALTER TABLE "issue_statuses" ADD CONSTRAINT "issue_statuses_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issue_statuses_team" ON "issue_statuses" USING btree ("team_id");--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_issue_statuses_team_name_ci" ON "issue_statuses" USING btree ("team_id",lower("name"));--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_issue_statuses_team_builtin" ON "issue_statuses" USING btree ("team_id","builtin_key") WHERE builtin_key IS NOT NULL;--> statement-breakpoint
ALTER TABLE "issues" ADD CONSTRAINT "issues_status_id_issue_statuses_id_fk" FOREIGN KEY ("status_id") REFERENCES "public"."issue_statuses"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_issues_status_id" ON "issues" USING btree ("status_id") WHERE status_id IS NOT NULL;--> statement-breakpoint
-- EXP-314 data backfill: seed the 7 locked builtin statuses for every EXISTING
-- team (new teams get them from the seed_builtin_issue_statuses trigger), then
-- anchor every existing issue's status_id. The VALUES rows mirror
-- contract.json's issueStatusDefaults (parity-locked by apps/web's
-- domain-contract test). ON CONFLICT DO NOTHING makes a re-run (or a race
-- with the seed trigger) harmless via uniq_issue_statuses_team_builtin.
INSERT INTO "issue_statuses" ("team_id", "category", "name", "color", "sort_order", "builtin_key")
SELECT t."id", d.category::"issue_status_category", d.name, d.color, d.sort_order, d.key::"issue_status"
FROM "teams" t
CROSS JOIN (VALUES
	('backlog', 'backlog', 'Backlog', '#A1A1AA', 1),
	('todo', 'unstarted', 'Todo', '#FAFAFA', 1),
	('in_progress', 'started', 'In Progress', '#EAB308', 1),
	('in_review', 'started', 'In Review', '#22C55E', 2),
	('done', 'completed', 'Done', '#3B82F6', 1),
	('cancelled', 'cancelled', 'Cancelled', '#A1A1AA', 1),
	('duplicate', 'duplicate', 'Duplicate', '#A1A1AA', 1)
) AS d(key, category, name, color, sort_order)
ON CONFLICT DO NOTHING;--> statement-breakpoint
DO $$
BEGIN
	-- Backfill WITHOUT stamping updated_at ("recently active" ordering and the
	-- done-group sort key off it). The update_updated_at trigger lives in the
	-- custom trigger file, so it only exists on databases that have booted at
	-- least once — a fresh DB migrates first (and has no issues to backfill).
	IF EXISTS (
		SELECT 1 FROM pg_trigger
		WHERE tgname = 'update_updated_at' AND tgrelid = 'issues'::regclass
	) THEN
		EXECUTE 'ALTER TABLE issues DISABLE TRIGGER update_updated_at';
	END IF;
	UPDATE issues i SET status_id = s.id
	FROM issue_statuses s
	WHERE s.team_id = i.team_id AND s.builtin_key = i.status AND i.status_id IS NULL;
	IF EXISTS (
		SELECT 1 FROM pg_trigger
		WHERE tgname = 'update_updated_at' AND tgrelid = 'issues'::regclass
	) THEN
		EXECUTE 'ALTER TABLE issues ENABLE TRIGGER update_updated_at';
	END IF;
END $$;
