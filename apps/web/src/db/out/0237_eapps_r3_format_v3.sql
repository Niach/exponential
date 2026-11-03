-- eApps R3 (docs/spec/apps.md §3.4): the package format is version 3 (the
-- per-kind `policy`). A version-2 package is a valid version-3 package
-- without policies (the defaults are version 2's rules), so the stored
-- heads and versions move up instead of being refused at the next read.
UPDATE "apps" SET "package" = jsonb_set("package", '{formatVersion}', '3'::jsonb) WHERE "package"->>'formatVersion' = '2';--> statement-breakpoint
UPDATE "app_versions" SET "package" = jsonb_set("package", '{formatVersion}', '3'::jsonb) WHERE "package"->>'formatVersion' = '2';
