ALTER TABLE "sidebar_layout" DROP CONSTRAINT "sidebar_layout_section_check";--> statement-breakpoint
ALTER TABLE "boards" DROP CONSTRAINT "boards_repository_id_repositories_id_fk";
--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "slug" varchar(100) DEFAULT '';--> statement-breakpoint
-- #EXP-2171: the route key = the name's last segment, slugged (repositorySlug in
-- the populate_repository_slug trigger, custom/0001_triggers.sql); a clash inside a team takes the
-- whole owner/name, a clash left after that its id's first 8.
WITH base AS (
  SELECT id, team_id, created_at,
    coalesce(nullif(left(trim(both '-' from regexp_replace(lower(regexp_replace(full_name, '^.*/', '')), '[^a-z0-9._-]+', '-', 'g')), 100), ''), 'repo') AS short,
    coalesce(nullif(left(trim(both '-' from regexp_replace(lower(full_name), '[^a-z0-9._-]+', '-', 'g')), 100), ''), 'repo') AS long
  FROM "repositories"
), picked AS (
  SELECT id, team_id, created_at,
    CASE WHEN count(*) OVER (PARTITION BY team_id, short) = 1 THEN short ELSE long END AS slug
  FROM base
), ranked AS (
  SELECT id, slug, row_number() OVER (PARTITION BY team_id, slug ORDER BY created_at, id) AS n FROM picked
)
UPDATE "repositories" r
SET "slug" = CASE WHEN ranked.n = 1 THEN ranked.slug ELSE left(ranked.slug, 91) || '-' || left(r.id::text, 8) END
FROM ranked WHERE ranked.id = r.id;--> statement-breakpoint
ALTER TABLE "repositories" ALTER COLUMN "slug" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "description" text;--> statement-breakpoint
ALTER TABLE "repositories" ADD COLUMN "sync" jsonb DEFAULT '{"mode":"none"}'::jsonb NOT NULL;--> statement-breakpoint
-- #EXP-2171: the mirror settings leave provider_config (mirrorUrl, mirrorEnabled).
UPDATE "repositories"
SET "sync" = jsonb_build_object(
    'mode', CASE WHEN coalesce(provider_config->>'mirrorEnabled', 'true') <> 'false' THEN 'mirror' ELSE 'none' END,
    'url', trim(provider_config->>'mirrorUrl')),
  "provider_config" = provider_config - 'mirrorUrl' - 'mirrorEnabled'
WHERE coalesce(trim(provider_config->>'mirrorUrl'), '') <> '';--> statement-breakpoint
UPDATE "repositories" SET "provider_config" = provider_config - 'mirrorUrl' - 'mirrorEnabled'
WHERE provider_config ?| array['mirrorUrl', 'mirrorEnabled'];--> statement-breakpoint
ALTER TABLE "boards" ADD CONSTRAINT "boards_repository_id_repositories_id_fk" FOREIGN KEY ("repository_id") REFERENCES "public"."repositories"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "repositories" ADD CONSTRAINT "repositories_team_slug_unique" UNIQUE("team_id","slug");--> statement-breakpoint
ALTER TABLE "sidebar_layout" ADD CONSTRAINT "sidebar_layout_section_check" CHECK (section IN ('team', 'board', 'agent', 'app', 'repository'));