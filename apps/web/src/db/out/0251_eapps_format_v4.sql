-- #EXP-1674 (docs/spec/apps.md §2.2.1): the package format is version 4 (a
-- screen's `nav: false` and `exposed`). A version-3 package is a valid
-- version-4 package, so the stored heads, drafts and versions move up instead
-- of being refused at the next read. A package that still calls
-- harness.openIssue or harness.openDevice keeps opening; that press stops
-- working until the app is edited (decided in #EXP-1674).
UPDATE "apps" SET "package" = jsonb_set("package", '{formatVersion}', '4'::jsonb) WHERE "package"->>'formatVersion' = '3';--> statement-breakpoint
UPDATE "apps" SET "draft_package" = jsonb_set("draft_package", '{formatVersion}', '4'::jsonb) WHERE "draft_package"->>'formatVersion' = '3';--> statement-breakpoint
UPDATE "app_versions" SET "package" = jsonb_set("package", '{formatVersion}', '4'::jsonb) WHERE "package"->>'formatVersion' = '3';
