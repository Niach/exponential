-- eApps V1 (#VAPP-210, docs/spec/apps.md §2.3): draft vs published. `package`
-- is now the PUBLISHED version's package; `draft_package` = the head's while
-- the head is ahead. Every app that exists today stays live as it is: its
-- head becomes its published version.
ALTER TABLE "apps" ADD COLUMN "published_version" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "apps" ADD COLUMN "draft_package" jsonb;--> statement-breakpoint
UPDATE "apps" SET "published_version" = "head_version" WHERE "head_version" > 0;
