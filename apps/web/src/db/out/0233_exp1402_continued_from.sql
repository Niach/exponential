ALTER TABLE "sessions" ADD COLUMN "continued_from_id" uuid;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_continued_from_id_sessions_id_fk" FOREIGN KEY ("continued_from_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
-- #EXP-1402: backfill the lineage of existing continuations (best effort): the
-- newest earlier ENDED sibling (or, for a moved root, the ended parent) that
-- the row copied branch and title from at insert.
UPDATE "sessions" AS s SET "continued_from_id" = (
  SELECT p."id" FROM "sessions" AS p
  WHERE p."root_id" = s."root_id" AND p."id" <> s."id" AND p."status" = 'ended'
    AND p."branch" = s."branch" AND p."title" = s."title"
    AND p."created_at" < s."created_at"
    AND (p."parent_id" = s."parent_id" OR s."parent_id" = p."id")
  ORDER BY p."created_at" DESC LIMIT 1
)
WHERE s."branch" IS NOT NULL AND s."parent_id" IS NOT NULL AND s."continued_from_id" IS NULL;
