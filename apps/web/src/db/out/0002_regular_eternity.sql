-- Project = Repository (v4). Add projects.repository_id, backfill from the
-- soon-to-be-dropped project_repositories join, then drop the join table.
-- Hand-edited (see docs/masterplan.md §7) to be safe on an existing dev DB:
-- add NULLABLE → backfill (primary link, else sole link) → abort loudly if any
-- project is still unlinked → SET NOT NULL → drop project_repositories LAST.

--> 1. Add the column NULLABLE so the backfill can run.
ALTER TABLE "projects" ADD COLUMN "repository_id" uuid;--> statement-breakpoint

--> 2a. Backfill from the primary link.
UPDATE "projects" p
SET "repository_id" = pr."repository_id"
FROM "project_repositories" pr
WHERE pr."project_id" = p."id"
  AND pr."is_primary" = true
  AND p."repository_id" IS NULL;--> statement-breakpoint

--> 2b. Backfill from the sole link (projects with exactly one link, no primary).
UPDATE "projects" p
SET "repository_id" = pr."repository_id"
FROM "project_repositories" pr
WHERE pr."project_id" = p."id"
  AND p."repository_id" IS NULL
  AND (SELECT count(*) FROM "project_repositories" pr2 WHERE pr2."project_id" = p."id") = 1;--> statement-breakpoint

--> 3. Fail loudly (docs/masterplan.md §7): a project = a repository, so any
--> project that the 2a/2b backfills could not link must be resolved by the
--> operator (link it to a real repository, or delete it) before this migration
--> can enforce NOT NULL. Never fabricate/misattribute a repo on customer data.
DO $$
DECLARE
  unlinked int;
  offenders text;
BEGIN
  SELECT count(*) INTO unlinked FROM "projects" WHERE "repository_id" IS NULL;
  IF unlinked > 0 THEN
    SELECT string_agg(
             format('project %s (workspace %s)', p."id", p."workspace_id"),
             ', ' ORDER BY p."id"
           )
      INTO offenders
      FROM "projects" p
     WHERE p."repository_id" IS NULL;
    RAISE EXCEPTION
      'Cannot enforce projects.repository_id NOT NULL: % project(s) are not linked to a repository. Link each to a repository (or delete it), then re-run the migration. Offending: %',
      unlinked, offenders;
  END IF;
END $$;--> statement-breakpoint

--> 4. Enforce the FK + NOT NULL now that every project has a repository.
ALTER TABLE "projects" ADD CONSTRAINT "projects_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "projects" ALTER COLUMN "repository_id" SET NOT NULL;--> statement-breakpoint
CREATE INDEX "idx_projects_repository" ON "projects" USING btree ("repository_id");--> statement-breakpoint

--> 5. Drop the join table LAST.
ALTER TABLE "project_repositories" DISABLE ROW LEVEL SECURITY;--> statement-breakpoint
DROP TABLE "project_repositories" CASCADE;
