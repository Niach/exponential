-- Hand-edited (REV-4): board prefixes had no per-team uniqueness and ran up
-- to 10 chars, so two boards could both mint WEB-7 and identifier lookups
-- (reviews detail, #PREFIX-n mentions, exp/PREFIX-n branches) resolved an
-- arbitrary one. Cap prefixes at 4 chars and enforce (team_id, prefix)
-- uniqueness. Data first — the ALTERs below reject rows that still violate.
-- 1. Truncate longer prefixes to their first four characters.
UPDATE "boards" SET "prefix" = left("prefix", 4) WHERE char_length("prefix") > 4;--> statement-breakpoint
-- 2. Dedupe (team_id, prefix) collisions (pre-existing or minted by the
-- truncation): the oldest board keeps the prefix, later ones get the first
-- free `<stem><n>` that still fits in 4 chars.
DO $$
DECLARE
  dup RECORD;
  candidate varchar(4);
  n integer;
BEGIN
  FOR dup IN
    SELECT b.id, b.team_id, b.prefix
    FROM "boards" b
    WHERE EXISTS (
      SELECT 1 FROM "boards" older
      WHERE older.team_id = b.team_id AND older.prefix = b.prefix
        AND (older.created_at, older.id) < (b.created_at, b.id)
    )
    ORDER BY b.team_id, b.prefix, b.created_at, b.id
  LOOP
    n := 2;
    LOOP
      candidate := left(dup.prefix, 4 - length(n::text)) || n::text;
      EXIT WHEN NOT EXISTS (
        SELECT 1 FROM "boards"
        WHERE team_id = dup.team_id AND prefix = candidate
      );
      n := n + 1;
    END LOOP;
    UPDATE "boards" SET "prefix" = candidate WHERE id = dup.id;
  END LOOP;
END $$;--> statement-breakpoint
-- 3. Re-stamp identifiers on issues whose board prefix changed above, so
-- identifiers stay `{prefix}-{number}` and — with the constraint — unique per
-- team (duplicate WEB-7s would otherwise stay ambiguous forever).
UPDATE "issues" i SET "identifier" = b."prefix" || '-' || i."number"
FROM "boards" b
WHERE i."board_id" = b."id" AND i."identifier" <> b."prefix" || '-' || i."number";--> statement-breakpoint
ALTER TABLE "boards" ALTER COLUMN "prefix" SET DATA TYPE varchar(4);--> statement-breakpoint
ALTER TABLE "boards" ADD CONSTRAINT "boards_team_id_prefix_unique" UNIQUE("team_id","prefix");
