-- #VAPP-260 (docs/spec/apps.md §2.2.2): the package format is version 5 (the
-- capability manifest `capabilities`). A version-4 package is a valid
-- version-5 package (no capabilities), so the stored heads, drafts and
-- versions move up instead of being refused at the next read.
UPDATE "apps" SET "package" = jsonb_set("package", '{formatVersion}', '5'::jsonb) WHERE "package"->>'formatVersion' = '4';--> statement-breakpoint
UPDATE "apps" SET "draft_package" = jsonb_set("draft_package", '{formatVersion}', '5'::jsonb) WHERE "draft_package"->>'formatVersion' = '4';--> statement-breakpoint
UPDATE "app_versions" SET "package" = jsonb_set("package", '{formatVersion}', '5'::jsonb) WHERE "package"->>'formatVersion' = '4';
