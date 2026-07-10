CREATE TABLE "issue_number_counters" (
	"project_id" uuid PRIMARY KEY NOT NULL,
	"counter" integer NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "issue_number_counters" ADD CONSTRAINT "issue_number_counters_project_id_projects_id_fk" FOREIGN KEY ("project_id") REFERENCES "public"."projects"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
-- Renumber pre-existing duplicate (project_id, number) rows created by the old
-- unlocked MAX+1 trigger so the unique index below can build on dirty DBs.
-- The oldest row per duplicate group keeps its number; later duplicates move
-- past the project's current max and get their identifier regenerated from
-- projects.prefix (the issues update_updated_at trigger bumps updated_at, so
-- Electric pushes the corrected identifiers to every client).
WITH ranked AS (
  SELECT id, project_id, created_at,
         row_number() OVER (PARTITION BY project_id, number ORDER BY created_at, id) AS rn
  FROM issues
), dupes AS (
  SELECT id, project_id,
         row_number() OVER (PARTITION BY project_id ORDER BY created_at, id) AS seq
  FROM ranked WHERE rn > 1
), maxes AS (
  SELECT project_id, MAX(number) AS max_number FROM issues GROUP BY project_id
)
UPDATE issues i
SET number = m.max_number + d.seq,
    identifier = p.prefix || '-' || (m.max_number + d.seq)
FROM dupes d
JOIN maxes m ON m.project_id = d.project_id
JOIN projects p ON p.id = d.project_id
WHERE i.id = d.id;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_issues_project_number" ON "issues" USING btree ("project_id","number");--> statement-breakpoint
-- Seed each project's monotonic counter at its post-dedup max so identifier
-- recycling protection starts immediately (before the first insert under the
-- new generate_issue_number()). ON CONFLICT keeps a re-run harmless.
INSERT INTO issue_number_counters (project_id, counter)
SELECT project_id, MAX(number) FROM issues GROUP BY project_id
ON CONFLICT (project_id) DO NOTHING;
